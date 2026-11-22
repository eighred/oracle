# Costed spot backtest

From `core/`, run `cargo run --locked -p oracle-backtest`. Use `-- --output report.json` to save the report. Repeating the command accepts an identical file. Concurrent writers publish a complete temporary file with create-only semantics. A conflicting report is preserved. Process restart replays the bounded input from its beginning. Published reports are local research artifacts.

The command opens the [dated portfolio](../portfolio_replay/README.md) at its original 25 USDT mark with 50 USDT cash and 2 BTC. The frozen [synthetic close series](fixtures/spot_minutes.json) has three warm-up observations and six held-out valuations. At each step the strategy sees the known prefix. A decision made at receipt can fill at the following minute close if that instant is strictly later. Settlement and reporting use the arrival time of the fill observation. A gap, duplicate or late signal prevents a complete run. No parameters are fitted or selected on the holdout. Funding and external flows are zero.

| Rule | Decision |
|---|---|
| Hold | Keep the original cash and BTC throughout the holdout. This is the benchmark. |
| DCA | Buy 0.25 BTC using existing cash on alternating held-out settlements, beginning at the first. |
| Momentum | Buy 0.25 BTC on an increase between the two latest known closes, sell 0.25 BTC on a decrease and hold on a tie. |

All three rules use the same long-only cash ledger. A fill uses the next reference mark with a two-basis-point half-spread and three basis points of adverse slippage. Round the resulting execution price outward to a 0.01 USDT tick. Charge ten basis points on execution notional, rounded up to one quote atom. The fill exposes spread, slippage and tick costs already embedded in that price. Orders require funded cash or inventory, exact quantity increments, representable quote notional and at least 1 USDT notional. The model assumes the fixed lot is available at the modeled execution price. It has no depth, partial fills, stops or limits.

Each ledger row identifies the signal and settlement observations, postings, balances and valuation. Simulation-period realized P&L uses average cost with capitalized buy fees and the opening mark as the reference basis for initial inventory. Realized plus unrealized P&L reconciles to the change in NAV. Turnover divides absolute traded notional by starting NAV. Maximum drawdown uses the opening NAV and each subsequent minute-close NAV. Exact recurring results use rational strings. The report retains the source digest, opening snapshot, model, rules, fee schedule, warm-up and holdout bounds and ledger digest. Its deterministic rules use no random seed.

Run `python3 crates/backtest/tests/accounting_reference.py` from `core/` for an independent reconstruction of every held-out balance, fee, turnover and drawdown. The historical run measures accounting behavior on synthetic inputs. Prospective paper results require a separately frozen window and actual observations.
