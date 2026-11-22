# Oracle portfolio workstation

The Vue 3 client reads one authorized Eighred portfolio from the [report API](../core/crates/report_api/README.md). Opening holdings, one-day risk, strategy returns, drawdown and all ledger amounts come from the published Rust report. Choose a strategy to inspect its settlements, then choose a minute for the modeled fill, fees, cash posting, and realized and unrealized P&L. Expand report provenance to download the exact revision. The canvas renders account values; the ledger provides the values by timestamp. Times are UTC and the historical source is labeled on the page.

## Eighred palette

The [workstation stylesheet](src/style.css) defines the Eighred sRGB palette. Each token uses the most frequent decoded color in the corresponding flat region of the supplied AVIF. Black, off-white and gray form the reading surface. Pink identifies momentum, blue the hold benchmark and teal periodic buys. Green highlights opening NAV and orange marks the archived observation and keyboard focus. The chart and its legend read the same tokens.

## Run locally

Use Linux or WSL with the checked-in Rust toolchain, Node 22.12 or newer, npm, Python 3.10 or newer and PostgreSQL server tools on the same host. From the repository root:

```sh
(cd core && cargo build --locked -p oracle-report-api)
(cd web && npm ci --ignore-scripts && npm run build)
python3 core/crates/report_api/tools/local_pilot.py \
  --pg-bin "$(pg_config --bindir)" --api-bin core/target/debug/oracle-report-api --dist web/dist
```

Run the helper as a regular OS user. Choose a temporary passphrase at the masked terminal prompt. The helper starts a disposable database in a private directory, publishes the synthetic report and prints its loopback address. Sign in as `operator` with that passphrase. Press Enter in the terminal to stop the processes and remove this disposable state. The server serves the built web assets and the API from the same origin. Credentials are absent from the JavaScript bundle.

## Verify

From `web/`, after building both components:

```sh
npx --no-install playwright install chromium
python3 ../core/crates/report_api/tools/local_pilot.py \
  --pg-bin "$(pg_config --bindir)" --api-bin ../core/target/debug/oracle-report-api \
  --dist dist --port 0 --check npm run test:browser
```

The harness exercises publication, immutable retrieval, two principals on the same server connection, transaction scope cleanup, direct PostgreSQL row policies, expiry and revocation. It restores a database backup and compares authorized response bytes and denied access. Browser checks cover the published NAV, selected strategy, fees, benchmark, report digest, downloads, keyboard operation and a 375 px viewport. Loading and service-error rendering use synthetic HTTP responses. The historical portfolio view has no live subscription or execution entrypoint.
