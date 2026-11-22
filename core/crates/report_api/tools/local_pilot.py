"""Run the synthetic workstation on loopback with a disposable PostgreSQL cluster."""
import argparse
import base64
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
from urllib.error import HTTPError
from urllib.request import Request, urlopen


@contextmanager
def api_server(binary, dist, env, port):
    with subprocess.Popen([binary, "serve", dist], stdout=subprocess.PIPE, text=True,
                          env={**env, "ORACLE_PORT": str(port)}) as server:
        try:
            line = server.stdout.readline().strip()
            if not line.startswith("Listening on 127.0.0.1:"):
                raise RuntimeError("API did not bind to loopback")
            yield "http://" + line.split()[-1]
        finally:
            server.terminate()
            server.wait(timeout=10)


def fetch(url, login=None):
    headers = {}
    if login:
        headers["Authorization"] = "Basic " + base64.b64encode(login.encode()).decode()
    try:
        with urlopen(Request(url, headers=headers), timeout=8) as result:
            return result.status, result.headers.get("x-oracle-report-id"), result.read()
    except HTTPError as error:
        return error.code, None, error.read()


def check_projection(origin, password, denied, expected, sql):
    latest = origin + "/api/portfolios/own-paper-01/reports/latest"
    login = "operator:" + password
    code, revision, body = fetch(latest, login)
    assert code == 200 and revision == expected == hashlib.sha256(body).hexdigest()
    payload = json.loads(body)
    assert payload["risk"]["value_at_risk"] == "2"
    assert payload["backtest"]["opening_snapshot"] == payload["risk"]["snapshot"]
    assert fetch(latest.replace("latest", revision), login) == (200, revision, body)
    assert fetch(latest)[0] == 401
    assert fetch(latest, "observer:" + denied)[0] == 403
    assert fetch(latest.replace("own-paper-01", "empty-pilot"), login)[0] == 404
    sql("UPDATE oracle_read.sessions SET expires_at=now()-interval '1 second' WHERE principal='operator'")
    assert fetch(latest, login)[0] == 401
    sql("UPDATE oracle_read.sessions SET expires_at=now()+interval '8 hours' WHERE principal='operator'")
    sql("UPDATE oracle_read.sessions SET revoked_at=now() WHERE principal='operator'")
    assert fetch(latest, login)[0] == 401
    sql("UPDATE oracle_read.sessions SET revoked_at=NULL WHERE principal='operator'")
    assert fetch(latest, login) == (200, revision, body)
    return body


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pg-bin", type=Path, required=True)
    parser.add_argument("--api-bin", type=Path, required=True)
    parser.add_argument("--dist", type=Path, required=True)
    parser.add_argument("--port", type=int, default=8040)
    parser.add_argument("--check", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    os.umask(0o077)
    if not (args.dist / "index.html").is_file():
        parser.error("build the Vue workstation first")
    migration = Path(__file__).resolve().parents[1] / "migrations/001_workstation.sql"
    with tempfile.TemporaryDirectory(prefix="oracle-pilot-") as folder:
        stage = Path(folder)
        pg = args.pg_bin
        data = stage / "db"
        subprocess.run([pg/"initdb", "-D", data, "-U", "pilot_admin", "--no-locale", "--encoding=UTF8",
                        "--auth-local=trust", "--auth-host=reject"], check=True, stdout=subprocess.DEVNULL)
        subprocess.run([pg/"pg_ctl", "-D", data, "-l", stage/"postgres.log", "-o",
                        f"-c listen_addresses='' -k '{stage}'", "-w", "start"], check=True)
        try:
            def sql(statement):
                subprocess.run([pg/"psql", "-Xq", "-v", "ON_ERROR_STOP=1", "-h", stage, "-U",
                                "pilot_admin", "-d", "postgres"], input=statement, text=True, check=True)
            sql(migration.read_text())
            password = secrets.token_hex(32)
            def session(user, token):
                key = hashlib.sha256(token.encode()).hexdigest()
                sql(f"INSERT INTO oracle_read.sessions VALUES ('{key}','{user}',now()+interval '8 hours',NULL)")
            sql("CREATE ROLE pilot_reader LOGIN IN ROLE oracle_report_reader;"
                "CREATE ROLE pilot_writer LOGIN IN ROLE oracle_report_writer;"
                "INSERT INTO oracle_read.portfolio_grants VALUES ('operator','own-paper-01')")
            session("operator", password)
            def database_env(user, database="postgres"):
                return {**os.environ, "ORACLE_DATABASE_URL": f"host='{stage}' user={user} dbname={database}"}
            def publish():
                return subprocess.check_output([args.api_bin, "publish-synthetic"],
                                               env=database_env("pilot_writer"), text=True).strip()
            revision = publish()
            with api_server(args.api_bin, args.dist, database_env("pilot_reader"), args.port) as origin:
                if args.check is None:
                    keyfile = stage/"operator.password"
                    keyfile.write_text(password)
                    print(f"{origin} · sign in as operator using {keyfile} · press Enter to stop", flush=True)
                    input()
                    return
                assert publish() == revision
                denied = secrets.token_hex(32)
                session("observer", denied)
                sql("INSERT INTO oracle_read.portfolio_grants VALUES ('operator','empty-pilot')")
                # Run direct SQL as the restricted reader on one connection. The API gate
                # would otherwise mask an accidentally unrestricted table.
                sql("INSERT INTO oracle_read.reports(portfolio,report_id,payload) "
                    "SELECT 'other-paper',encode(sha256('other'::bytea),'hex'),'other'::bytea")
                key = hashlib.sha256(password.encode()).hexdigest()
                sql(f"""
                    SET ROLE pilot_reader;
                    BEGIN;
                    SET LOCAL oracle.operator='operator';
                    SET LOCAL oracle.token_digest='{key}';
                    DO $$ BEGIN
                        IF (SELECT count(*) FROM oracle_read.reports) <> 1 OR
                           EXISTS(SELECT FROM oracle_read.reports WHERE portfolio='other-paper')
                        THEN RAISE EXCEPTION 'portfolio isolation failed'; END IF;
                    END $$;
                    COMMIT;
                    DO $$ BEGIN
                        IF EXISTS(SELECT FROM oracle_read.reports) OR
                           has_table_privilege(current_user,'oracle_read.sessions','SELECT')
                        THEN RAISE EXCEPTION 'reader scope leaked'; END IF;
                    END $$;
                """)
                original = check_projection(origin,password,denied,revision,sql)
                if args.check:
                    subprocess.run(args.check, check=True, env={**os.environ,"ORACLE_TEST_ORIGIN":origin,
                                   "ORACLE_TEST_PASSWORD":password, "ORACLE_DENIED_PASSWORD":denied})
            # Restore the database to a second database on the isolated cluster.
            # Global roles stay in the cluster. Read the restored bytes through a fresh API connection.
            backup = stage/"pilot.dump"
            common = ["-h",stage,"-U","pilot_admin"]
            subprocess.run([pg/"pg_dump",*common,"-Fc","-f",backup,"postgres"],check=True)
            subprocess.run([pg/"createdb",*common,"restored"],check=True)
            subprocess.run([pg/"pg_restore",*common,"--exit-on-error","-d","restored",backup],check=True)
            with api_server(args.api_bin,args.dist,database_env("pilot_reader","restored"),0) as restored:
                route = "/api/portfolios/own-paper-01/reports/" + revision
                assert fetch(restored+route,"operator:"+password) == (200,revision,original)
                assert fetch(restored+route,"observer:"+denied)[0] == 403
            print("Verified publication, scoped reads, revocation and restored report bytes.", flush=True)
        finally:
            subprocess.run([pg/"pg_ctl","-D",data,"-m","fast","-w","stop"],check=True)


if __name__ == "__main__":
    main()
