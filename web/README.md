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

Use `--observe-ccxt-fixture` to exercise the same screen with candles read through CCXT Rust. **Paper outcomes** names the venue source, shows the opening valuation and one-minute risk estimates, and links each mark to its receipt digest under **Market data receipts**.

To observe the paper ledger, add `--observe-paper` to the local pilot command. It freezes a future nine-sample window and records fresh synthetic prices once per UTC minute. At the end, open **Paper outcomes** to inspect the benchmark, net returns, costs and settlement rows or download the immutable JSON. The workstation identifies the source and shows missing coverage for a stopped experiment. With `--check`, the same command also interrupts the disposable PostgreSQL server mid-run, checks idempotent redelivery, verifies each settlement independently and exercises browser access to the restored report. Allow about ten minutes for that mode.

From `web/`, after building both components:

```sh
npx --no-install playwright install chromium
python3 ../core/crates/report_api/tools/local_pilot.py \
  --pg-bin "$(pg_config --bindir)" --api-bin ../core/target/debug/oracle-report-api \
  --dist dist --port 0 --check npm run test:browser
```

The harness exercises publication, immutable retrieval, two principals on the same server connection, transaction scope cleanup, direct PostgreSQL row policies, expiry and revocation. It restores a database backup and compares authorized response bytes and denied access. Browser checks cover the published NAV, selected strategy, fees, benchmark, report digest, downloads, keyboard operation and a 375 px viewport. Loading and service-error rendering use synthetic HTTP responses. The historical portfolio view has no live subscription or execution entrypoint.
## Consumer app

Discover opens a market table with Popular, Gainers and Losers filters. The browser reads Oracle’s cached market directory, groups matching base and quote symbols and lets members search across the eligible universe. The interface and member API omit provider names. Select a row to request a paper quote or start a basket. The Community and News tabs show trader activity and publisher headlines. The mobile dock opens Discover, Baskets and Wallet. The operator workspace is available at `/#overview`. [Design reference](DESIGN.md)

Members can star a pair in Discover and open `/#discover?tab=watchlist` to reorder or remove it. The server stores the list by account. A missing market price leaves the saved pair visible. Returning to the app reloads the list, and conflicting edits show the other device's saved order.

The manifest supports standalone installation from a secure web origin. The service worker handles an offline navigation with a reconnect screen. Portfolio balances, account data and financial actions use the network. All device shells load the same packaged Vue and TypeScript assets.

### Android and iOS

Use Node 22 or later and install the web dependencies from the lockfile. From this directory:

```sh
npm ci --ignore-scripts
npm run mobile:sync
npx --no-install cap open android
npx --no-install cap open ios
```

The native identifier is `com.eighred.oracle`. Use Android Studio with JDK 21 and Android SDK 36, or Xcode on macOS with an iOS simulator. The mobile CI workflow produces a debug APK, an unsigned Android App Bundle and an unsigned iPhone device archive. It also builds and launches the simulator app. Store distribution requires the publisher's signing and store accounts.

For a local Android pilot, run the API on port 8040 and connect the test device with `adb reverse tcp:8040 tcp:8040`. The iOS simulator uses its Mac host's loopback. For a test deployment, set `VITE_ORACLE_API_ORIGIN` to the approved HTTPS server origin before building. The native client sends credentials only to that origin through the HTTP bridge and keeps the signed-in session in memory. Verified members open their wallets with email or username and an authenticator. The scoped operator login serves the separate operator pilot. For a loopback member test on Android, also forward the issuer port with `adb reverse tcp:8180 tcp:8180`. Paper venues and journals are served by the Rust backend.

For member registration, email delivery and mandatory authenticator setup, follow the [member identity runbook](../core/identity/README.md).
