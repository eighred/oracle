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
