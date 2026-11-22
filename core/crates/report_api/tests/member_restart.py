"""Verify member paper balances survive a full restart of the persistent Linux launcher."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import tempfile
from urllib.request import Request, urlopen
from uuid import uuid4
from contextlib import contextmanager


@contextmanager
def workspace(args, state):
    script = Path(__file__).resolve().parents[1] / "tools/member_workspace.py"
    command = [sys.executable, script, "--pg-bin", args.pg_bin, "--api-bin", args.api_bin, "--dist", args.dist,
               "--state-dir", state, "--port", "0"]
    env = {**os.environ, "ORACLE_OIDC_ISSUER": "http://127.0.0.1:8180/realms/oracle"}
    with subprocess.Popen(command, env=env, stdout=subprocess.PIPE, text=True) as server:
        try:
            for line in server.stdout:
                if line.startswith("Oracle paper workspace at "):
                    yield line.split()[-1]
                    return
            raise RuntimeError("Member workspace failed to start")
        finally:
            server.send_signal(signal.SIGTERM)
            server.wait(timeout=15)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ["pg-bin", "api-bin", "dist"]:
        parser.add_argument("--" + option, type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="oracle-member-restart-") as folder:
        state = Path(folder)
        token = secrets.token_urlsafe(32)
        digest = hashlib.sha256(token.encode()).hexdigest()
        handle = "@restart_test"
        headers = {"Authorization": "Basic " + base64.b64encode((handle + ":" + token).encode()).decode(),
                   "Content-Type": "application/json"}

        def read(origin):
            with urlopen(Request(origin + "/api/profile", headers=headers), timeout=6) as response:
                return json.load(response)

        with workspace(args, state) as origin:
            socket = (state / "db/postmaster.pid").read_text().splitlines()[4]
            # The real provider path is exercised separately by the SMTP and authenticator browser check.
            subprocess.run([args.pg_bin / "psql", "-h", socket, "-U", "workspace_owner", "-d", "postgres",
                            "-Xq", "-v", "ON_ERROR_STOP=1"], input=f"""SET ROLE oracle_report_reader;
                SELECT oracle_member.issue('https://restart.invalid','test','restart_test','{digest}',
                    floor(extract(epoch FROM now()))::bigint+180,'restart-session');""",
                           text=True, stdout=subprocess.DEVNULL, check=True)
            data = json.dumps({"op": "deposit", "request": str(uuid4()), "currency": "TRY", "amount": "125.25"}).encode()
            with urlopen(Request(origin + "/api/baskets", data=data, headers=headers), timeout=6) as response:
                assert response.status == 200
            before = read(origin)
            assert before["principal"] == handle and before["balances"][0]["available"] == "125.25"
        with workspace(args, state) as origin:
            assert read(origin) == before
        print("Verified member identity binding, money and activity after full workspace restart.")


if __name__ == "__main__":
    main()
