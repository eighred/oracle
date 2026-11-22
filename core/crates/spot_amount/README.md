# Spot amounts

The market adapter and portfolio replay share one unsigned decimal representation for spot quantities and prices. Both callers supply the instrument and currency. The type accepts at most eight decimal places. Valuation multiplies integer atoms at sixteen decimal places and checks the addition of quote cash for overflow. Parsing and arithmetic have no persistent state or recovery dependency.

Run `cargo test --locked -p oracle-spot-amount` from the core workspace.
