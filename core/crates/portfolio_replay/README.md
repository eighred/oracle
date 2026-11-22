# Point-in-time spot portfolio replay

The Rust command is the first consumer of the [synthetic BTC/USDT portfolio](fixtures/own_spot.json). Its ledger input is a dated opening position and cash balance, both owned by this fixture. The price source supplies closed one-minute bars and a later correction. The returned JSON carries the exact quote-currency value, selected observation ID, instrument revision, source digest, rights basis and the two query cutoffs.

From the Rust workspace:

```sh
cargo run --locked -p oracle-portfolio-replay -- 1767225660000 1767225660100
cargo run --locked -p oracle-portfolio-replay -- 1767225660000 1767225780000
cargo test --locked -p oracle-portfolio-replay
```

Arguments are UTC Unix milliseconds: effective time, knowledge cutoff, then an optional path to the exact approved dataset. The loader verifies the byte digest against its trusted fixture inventory. The only admitted source is the Eighred-authored synthetic dataset. A venue adapter must establish its own source identity and rights before its data can enter the financial read path.

For an effective instant, select the latest fully closed UTC minute and the newest revision of that minute received by the knowledge cutoff. An absent or unreceived bar yields incomplete coverage and a null NAV. The opening fixture has 2 BTC and 50 USDT. The original mark of 25 USDT gives 100 USDT NAV. The correction to 25.50 at the later knowledge cutoff gives 101 USDT on the same holdings. Neither figure represents actual market prices or booked profits.

Amounts accept unsigned base-10 strings with up to eight fractional places; quantity, volume, quote cash and prices retain their respective units. Inventory valuation uses a 128-bit product with up to sixteen decimal places. The addition of cash checks for overflow. The snapshot supports long spot holdings with a common quote currency. New instruments, trading flows, FX conversion and external rights require separate contracts and tests.

## Historical risk on the dated snapshot

From `core/`, run `cargo run --locked -p oracle-portfolio-replay -- --risk-bps 6250 1767225660000 1767225660100`. The report freezes the selected quantity and mark and applies four equal-weight daily returns from the [synthetic close index](fixtures/daily_risk_index.json). Cash earns zero over each scenario. The loss is negative change in quote-currency NAV. Each observation identifies the daily interval and the time its index values became known. Missing portfolio inputs, mismatched currency, a gap or insufficient history withhold the estimates.

VaR is the inverse empirical CDF. Expected shortfall averages the worst probability mass above the confidence cutoff and includes the fractional weight of a boundary observation. At 62.5% the 50 USDT spot exposure gives scenario losses of 0, 1, 2 and 9 USDT, VaR of 2 USDT and expected shortfall of 20/3 USDT. Values use exact rational strings in USDT. This four-scenario history is a test of the method. It supplies no measured forecast accuracy.

Run `python3 crates/portfolio_replay/tests/risk_reference.py` from `core/` to reprice all four scenarios independently against the CLI, including the subsequent price correction.

## Score a frozen daily forecast

From `core/`, run `cargo run --locked -p oracle-portfolio-replay -- --score-daily-risk-bps 6250 1767225660000 1767312060300`. The arguments select the confidence in basis points, the opening UTC minute close and the scoring knowledge cutoff. The fixed schedule freezes holdings, the last visible mark revision and all scenario inputs one second after the opening minute closes. Replaying at a later scoring cutoff preserves the forecast. This is a historical simulation on versioned synthetic data.

The next comparable valuation is exactly 24 hours after the opening close. The [separate outcome dataset](fixtures/next_day_spot.json) supplies the latest revision of that exact minute known when scoring. A score requires the same portfolio, instrument revision, venue, currencies, cash and quantity. With fixed spot holdings, fees, slippage, funding and external flows are zero. Missing observations or mismatched exposures leave the interval unscored. The report retains the scheduled and scored counts.

At 00:01 UTC on 1 January 2026 the frozen portfolio holds 2 BTC at 25 USDT and cash of 50 USDT. Its four daily scenarios give 2 USDT VaR and 20/3 USDT expected shortfall at 62.5%. At the 2 January 00:01 close, the first observed mark of 23.50 gives NAV of 97 USDT and a loss of 3 USDT, exceeding VaR. At scoring cutoff `1767312180000`, a revised mark of 24 gives NAV of 98 USDT. Its loss equals VaR, so the strict exceedance flag clears. These are two vintages of one outcome. The sample cannot estimate forecast accuracy, exception clustering or regime sensitivity.
