"""Exercise the configured scanner against clean and synthetic committed history."""

import argparse
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scanner", default="gitleaks")
    scanner = shutil.which(parser.parse_args().scanner)
    if not scanner:
        raise SystemExit("Gitleaks was not found")

    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        def git(*args):
            subprocess.run(["git", "-c", "user.name=Oracle Test", "-c",
                            "user.email=test@example.invalid", "-c", "commit.gpgsign=false",
                            *args], cwd=repo, check=True, capture_output=True)

        def scan(expected):
            result = subprocess.run([scanner, "git", ".", "--redact", "--no-banner",
                                     "--exit-code", "42", "--config", str(ROOT / ".gitleaks.toml"),
                                     "--log-opts=--all --full-history"], cwd=repo, capture_output=True)
            if result.returncode != expected:
                raise SystemExit(f"Scanner returned {result.returncode}, expected {expected}")

        git("init", "-b", "main")
        fixture = repo / "fixture.txt"
        fixture.write_text("ordinary fixture\n", encoding="utf-8")
        git("add", "fixture.txt")
        git("commit", "-m", "Clean baseline")
        scan(0)
        fixture.write_text("test_token=ghp_" + "R8pK6xW2" + "sT3fQ9mZ" + "1uD7vN4c" + "Ja0EyL5g" + "VbHo" + "\n", encoding="utf-8")
        git("add", "fixture.txt")
        git("commit", "-m", "Synthetic detector fixture")
        git("rm", "fixture.txt")
        git("commit", "-m", "Remove fixture from tip")
        scan(42)
    print("Gitleaks accepted clean history and detected the removed synthetic token.")


if __name__ == "__main__":
    main()
