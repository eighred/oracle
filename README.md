# Eighred Oracle

Oracle is Eighred's internal research and risk workstation. The first delivery should let an Eighred operator evaluate one own-account strategy from dated inputs through a reproducible backtest, cost-adjusted paper portfolio and browser report. It must work without customers, subscriptions or funded exchange credentials.

Start with the [architecture and quantitative charter](docs/architecture/oracle.md), the [coding-agent handoff](docs/engineering/ai_delivery_prompt.md) and the [workspace runbook](docs/engineering/workspace.md). The [program index](https://github.com/eighred/oracle/issues/13) maps the 70 design candidates to their acceptance owners. The foundation provides engineering governance, Oracle Forge and the module-update receiver. Product delivery is tracked by issue and revision.

## Local development

Run `cargo run --locked -p oracle-backtest` from `core/` for the held-out BTC/USDT comparison. The [backtest guide](core/crates/backtest/README.md) identifies the benchmark, fill clock, costs and independent ledger checks.

The financial core lives in `core/`. Its first command replays a dated synthetic BTC/USDT portfolio with exact arithmetic. From `core/`, run `cargo run --locked -p oracle-portfolio-replay -- 1767225660000 1767225660100`. The [spot replay guide](core/crates/portfolio_replay/README.md) explains the two cutoffs, source provenance, correction and missing-price behavior. The [market adapter guide](core/crates/market_data/README.md) covers the read-only CCXT command and offline HTTP verification.

From the repository root, with Git and Python 3.11 or later:

```sh
python -B tools/check_foundation.py doctor
python -B tools/check_foundation.py verify
```

[Contributing](CONTRIBUTING.md) covers the full local check sequence. See the [engineering contract](AGENTS.md), [security policy](SECURITY.md) and [Oracle Forge guide](.agents/plugins/oracle-forge/README.md). The designated hosted destination is private `eighred/oracle`. [Hosting requirements](.github/governance/README.md) describe the controls to verify there.

The [license](LICENSE.md) is the exact Eighred license file at [the specified Kanz revision](https://github.com/eighred/kanz/blob/e8998af758d38ded6c25e853ef21e451e0fe425b/LICENSE.md). External distribution and the terms applicable to private contributions require owner and legal review.
