"""Run the synthetic workstation on loopback with a disposable PostgreSQL cluster."""
import argparse
import base64
from getpass import getpass
from contextlib import contextmanager, nullcontext, ExitStack
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from fixture_venue import fixture_venue


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
    parser.add_argument("--basket-curator", action="store_true", help="allow the local operator to pin official basket templates")
    parser.add_argument("--platform-admin", action="store_true", help="allow the local operator to manage simulated company settings")
    sources = parser.add_mutually_exclusive_group()
    sources.add_argument("--observe-paper", action="store_true",
                        help="freeze a nine-minute synthetic experiment and publish its observed outcome")
    sources.add_argument("--observe-ccxt-fixture", action="store_true",
                         help="read newly generated loopback candles with CCXT Rust for nine minutes")
    parser.add_argument("--check", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    os.umask(0o077)
    if not (args.dist / "index.html").is_file():
        parser.error("build the Vue workstation first")
    migration = Path(__file__).resolve().parents[1] / "migrations/001_workstation.sql"
    with (fixture_venue() if args.observe_ccxt_fixture else nullcontext((None, {}))) as (fixture_port, candles), tempfile.TemporaryDirectory(prefix="oracle-pilot-") as folder:
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
            sql(migration.with_name("002_paper.sql").read_text())
            sql(migration.with_name("003_source_scopes.sql").read_text())
            sql(migration.with_name("004_baskets.sql").read_text())
            sql(migration.with_name("005_platform_admin.sql").read_text())
            sql(migration.with_name("006_basket_paper.sql").read_text())
            sql(migration.with_name("007_member_identity.sql").read_text())
            sql(migration.with_name("008_spot_tickets.sql").read_text())
            sql(migration.with_name("009_community.sql").read_text())
            sql(migration.with_name("010_basket_venue_policy.sql").read_text())
            sql(migration.with_name("011_watchlist.sql").read_text())
            if args.check is not None:
                sql((migration.parents[1] / "tests/spot_ticket.sql").read_text())
            if args.platform_admin or args.check is not None:
                sql("INSERT INTO oracle_admin.administrators VALUES ('operator')")
            if args.basket_curator or args.check is not None:
                sql("INSERT INTO oracle_social.curators VALUES ('operator')")
            password = (secrets.token_hex(32) if args.check is not None
                        else getpass("Choose a temporary operator passphrase (16–256 UTF-8 bytes): "))
            if not 16 <= len(password.encode()) <= 256:
                raise ValueError("passphrase must contain 16–256 UTF-8 bytes")
            def session(user, token):
                salt = secrets.token_bytes(16)
                key = hashlib.pbkdf2_hmac("sha256", token.encode(), salt, 600_000).hex()
                sql(f"INSERT INTO oracle_read.sessions VALUES ('{key}','{user}',decode('{salt.hex()}','hex'),now()+interval '8 hours',NULL)")
                return key
            sql("CREATE ROLE pilot_reader LOGIN IN ROLE oracle_report_reader;"
                "CREATE ROLE pilot_basket LOGIN IN ROLE oracle_basket_worker;"
                "CREATE ROLE pilot_writer LOGIN IN ROLE oracle_report_writer;"
                "CREATE ROLE pilot_paper LOGIN IN ROLE oracle_paper_operator;"
                "INSERT INTO oracle_read.portfolio_grants VALUES ('operator','own-paper-01')")
            key = session("operator", password)
            def database_env(user, database="postgres"):
                config = {**os.environ, "ORACLE_DATABASE_URL": f"host='{stage}' user={user} dbname={database}"}
                if fixture_port:
                    config["ORACLE_FIXTURE_PORT"] = str(fixture_port)
                return config
            def publish():
                return subprocess.check_output([args.api_bin, "publish-synthetic"],
                                               env=database_env("pilot_writer"), text=True).strip()
            revision = publish()
            def paper(action, window, *indices, role="pilot_paper"):
                raw = subprocess.check_output([args.api_bin, "paper", action, str(window), *map(str, indices)],
                                              env=database_env(role), text=True)
                return json.loads(raw)
            paper_revision = None
            observe = args.observe_paper or args.observe_ccxt_fixture
            if observe or args.check is not None:
                first = (time.time_ns() // 60_000_000_000 + 2) * 60_000
                freeze = "freeze-ccxt-fixture" if fixture_port else "freeze"
                manifest = paper(freeze, first)
                assert paper(freeze, first) == manifest
                if observe:
                    print(f"Paper window frozen. First sample at {first} UTC milliseconds.", flush=True)
                    for index in range(manifest["samples"]):
                        due = (first + index * manifest["cadence_ms"]) / 1000 + 0.5
                        time.sleep(max(0, due - time.time()))
                        event = paper("sample", first, index)
                        print(f"Paper sample {index}: {event['result']}", flush=True)
                        if event["result"] == "missed":
                            break
                        if index == 4:
                            if args.check is not None:
                                subprocess.run([pg/"pg_ctl", "-D", data, "-m", "immediate", "-w", "stop"], check=True)
                                subprocess.run([pg/"pg_ctl", "-D", data, "-l", stage/"postgres.log", "-o",
                                                f"-c listen_addresses='' -k '{stage}'", "-w", "start"], check=True)
                            assert paper("sample", first, index)["result"] == "already_recorded"
                else:
                    assert paper("cancel", first) == paper("cancel", first)
                observed = paper("report", first)
                if fixture_port and observed["state"] == "complete":
                    assert len(candles) == len(observed["source_receipts"]) == 9
                    for receipt, sample in zip(observed["source_receipts"], observed["samples"], strict=True):
                        raw = candles[receipt["open_ms"]]
                        assert receipt["response_json_sha256"] == hashlib.sha256(
                            json.dumps(raw, separators=(",", ":")).encode()).hexdigest()
                        assert receipt["received_ms"] == sample["known_ms"]
                if observe and observed["state"] == "complete":
                    audit = Path(__file__).resolve().parents[2]/"backtest/tests/paper_reference.py"
                    export = stage/"paper.json"
                    export.write_text(json.dumps(observed), encoding="utf-8")
                    subprocess.run(["python3", audit, export], check=True)
                paper_revision = paper("publish", first, role="pilot_writer")["report_id"]
                assert paper("publish", first, role="pilot_writer")["report_id"] == paper_revision
            if args.check is not None:
                recovery = Path(__file__).resolve().parents[1] / "tests/paper_wallet_recovery.sql"
                sql(f"BEGIN; SET LOCAL oracle.operator='operator'; SET LOCAL oracle.token_digest='{key}';"
                    + recovery.read_text() + "ROLLBACK;")
            with api_server(args.api_bin, args.dist, database_env("pilot_reader"), args.port) as origin, ExitStack() as stack:
                worker = subprocess.Popen([args.api_bin, "basket-worker" if args.check is None else "basket-worker-fixture"],
                                          env=database_env("pilot_basket"))
                stack.callback(worker.wait, timeout=10)
                stack.callback(worker.terminate)
                if args.check is None:
                    print(f"{origin} · sign in as operator with your temporary passphrase · press Enter to stop", flush=True)
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
                sql(f"""
                    SET ROLE pilot_reader;
                    BEGIN;
                    SET LOCAL oracle.operator='operator';
                    SET LOCAL oracle.token_digest='{key}';
                    DO $$ BEGIN
                        IF (SELECT count(*) FROM oracle_read.reports WHERE kind='historical') <> 1 OR
                           has_schema_privilege(current_user,'oracle_paper','USAGE') OR
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
                paper_route = "/api/portfolios/own-paper-01/paper/"
                paper_code, paper_id, paper_body = fetch(origin+paper_route+"latest","operator:"+password)
                assert paper_code == 200 and paper_id == paper_revision == hashlib.sha256(paper_body).hexdigest()
                assert json.loads(paper_body) == observed
                assert fetch(origin+paper_route+paper_revision,"observer:"+denied)[0] == 403
                assert fetch(origin+paper_route+revision,"operator:"+password)[0] == 404
                # Exercise source eligibility independently of portfolio access. This test never reads a venue.
                window = (time.time_ns() // 60_000_000_000 + 3) * 60_000
                refused = subprocess.run([args.api_bin, "paper", "freeze-binance", str(window)],
                                         env=database_env("pilot_paper"), capture_output=True, text=True)
                assert refused.returncode and "missing active BTCUSDT" in refused.stderr
                sql("INSERT INTO oracle_paper.binance_scopes(evidence_ref,allowed_until) VALUES ('local permission test',now()+interval '1 hour')")
                venue = paper("freeze-binance", window)
                assert venue["source_kind"] == "ccxt_binance_spot"
                assert paper("freeze-binance", window+60_000)["source_scope_id"] == venue["source_scope_id"]
                paper("cancel", window)
                scoped = paper("publish", window, role="pilot_writer")["report_id"]
                assert fetch(origin+paper_route+scoped,"operator:"+password)[0] == 200
                sql("UPDATE oracle_paper.binance_scopes SET revoked=true WHERE evidence_ref='local permission test'")
                assert fetch(origin+paper_route+scoped,"operator:"+password)[0] == 404
                assert fetch(origin+paper_route+"latest","operator:"+password)[1] == paper_revision
                if args.check:
                    subprocess.run(args.check, check=True, env={**os.environ,"ORACLE_TEST_ORIGIN":origin,
                                   "ORACLE_TEST_PASSWORD":password, "ORACLE_DENIED_PASSWORD":denied})
                basket_catalogs = {user: fetch(origin+"/api/baskets",user+":"+token) for user,token in (("operator",password),("observer",denied))}
                profiles = {user: fetch(origin+"/api/profile",user+":"+token) for user,token in (("operator",password),("observer",denied))}
                watchlists = {user: fetch(origin+"/api/v1/me/watchlist",user+":"+token) for user,token in (("operator",password),("observer",denied))}
                assert all(result[0] == 200 for result in profiles.values())
                assert all(result[0] == 200 for result in basket_catalogs.values())
            # Restore the database to a second database on the isolated cluster.
            # Global roles stay in the cluster. Read the restored bytes through a fresh API connection.
            backup = stage/"pilot.dump"
            common = ["-h",stage,"-U","pilot_admin"]
            subprocess.run([pg/"pg_dump",*common,"-Fc","-f",backup,"postgres"],check=True)
            subprocess.run([pg/"createdb",*common,"restored"],check=True)
            subprocess.run([pg/"pg_restore",*common,"--exit-on-error","-d","restored",backup],check=True)
            with api_server(args.api_bin,args.dist,database_env("pilot_reader","restored"),0) as restored:
                for user,token in (("operator",password),("observer",denied)):
                    assert fetch(restored+"/api/profile",user+":"+token) == profiles[user]
                    assert fetch(restored+"/api/v1/me/watchlist",user+":"+token) == watchlists[user]
                    assert fetch(restored+"/api/baskets",user+":"+token) == basket_catalogs[user]
                route = "/api/portfolios/own-paper-01/reports/" + revision
                assert fetch(restored+route,"operator:"+password) == (200,revision,original)
                assert fetch(restored+route,"observer:"+denied)[0] == 403
                assert fetch(restored+paper_route+paper_revision,"operator:"+password) == (200,paper_revision,paper_body)
            print("Verified publication, scoped reads, revocation and restored report bytes.", flush=True)
        finally:
            subprocess.run([pg/"pg_ctl","-D",data,"-m","fast","-w","stop"],check=True)


if __name__ == "__main__":
    main()
