# Contributing to Oracle

Read the [engineering contract](AGENTS.md) and [security policy](SECURITY.md). Start a focused branch from the current main and associate the change with an acceptance criterion. Use a synthetic case that would have failed before the change. For financial calculations, choose an independent reference and state the unit, cutoff and tolerance before running the implementation.

Use the [workspace runbook](docs/engineering/workspace.md) for full-source checkout, compartmentalized assignments, contract migration and module pin proposals. Run the Forge package validator as well as the repository checks below:

```sh
python -B .agents/plugins/oracle-forge/scripts/validate.py --tracked
python -B .agents/plugins/oracle-forge/scripts/provenance.py
python -B -m unittest discover -s .agents/plugins/oracle-forge/scripts -p 'test_*.py' -v
```

## Run locally

Use Git, Python 3.11+ and Gitleaks 8.30.1. The Python checks use the standard library. Run from the repository root. The Git scan covers committed history. Scan the working directory separately before staging sensitive changes.

```sh
python -B tools/check_foundation.py verify
python -B -m unittest discover -s tests -v
gitleaks dir . --redact --no-banner
gitleaks git . --redact --no-banner --log-opts="--all --full-history"
python -B tools/check_secret_scan.py
```

The self-test uses a disposable repository with synthetic data. GitHub Actions also runs CodeQL for the languages represented in this foundation. The [hosting guide](.github/governance/README.md) lists required job names and private-repository prerequisites.

Open a PR with the [review template](.github/pull_request_template.md). Report completed and skipped checks and the exact tested commit. Keep unreleased vulnerability details in the private reporting channel. See the [Forge guide](.agents/plugins/oracle-forge/README.md) for supported installation, upgrades and rollback.
