# Oracle portfolio workstation

The Vue 3 client reads one authorized Eighred portfolio from the [report API](../core/crates/report_api/README.md). Portfolio review shows opening holdings, one-day risk and minute-close account values. Open Strategy comparison to compare net returns and drawdown, then follow View settlements to the selected strategy’s ledger. Choose a minute to inspect its modeled fill, fees, cash posting, and realized and unrealized P&L. The toolbar’s Report provenance control opens the source digests and the immutable download. Times are UTC.

## Workspace navigation and icons

Each workspace has a direct URL fragment. Desktop navigation collapses to an icon rail. On small screens the same links open in a navigation drawer. Settlement and provenance dialogs support Escape and return keyboard focus to the invoking control.

The [icon sprite](src/assets/openai-icons.svg) contains eleven glyphs from OpenAI’s MIT-licensed `@openai/apps-sdk-ui` 0.2.2. Each symbol preserves the published component’s viewBox, ordered paths and fill rules. The viewBox and every path’s drawing commands matched the installed Windows Codex 26.1111.2228.0 assets. The Vue client references the sprite directly. The distribution includes the [upstream license](public/licenses/openai-apps-sdk-ui.txt). [OpenAI Apps SDK UI source](https://github.com/openai/apps-sdk-ui) · [Versioned package](https://www.npmjs.com/package/@openai/apps-sdk-ui/v/0.2.2).

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

To observe the paper ledger, add `--observe-paper` to the local pilot command. It freezes a future nine-sample window and records fresh synthetic prices once per UTC minute. At the end, open **Paper outcomes** to inspect the benchmark, net returns, costs and settlement rows or download the immutable JSON. The workstation identifies the source and shows missing coverage for a stopped experiment. With `--check`, the same command also interrupts the disposable PostgreSQL server mid-run, checks idempotent redelivery, verifies each settlement independently and exercises browser access to the restored report. Allow about ten minutes for that mode.

From `web/`, after building both components:

```sh
npx --no-install playwright install chromium
python3 ../core/crates/report_api/tools/local_pilot.py \
  --pg-bin "$(pg_config --bindir)" --api-bin ../core/target/debug/oracle-report-api \
  --dist dist --port 0 --check npm run test:browser
```

The harness exercises publication, immutable retrieval, two principals on the same server connection, transaction scope cleanup, direct PostgreSQL row policies, expiry and revocation. It restores a database backup and compares authorized response bytes and denied access. Browser checks cover the published NAV, selected strategy, fees, benchmark, report digest, downloads, keyboard operation and a 375 px viewport. Loading and service-error rendering use synthetic HTTP responses. The historical portfolio view has no live subscription or execution entrypoint.
