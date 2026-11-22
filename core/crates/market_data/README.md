# Public spot market data

The `oracle-market-data` command requests one completed UTC minute from the Binance market-data-only host. It sends a single public spot kline GET through CCXT Rust. The operator supplies the UTC open timestamp in milliseconds. The venue, symbol, method, interval and URL are fixed. The command has no credential, order or arbitrary endpoint argument.

From the core workspace, exercise the adapter against the local synthetic HTTP venue:

```sh
cargo test --locked -p oracle-market-data
```

For an authorized venue location and data use, the equivalent live read is:

```sh
cargo run --locked -p oracle-market-data -- 1767225600000
```

Standard output contains the normalized observation, original parsed response array, SHA-256 of its compact JSON encoding and local receipt timestamp. The digest covers parsed JSON and preserves decimal strings. Whitespace and transport bytes are outside its scope. The open and close times come from the venue. The close timestamp is inclusive. Source rights are marked unreviewed. Financial replay accepts its own reviewed source manifest. Retaining venue output or admitting it to valuation requires the applicable source rights and instrument rules.

A missing minute, timeout, rejected response or throttle returns a nonzero exit code. The command makes one attempt and emits no replacement price. Recovery consists of a later read for the same minute after the venue permits it. A different response digest at that point is a separate observation. This diagnostic has no persistent cursor or writer.

The [paper journal](../report_api/README.md) calls the same Rust reader directly. Each observation feeds valuation, one-minute risk and simulated settlement. Replay reconstructs it from the saved parsed response without calling the venue again. The local integration sends the same CCXT request to an Eighred fixture on numeric loopback and identifies that origin separately in every report.

The lockfile pins CCXT, its engine and common runtime to 4.5.93. The published crates identify source commit `8fc542412690abe753f98cb00694e5703fcd8fde`. The Binance and OKX exchange features are selected. The local HTTP test checks the emitted request, including its public path, fixed query and absence of authentication headers. Two prices separated by one atom at 100 billion USDT detect accidental conversion through binary floating point. [CCXT release source](https://github.com/ccxt/ccxt/tree/8fc542412690abe753f98cb00694e5703fcd8fde/rust) · [Binance public market data](https://developers.binance.com/en/docs/products/spot/rest-api)

## Selected spot instruments

The Spot portfolio risk workstation calls the market adapter with a venue, native BASE/QUOTE, UTC window and expected minute count. The operator can enter any currently active spot pair returned by the venue for those native asset names. Binance supplies symbol identity and filters through exchangeInfo. OKX supplies spot instrument state, tick and lot size through public instruments. The adapter checks the returned base and quote before requesting prices. It accepts UTF-8 asset names and decimal prices smaller than 0.00000001.

The Binance adapter checks kline open and inclusive close times. OKX history-candles uses an exclusive upper cursor, returns newest first and marks closed candles with confirm=1. Both adapters require the exact requested set of completed minutes and produce an ascending panel. Missing or extra rows, inconsistent OHLC, suspended instruments and incomplete candles withhold the result. Metadata describes rules at retrieval time. The report records the window, each venue receipt time and digests of the parsed metadata and candle responses. The workstation serves derived fields only. [OKX public instruments and market data](https://www.okx.com/docs-v5/en/) · [Binance Spot REST API](https://developers.binance.com/en/docs/products/spot/rest-api).
