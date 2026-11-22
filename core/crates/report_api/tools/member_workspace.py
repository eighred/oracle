"""Serve verified members with a persistent loopback paper ledger on Linux."""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import os
import re
from pathlib import Path
import signal
import subprocess
import tempfile
import time
from local_pilot import api_server


@contextmanager
def postgres(pg, root):
    data = root / "db"
    if not data.exists():
        with tempfile.TemporaryDirectory(dir=root) as staging:
            fresh = Path(staging) / "db"
            subprocess.run([pg / "initdb", "-D", fresh, "-U", "workspace_owner", "--no-locale",
                            "--encoding=UTF8", "--auth-local=trust", "--auth-host=reject"], check=True,
                           stdout=subprocess.DEVNULL)
            fresh.rename(data)
    # The owner-only socket directory is independent of the long-lived state path.
    with tempfile.TemporaryDirectory(prefix="oracle-socket-") as socket:
        subprocess.run([pg / "pg_ctl", "-D", data, "-l", root / "postgres.log", "-o",
                        f"-c listen_addresses='' -k '{socket}'", "-w", "start"], check=True)
        try:
            yield socket
        finally:
            subprocess.run([pg / "pg_ctl", "-D", data, "-m", "fast", "-w", "stop"], check=True)


def migrate(pg, socket, staff):
    def sql(statement):
        result = subprocess.run([pg / "psql", "-XAtq", "-v", "ON_ERROR_STOP=1", "-h", socket,
                                 "-U", "workspace_owner", "-d", "postgres"],
                                input=statement, text=True, check=True, stdout=subprocess.PIPE)
        return result.stdout.strip()

    sql("CREATE TABLE IF NOT EXISTS public.workspace_migrations (name text PRIMARY KEY, digest text NOT NULL);"
        "REVOKE ALL ON public.workspace_migrations FROM PUBLIC;")
    folder = Path(__file__).resolve().parents[1] / "migrations"
    for migration in sorted(folder.glob("[0-9][0-9][0-9]_*.sql")):
        body = migration.read_text(encoding="utf-8").strip()
        name = migration.name
        digest = hashlib.sha256(body.encode()).hexdigest()
        existing = sql(f"SELECT digest FROM public.workspace_migrations WHERE name='{name}'")
        if existing:
            if existing != digest:
                raise RuntimeError(f"Applied migration differs from this checkout: {name}")
            continue
        if not body.startswith("BEGIN;") or not body.endswith("COMMIT;"):
            raise ValueError(f"Expected a transactional migration in {name}")
        sql(body[:-len("COMMIT;")] + f"INSERT INTO public.workspace_migrations VALUES('{name}','{digest}'); COMMIT;")
    sql("""DO $$ BEGIN
        IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='workspace_reader') THEN
            CREATE ROLE workspace_reader LOGIN IN ROLE oracle_report_reader;
        END IF;
        IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='workspace_worker') THEN
            CREATE ROLE workspace_worker LOGIN IN ROLE oracle_basket_worker;
        END IF;
    END $$;""")
    for handle, table in staff:
        if not handle:
            continue
        if not re.fullmatch(r"@[a-z0-9_]{3,32}",handle):
            raise ValueError("Use a verified @handle for the role assignment")
        if sql(f"SELECT 1 FROM oracle_member.members WHERE handle='{handle}'") != "1":
            raise ValueError("Complete member registration and first sign-in before assigning a role")
        sql(f"INSERT INTO {table}(principal) VALUES('{handle}') ON CONFLICT DO NOTHING")



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("pg-bin", "api-bin", "dist", "state-dir"):
        parser.add_argument("--" + arg, type=Path, required=True)
    parser.add_argument("--port", type=int, default=8040)
    parser.add_argument("--platform-admin", metavar="@HANDLE", help="assign platform settings to an existing verified member")
    parser.add_argument("--basket-curator", metavar="@HANDLE", help="assign publication review to an existing verified member")
    args = parser.parse_args()
    if os.getuid() == 0:
        parser.error("Run as the local workspace service user")
    if not os.environ.get("ORACLE_OIDC_ISSUER"):
        parser.error("Set ORACLE_OIDC_ISSUER to the member realm")
    os.umask(0o077)
    root = args.state_dir.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    if root.stat().st_uid != os.getuid() or root.stat().st_mode & 0o077:
        parser.error("The state directory must be owned by this user and have mode 0700")
    stopping = False

    def stop(signum, frame):
        nonlocal stopping
        stopping = True

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    with (root / "workspace.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        with postgres(args.pg_bin, root) as socket:
            migrate(args.pg_bin, socket, [(args.platform_admin, "oracle_admin.administrators"),
                                          (args.basket_curator, "oracle_social.curators")])

            def environment(role):
                return {**os.environ, "ORACLE_DATABASE_URL": f"host='{socket}' dbname=postgres user={role}"}

            with subprocess.Popen([args.api_bin, "basket-worker"], env=environment("workspace_worker")) as worker:
                try:
                    with api_server(args.api_bin, args.dist, environment("workspace_reader"), args.port) as origin:
                        print(f"Oracle paper workspace at {origin}", flush=True)
                        while not stopping:
                            if worker.poll() is not None:
                                raise RuntimeError("Paper allocation worker stopped")
                            time.sleep(0.5)
                finally:
                    worker.terminate()
                    worker.wait(timeout=10)


if __name__ == "__main__":
    main()
