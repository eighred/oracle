# Eighred Oracle architecture and quantitative delivery charter

**Decision baseline · 22 November 2026 · EOR-ARCH-001 implementation companion**

Eighred's first operator needs to answer a specific question: after realistic costs, can a predeclared strategy on one digital-asset portfolio be reproduced, explained and evaluated prospectively? A loss, an inconclusive result or a refused calculation can each be the correct answer. The internal pilot ends when an operator can establish which answer the evidence supports. The long-term product extends that workflow to research users, then to institutional portfolios and additional asset classes.

This specification develops the owner-supplied *EIGHRED ORACLE AI MASTER ARCHITECTURE V2.1*, SHA-256 `2101d5bafe9eccceea4374bdc74532e5140f5f22822040e9e7582405ab8c48ce`. Its design identifiers remain traceable to their GitHub owners. The repository foundation at `81e5dcf` supplies governance, CI and Forge. Product interfaces below describe implementation targets. Issue acceptance and evidence on a delivered revision establish operational status. [Delivery index and all 70 candidates](https://github.com/eighred/oracle/issues/13)

## Pilot scope and investment mandate

| Decision | First delivery | Evidence before expanding |
|---|---|---|
| User and portfolio | One Eighred operator, one synthetic own-account portfolio and one quote currency. No customer is needed. | A second principal exercises denied access. External pilots need approved data rights, an identified user and recorded UAT. |
| Market | One licensed or otherwise permitted public spot feed. First qualification candidate is Binance BTC/USDT. | Complete instrument and method matrix, historical coverage and permitted use per venue. OKX has a separate qualification. |
| Decision | Review a dated risk estimate, compare a frozen strategy to buy-and-hold, then observe paper performance. | Independent arithmetic, deterministic replay and prospective observations. A positive return alone cannot promote a model. |
| Authority | Read market data and persist research and simulated postings in the authorized environment. | Any future capital decision records the human principal, venue, amount, loss limit, validity and revocation path. |
| Distribution | Original responsive Vue workstation in the private Oracle project. | Client-facing use requires brand review, market-data display rights, authorization controls and applicable vendor agreements. |

Benchmark Oracle first against its own recorded baseline using identical data, instruments, hardware, precision and time windows. Measure reconciliation error, p95 risk report latency, incomplete-data detection time, cost per completed report and operator time from an anomaly to its explanation. A claim about Aladdin requires a permitted, comparable external measurement. Record no comparison when coverage or reference results differ. [Aladdin Risk product scope](https://www.blackrock.com/aladdin/platforms/products/aladdin-risk)

## Runtime ownership and trust

| Owner | Reads | Writes and failure boundary |
|---|---|---|
| Rust market adapter | Selected CCXT REST and WebSocket observations, venue rules and rights metadata | Normalized market events and quality state. It may pause a source on a gap. It has no order authority in the pilot. |
| Rust portfolio and risk | Dated cash, positions, eligible marks, FX and approved model version | Reconciled snapshots and independently versioned risk runs. A missing required mark makes coverage incomplete. |
| Rust simulation | Frozen strategy, event stream, instrument and cost rules, starting ledger | Deterministic simulated orders, fills, postings and outcomes. Historical and prospective runs have different run identities. |
| Rust/Axum API | Server-authenticated principal, scoped portfolio and report projections | Authorized reads and audited research requests. Validation resolves the portfolio at the persistence boundary. |
| Vue 3 + strict TypeScript | Versioned read API and market subscriptions | Local interaction and presentation. Chart calculations cannot become ledger entries. |
| Python research / optional Julia verification | Versioned, minimum-necessary research extract | Candidate studies and independent checks. A reviewer controls model registration and promotion. |
| PostgreSQL | Typed commands from the owning writer | Transactions commit authoritative money, scope, run metadata and outbox records. An analytics replica may be rebuilt from durable evidence. |

Start with domain modules and a small number of processes. A new deployment needs a concrete trust, recovery or capacity reason. Use Tokio for asynchronous venue and API work. Keep valuation and simulation functions callable with recorded inputs and injected time. PostgreSQL supplies the first persistent boundary. Add object storage for licensed immutable history when retention and size justify it. Consider ClickHouse for derived analytics and Arrow/Parquet for research interchange once there is a real reader. If messaging is introduced, name one publisher and ordering owner for each event family. An outbox and its redelivery policy must survive a process crash.

The authoritative flow is **venue observation → qualified market data → reconciled snapshot → risk or simulation run → immutable report → scoped operator view**. Each arrow identifies one producer, a versioned input and an observable failure result. Keep exchange credentials and venue-specific objects inside the adapter. MCP, model inference and browser sessions do not participate in exchange connectivity.

## Market data and point-in-time truth

Select reviewed matching CCXT Rust versions, disable default exchange features, enable only the chosen venue per crate and commit the lockfile. REST handles instrument discovery and bounded backfill. WebSocket handles subscribed observations. Public methods are the first slice. Demonstrate the actual method on the intended venue and validate timestamp, precision and completeness before declaring support. An upstream method name does not establish venue behavior. [Reviewed upstream Rust workspace](https://github.com/ccxt/ccxt/tree/d49e7aa7ba2d5a0bd8bcc9d32a8eb5ca5d0e37c1/rust)

Each normalized observation carries venue, native market ID, canonical instrument revision, event time, local receipt time, source sequence when available, payload version, quality and provenance. Record rights separately for internal computation, retention, display and redistribution. Prefer synthetic fixtures in Git. Keep restricted raw evidence in its approved store, referencing it by digest and scope. Public endpoint access alone establishes no redistribution right.

| Condition | Adapter behavior | Downstream evidence |
|---|---|---|
| Rate limit or venue maintenance | Respect venue backoff, cap retries and expose the last successful watermark. | Operator sees a delayed source. No fabricated zero-volume candles. |
| Reconnect or duplicate | Deduplicate by stable source identity. Resume from the last accepted watermark and request a bounded gap fill. | Replaying the interval twice yields one event per revision and the same snapshot. |
| Missing sequence or unrecoverable gap | Record the interval and the limits of reconstruction. Pause calculations requiring full coverage. | Reports identify missing instruments, affected time range and withheld result. |
| Late correction | Create a new observation revision with receipt time and predecessor. | An earlier knowledge cutoff resolves the original observation; a rerun at a later cutoff can use the correction. |
| Symbol collision or relisting | Identify venue, market type, base/quote, native ID and validity interval. Resolve tick, quantity step and minimum notional at event time. | A venue ticker string cannot silently join a relisted instrument to old terms. |

An initial market fixture can use two bar revisions for the same minute. At 10:01 UTC a closed bar reports 100. At 10:05 the venue corrects the close to 101. A report known at 10:02 must still use 100 after the correction arrives. A report known at 10:06 may use 101 and records its new source digest. Reject a bar with an impossible OHLC range. Use explicit UTC intervals and distinguish the currently forming bar from a closed observation.

## Contracts, storage and scope

| Contract | Minimum fields | Invariant |
|---|---|---|
| Instrument revision | Venue, immutable market identity, effective interval, base/quote, precision and quantity rules | Aliases and human labels may change independently of identity. |
| Portfolio snapshot | Portfolio, revision, valid-at, known-at, reporting currency, signed positions, cash, selected prices and FX | Every balance and price has a unit and source. A missing amount and an observed zero remain distinguishable. |
| Risk run | Snapshot ID, model version, scenario manifest, horizon, confidence, sign convention, coverage and result digest | A result is meaningful only with its convention, window and inputs. |
| Strategy run | Strategy version, parameters, seed, dataset digest, benchmark, decision clock, fill assumptions and costs | Replaying the same canonical input reproduces the ordered ledger and result. |
| Paper posting | Run, instruction, fill identity, sequence, asset, signed quantity/cash, fee currency and decision time | One fill identity posts once. Reconciliation explains both sides and rounding. |
| Immutable report | Input and run references, artifact digest, creation time, quality, supersession and access scope | Revisions preserve the original result. Every displayed amount resolves to evidence. |

Use fixed-point or decimal arithmetic for booked amounts and exchange increments. Define scale, rounding point, currency, lot and fee conventions at the boundary. Serialize exact quantities as decimal strings; transmit statistical estimates with their method, numerical tolerance and units. Tag valid time and known time separately. Sort equal-time events using a persisted deterministic tie-breaker. A process timestamp during replay cannot decide trade order.

The server resolves identity from an authenticated session. A portfolio ID from a URL is a requested resource. Within the transaction, scope the restricted database role to the principal and enforce row-level access. Check pagination, count, export, subscriptions and long-lived jobs under the same scope. After reuse of a pooled connection, verify the previous tenant scope is gone. Avoid exposing whether a foreign portfolio exists. Record principal, requested action, policy version, decision and resource in a restricted audit trail. Show a safe correlation ID to the browser.

A paper posting and its outbox event commit in one transaction with a unique idempotency key. If a worker dies before commit, retry from the durable cursor. If it dies after commit and before acknowledgment, a retry finds the existing posting. Conflicting content for the same source identity requires reconciliation. Corrections append a new revision linked to its predecessor.

## Valuation, risk and independent reference cases

For spot inventory, value each signed position at its qualified mark, convert the position value from quote to reporting currency at the same cutoff, sum once, then add cash and subtract liabilities. Keep inventory, reserved cash, available cash and collateral distinct. A USDT mark needs an approved USDT/USD rate to produce a USD result. Futures, options and inverse contracts each need their own payoff and collateral models. Extend coverage instrument by instrument.

**Valuation fixture.** At a USD mark of 25, two units and USD 50 in cash yield USD 100 NAV. With unchanged holdings, a 10% spot shock produces USD 95 NAV and a USD 5 loss. If USD 10 is deposited immediately after that shock, the new NAV is USD 105. The market result remains a USD 5 loss. Show the deposit as an external flow. Withhold the USD result when the only price is in another currency and the required FX mark is unavailable.

**Tail-risk fixture.** Use loss equal to negative P&L. For ordered, equally weighted one-day losses of 0, 1, 2 and 9 units, the inverse empirical CDF at 62.5% gives VaR of 2. The worst 37.5% tail contains all 25% mass at 9 and 12.5% mass at 2. ES is therefore (0.25 × 9 + 0.125 × 2) / 0.375 = 20/3. This small fixture exposes quantile interpolation or tail-boundary errors. It has no statistical power as a forecast. Record the confidence level, effective sample size, weighting and horizon with a production-like result.

| Method | Validation requirement | Refusal or qualification |
|---|---|---|
| Historical VaR and ES | Use aligned, dated return scenarios. Test fractional boundary mass, ties, all-gain samples and a hedged exposure against a separately derived reference. | Missing required factors or insufficient observations produce a coverage result. Preserve any display-only rounding. |
| Frozen risk forecast | Save the forecast and holdings before the evaluation interval. Compare with the next aligned hypothetical unchanged-holdings P&L, and separately with booked P&L. | A subsequent correction creates a new evaluation revision. Future marks cannot change the original forecast. |
| Stress and attribution | Reprice a coherent joint shock. Reconcile signed contributions to the total under the declared approximation. Include FX and material liquidity assumptions. | Unsupported shocks remain explicitly unpriced. Quantify approximation error before presenting a nonlinear portfolio as a linear estimate. |
| Drawdown and performance | Define peak, trough, cash-flow treatment and valuation frequency. Show realized, unrealized and external flows separately. | Missing intervals and incomplete valuations affect coverage. Deposits cannot reset losses invisibly. |

Choose tolerances before inspecting the result. Require exact booked amounts at the specified scale; use a justified absolute or relative tolerance for statistical output. Store reference inputs and derivation independently of the candidate implementation. Report model and data uncertainty, including the short tail history common to many instruments.

## Backtest, paper ledger and research lifecycle

Freeze strategy version, universe, eligible intervals, cost assumptions, benchmark and decision clock before a holdout run. A closing price becomes usable after the interval closes and the source is known. The earliest simulated fill must follow the decision and respect available market evidence. Specify tick size, lot, minimum notional, available funds, maker/taker model and fee currency. A bar that crosses a stop and limit without revealing event order requires a conservative rule declared in advance or an explicit indeterminate result. A missing bar cannot provide a fill.

**Costed round trip.** Begin with USD 1,000. Buy two units at the simulated execution price of USD 100 and pay USD 0.20 fees. Cash becomes USD 799.80. Sell both at USD 105 and pay USD 0.21. Final cash is USD 1,009.59, inventory is zero and net realized P&L is USD 9.59. The fills already include their modeled execution prices. Charging the same spread or slippage again is a failure. For a leveraged instrument, separately justify funding and financing. A spot cash fixture marks these inapplicable.

| Run | Inputs and decision boundary | Evidence to retain |
|---|---|---|
| Historical benchmark | Fixed buy-and-hold and a periodic contribution baseline; record contribution timing. | Exact cash-flow schedule, measured return and independent ledger. |
| Historical strategy | Frozen parameters, time-ordered fit/validation/test segments, fixed costs and deterministic tie-breaks. | Every intent, refusal, simulated fill, cost, posting, drawdown and turnover; held-out comparison including losing periods. |
| Prospective paper | Declare the evaluation start/end, cadence and stop conditions before new observations arrive. Simulate fills with the qualified model as new events become known. | Timestamped decisions, observed gaps, reconciliation after restart, daily ledger and benchmark comparison. Backfilled history cannot become prospective evidence. |
| Own capital | Separate future process, mandate, credentials, limit engine, human authority, expiry and recovery evidence. | A signed go/no-go decision with named reviewer. Paper acceptance can also lead to a recorded hold. |

A simulation state change follows proposed → admitted or rejected → open → partially filled, filled, expired or canceled. Use immutable event IDs for transitions. Test cancel/fill ordering, duplicate fills, insufficient cash, missing tick rules and restart between fill and posting. Use the same valuation and cost conventions in backtest and paper runs. Label where the input clocks or liquidity assumptions differ. Publish a ledger summary that an operator can reconcile line by line to starting and closing NAV.

Research workers consume a versioned immutable extract with a schema and digest. Fit transforms on training data only. Purge overlapping labels and apply embargo where the horizon overlaps. Preserve an untouched final holdout, finite search budget and full trial history. Record failed and timed-out trials with reasons. Start with a reproducible local baseline, then qualify Optuna resumption and parameter sensitivity. Use ablation and a null control to interpret rule contribution. Correlated features can distribute importance across rules; disclose that limitation. Block-bootstrap scenarios preserve the declared temporal dependence. Distributed Ray workers require measured throughput need, bounded resources, reproducible results and permission isolation. A model registry records candidate, reviewer, evidence, approved uses, rollback target and revocation. ML or AI cannot authorize execution.

## Original Vue workstation and chart datafeed

Use the owner's Eighred palette across the workstation and chart legends. The [shared color tokens](../../web/src/style.css) record the flat sRGB colors sampled from the approved visual reference. Preserve the black ground, white and gray text, and pink, blue, teal, green and orange accents. Pair colored data series with names, line treatments and readable settlement values.

Follow the Codex desktop layout for navigation. Keep one selected workspace in the center, collapse the sidebar to icons on desktop, and open it as a drawer on mobile. Show posting details and report provenance on demand. Use the verified OpenAI Apps SDK UI glyphs documented in the [web workstation](../../web/README.md) and retain Oracle's own product mark.

The primary route is portfolio selection → dated overview → risk run → strategy comparison → fill, price and posting detail. Keep the selected portfolio, as-of time, base currency, benchmark and data-quality state visible as the operator drills down. A stale timestamp remains visible after reconnection until gaps are reconciled. Distinguish a zero position, a pending calculation, an incomplete result and a denied request. Provide table equivalents for visual information, keyboard navigation, focus restoration, reduced motion and a narrow-screen layout.

| Read surface | Minimum request and response | Test through the actual browser |
|---|---|---|
| Portfolio snapshot | Portfolio ID, effective/knowledge cutoff; return snapshot revision, valuation, currency, coverage and source references. | The same report ID resolves to the same displayed NAV and drill-down on desktop and mobile. |
| Risk and strategy result | Immutable run ID and allowed portfolio; return method, assumptions, benchmark, net metrics, status and traceable components. | Trace one displayed loss to contributing positions and a net strategy result to fees and postings. Browser calculations cannot alter the stored amounts. |
| Bars and streaming updates | Canonical instrument revision, resolution and time range; return ascending UTC bars with completion status, revision and watermark. Resume with an opaque scoped cursor. | Compare prices/volume to a known fixture, replace the last forming bar correctly, recover a gap, terminate a revoked subscription. |
| Report export | Scoped snapshot/run ID and the requested format; return the approved projection and integrity reference. | Reject a foreign or revoked artifact. Verify the authorized export agrees with the web view at the same cutoffs. |

Use OpenTrade to study generic workflows such as watchlists, account selection and activity review. Design the screens from Oracle operator tasks. Write original Vue components, navigation, interaction text and assets. Keep upstream code, Electron renderer, MCP and branding outside the implementation. Review any separately chosen dependency and its notices on its own terms. [Upstream license and restrictions](https://github.com/OpenTradeOSS/OpenTrade/blob/7e3572b416a6ba4d54bc2c662d475154d82cb958/LICENSE)

**Chart decision.** Advanced Charts is the intended licensed renderer. Start with a provider-independent Oracle datafeed and use Lightweight Charts under its applicable terms and attribution while private-use rights are pending. When historical and live fixture bars pass and integration into the private screen becomes the next task, notify the project owner in the chart issue. Provide the product description, internal operator audience, future paid audience, intended web/desktop hosting, data sources, attribution, distribution and upgrade questions. Obtain written scope and official library access before importing the proprietary bundle. Record market display rights independently. Keep licensed bundles in the authorized distribution channel. TradingView supplies the rendering library; Oracle supplies qualified data. [License scope](https://www.tradingview.com/charting-library-docs/latest/introduction/) · [Official distribution](https://www.tradingview.com/charting-library-docs/latest/quick-start/) · [Datafeed API](https://www.tradingview.com/charting-library-docs/latest/connecting_data/Datafeed-API/)

Later views can add hypothetical basket construction, stored drawings and multi-venue comparison on the same contracts. Watchlist preferences and drawings are scoped per identity. Basket preview computes allocation, estimated fees and rounding residual without creating an external transaction. Electron and optional mobile shells consume the web application once their IPC, credential storage, code signing and update paths have independent evidence.

## Workspace, contracts and source protection

Develop product modules inside the private `eighred/oracle` checkout. A change to a risk field and both its server and Vue consumers belongs in one PR with all affected tests. A full-source engineer can work throughout the checkout with the minimum necessary repository role. Grant repository administration separately from source access. The first actual assignment requiring partial visibility triggers a distinct private module repository, independent Git history and a fixed gitlink in the parent. Moving a directory later leaves older snapshots in existing history. Review historical exposure before granting access and keep sensitive new work in its approved compartment from inception.

The parent owns composition, consumer tests and the approved revision. The module owns its sources, contract fixtures, independent build and PR. A module contributor uses synthetic data and a scoped interface package. A full-source engineer works in the module directory inside the Oracle checkout. A module merge proposes a gitlink-only parent PR; protected integration acceptance promotes it. A breaking API change proceeds by adding a compatible version, migrating consumers, proving both versions during the transition and retiring the old contract after usage is clear. A rollback pins a known compatible revision and verifies persisted-data compatibility. [Workspace and access runbook](../engineering/workspace.md)

| Control owner | Evidence required |
|---|---|
| Repository and identity owners | Inspect effective organization base rights and direct/team/app access. Verify a module-only identity cannot list, clone, download, read logs or fetch artifacts for Oracle or sibling modules. Revoke and retest with fresh credentials; distinguish already held copies. |
| Endpoint and network security | In an approved VDI, run a permitted development task and attempt synthetic transfers over each governed channel: USB, clipboard, print, external mail, cloud storage and unmanaged downloads. Record device policy, redacted event IDs, exception expiry and delivery to the responsible reviewer. |
| Security operations | Restrict telemetry access and retention. Test a synthetic bulk-download anomaly and an authorized bulk build. Measure time to alert, disposition and escalation, including false positives. Human review owns personnel decisions. |
| Legal and HR | Determine applicable confidentiality, IP assignment, data handling, monitoring notice and lawful remedies by jurisdiction. Record approval and expiry in the proper private system. Keep privileged advice and employee records out of issues. |

Keep valuable algorithms on controlled servers, separate build and runtime identities, and distribute only approved interfaces or signed artifacts. Obfuscation can raise reverse-engineering effort for a distributed component. It cannot revoke already disclosed source. Private visibility and CODEOWNERS have distinct roles. Revalidate effective access before adding any compartmentalized contributor. [Security policy](../../SECURITY.md)

## Operations, commercial boundaries and staged delivery

**Reliability.** Monitor feed age, gaps, duplicate rate, rejected observations, incomplete valuations, run duration, paper reconciliation drift, outbox backlog and access-denial rates. A stale feed can allow historical report reads while pausing new dependent calculations. Test backup restoration with a restricted database role, saved manifests and immutable observation history. Compare snapshot and ledger digests before and after recovery. Record actual recovery point and time, environment and lost interval. Archive redacted logs with the tested revision. A local fixture proves its own failure case; a production recovery claim requires a separately witnessed drill.

**Cost.** Track CI minutes, artifact retention, active security licenses, market-data rights, storage and cost per completed run. Benchmark Tokyo and alternatives on the same permitted endpoints before selecting a deployment. Include regional restrictions, p95/p99 latency, outages, egress and restore cost. Keep only necessary exchange features in the build. Introduce extra brokers, replicas, GPUs or distributed optimizers when a measured capacity constraint and reproducible accuracy comparison justify them. Keep the principal repository private throughout development.

**Product and finance.** Separate cash receipts for subscriptions, eligible client incentive fees and Eighred own-account realized P&L. Research performance, simulated NAV and proposed fees never constitute collected revenue. An ordinary self-directed subscriber and Eighred's own account cannot incur a client performance fee. A future eligible managed agreement identifies jurisdiction, consent, eligible net-gain basis, fees already included, effective/known times, hurdle, high-water mark, currency, independent approver and dispute path. Administration proposes future versions and preserves old statements.

For a synthetic single-lot check, take opening NAV USD 10,000, pre-fee closing NAV USD 11,200, previous HWM USD 10,000 and agreed hurdle amount USD 300 with no intervening flows. Under an illustrative threshold of USD 10,300 the eligible excess is USD 900. A shadow 15% schedule computes USD 135 and post-fee NAV of USD 11,065. A later 10% schedule cannot rewrite this statement. A recovery to USD 11,000 below a USD 11,065 HWM has no new eligible excess. Additional subscriptions, lot equalization, FX changes, withdrawals and reversals require independently reviewed accounting. Actual crystallization, custody and collection each require express authority.

**Administrative content.** A later content editor may draft, preview, schedule, retire and roll back a versioned banner or dismissible pop-up. Enforce audience, locale, time zone, safe content, accessible focus and publication scope at the server. Trace each change to a principal and revision. Content permissions confer no portfolio or billing rights.

| Gate | Evidence for acceptance | Outcome if a dependency is missing |
|---|---|---|
| Qualified inputs | Approved source/rights manifest, actual supported read, timestamped recorded fixture, gap and duplicate controls. | Continue offline validation and disclose the unqualified live source. |
| Reconciled financial core | Independent cash/position references, dated FX, quantile/stress goldens, restricted database and scoped API tests. | Publish incomplete coverage and the exact missing input. |
| Reproducible strategy | Fixed benchmark, independent round trip, untouched holdout, failure controls, repeatable ledger digest. | Retain the failed run and its assumptions. |
| Operator and paper pilot | Browser provenance drill-down, prospective paper period declared in advance, actual market observations, restart and restore proof and reconciliation. | Keep the pilot open until its observation period and evidence exist. |
| External research | Data and vendor rights, isolation, scoped external UAT, cost measurement and approved commercial terms. | Keep use within authorized scope. |
| Capital or regulated product | Separately recorded risk budget, limits, custody, legal review, recovery, authorized humans and deployment evidence. | Record a hold. An assistant or software milestone cannot lift it. |

First build market inputs and instrument rules alongside scoped persistence. Follow with reconciliation and independent risk, then costed backtesting, the web workflow and prospective paper evaluation. The architecture source's 30-day paper window is a proposed input to a **later capital review**; agree the internal pilot observation window before its run. Program acceptance includes negative outcomes. Use the delivery index for the 70 original identifiers, changed chart decision, later research/regulated product work and owner-specific security and administration additions. [Program and evidence owners](https://github.com/eighred/oracle/issues/13)

## Implementation worksheet for the first vertical slice

The examples establish the proposed v1 boundary between Rust and the web application. Preserve the semantics when choosing concrete server types. Treat paths below as interface designs until an integration test exercises them on the delivered revision.

| Proposed API | Authorization and consistency | Response and refusal |
|---|---|---|
| `GET /v1/portfolios/{id}/snapshots/{revision}` | Resolve the principal, scope the requested portfolio and fetch one immutable revision in a single read transaction. | Return exact cash, positions, marks and NAV with both cutoffs. An inaccessible object receives the same safe external response as a nonexistent one. |
| `POST /v1/portfolios/{id}/backtests` | Validate the registered strategy, approved dataset, allowed date range, cost model and per-principal compute budget. Bind the idempotency key to the normalized request. | Return an accepted run ID; the same key and body resolves to the same run. A changed body under the same key conflicts. An expired entitlement rejects new work. |
| `GET /v1/portfolios/{id}/runs/{run_id}` | Resolve the run through the scoped portfolio. Include versioned data/model references and the state of asynchronous work. | Distinguish pending, running, complete, incomplete, failed and canceled. Return the reason and coverage for a partial result. |
| `GET /v1/markets/{instrument_revision}/bars` | Check source entitlement before querying history. Bind resolution, UTC half-open interval and knowledge cutoff to pagination. | Return eligible ascending bars and the next cursor. Missing intervals are explicit. A cursor cannot switch instruments, tenants or source versions. |
| `GET /v1/portfolios/{id}/reports/{revision}` | Authorize the report and its source evidence at read time. Recheck authorization before asynchronous export delivery. | Return a stable report digest, revision chain and scoped drill-down links. Rendered output preserves currency, precision, timestamp and simulation status. |

Example of the fields shared by a synthetic snapshot, risk result and browser trace:

```json
{
  "schema_version": 1,
  "portfolio_id": "fixture-own-01",
  "snapshot_revision": "fixture-v1",
  "valid_at": "2026-01-02T10:00:00Z",
  "known_at": "2026-01-02T10:02:00Z",
  "reporting_currency": "USD",
  "cash": {"amount": "50.00", "currency": "USD"},
  "positions": [{
    "instrument_revision": "synthetic-unit-v1",
    "quantity": "2",
    "mark": {"amount": "25.00", "currency": "USD"},
    "mark_observation": "fixture-mark-01"
  }],
  "nav": {"amount": "100.00", "currency": "USD"},
  "completeness": "complete",
  "missing_inputs": []
}
```

For the missing-mark counterpart set `nav` to null, return `completeness = incomplete` and identify the affected instrument and missing price. A partial subtotal, if shown, needs a separate field and coverage label. The persisted financial read model supplies both values. UI formatting can round for display; the audit/export path retains original scale.

### Evidence chain and correction example

1. Record authorized source, native instrument ID, market rule revision and receipt watermark. Verify the observation digest before normalizing. A venue event can be valid for an interval earlier than its receipt.
2. Select available observations by effective interval and knowledge cutoff. Bind the selected version IDs into a canonical input manifest. Hash canonical fields using a declared encoding, order, UTC timestamp representation and decimal normalization. Exclude volatile wall-clock duration from the financial result digest.
3. Reconcile opening holdings and cash against eligible fills and external flows. Emit the exact entry identities used, model/rules version and snapshot revision. A silent removal, duplicate instruction or mismatched currency fails the reconciliation.
4. Calculate the financial result from the manifest. Attach the independent expected result, method and tolerance to the PR evidence. Preserve both signed and absolute loss conventions at the boundary to prevent a dashboard display choice from changing finance.
5. Publish a run ID and immutable report revision. When a later price correction arrives, rebuild at the new knowledge cutoff, persist the revised manifest and link the old/new reports. An operator can recover what the first decision used even after the browser cache expires.

### Order of evidence for prospective testing

Before the paper window opens, freeze the strategy revision, benchmark, admitted instruments, observation schedule, starting balances, fees, fill assumptions, intended report and conditions for stopping or declaring incomplete evidence. Record delayed exchange data and operator intervention as events. Preserve raw arrival and scheduling facts necessary to distinguish an unavailable opportunity from a deliberately rejected trade.

At each close, reconcile opening cash plus all signed postings to closing cash, position quantity to signed fill quantities, and NAV changes to market movement, costs and external flows. Compare these totals to the same interval of the declared benchmark. Report the numbers of expected, observed, missing and recovered intervals. A restart must resume from the last durable checkpoint without replacing prospective evidence with a later replay.

At the end of the predeclared window, an independent reviewer should be able to reproduce the accounting, identify the largest loss interval, read the data-quality status at that time and trace the event to a browser-visible report. Compare execution-time distributions at the same input size. Record the runtime environment and stratify slow observations by source delay, normalization, persistence, model compute and client rendering. If limits or time coverage were changed mid-run, keep both evaluation records and their decision history.

### Later research and commercial activation

| Capability | Candidate design | Activation evidence |
|---|---|---|
| Strategy search | Immutable feature schema, costed objective, walk-forward folds and bounded local trial registry | Time-leakage control, held-out result, comparison with the simple baseline, uncertainty and failed trials. |
| Distributed compute | Versioned Arrow extracts, resource-capped workers, cancellation and deterministic seeds | Single-host reference agrees within declared tolerance. Measure elapsed time, memory, transfer and cost at the same coverage. |
| External research tenancy | Per-portfolio rights, minimum disclosure, expiring report access and scoped exports | Two-party isolation, signed data rights, external user acceptance and successful revocation. |
| Managed-product fees | Immutable agreement version by eligible product, jurisdiction, investor/lot, basis and effective interval | Independent legal decision, signed mandate, finance checks, reconciled NAV, dispute/refund path and approved settlement. |
| Regulated funds and new asset classes | Independent unit register, custody reconciliation and product-specific valuation/risk models | Evidence per asset payoff, fund rules, subscriptions/redemptions, reporting currency, legal scope and custody controls. |

A declaration of technical readiness, an internal model approval, a signed commercial agreement, an authorized transfer and a settled payment are different observable events. Preserve the event, owner and evidence that actually occurred.
