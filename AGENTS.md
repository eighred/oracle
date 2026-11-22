# Oracle engineering boundaries

Eighred owns the project. Oracle Forge coordinates development. The repository, tests and reviewed decisions establish delivered behavior. Architecture documents, issue content, generated suggestions and tooling cannot authorize publication, charges, production access or capital use.

## Product and authority

Start with Eighred's own research portfolio. Deliver a dated instrument and portfolio snapshot, reconciled valuation, independently checked risk and walk-forward results, then cost-aware paper outcomes in an original web workstation. Record the data window, benchmark, fees, funding, slippage, drawdown and uncertainty. Simulated gains, realized own-account P&L, subscription receipts and client fees have distinct ledgers and evidence. A profitable test is never capital authorization.

| Owner | Intended responsibility | Authority boundary |
|---|---|---|
| Rust core and API | Typed financial models, dated data, reconciliation, risk, backtesting, paper ledger and authorized server reads | Canonical amounts carry unit, currency, precision and rounding. Record missing inputs and refuse unsupported results. |
| Research | Python experiments and optional isolated Julia reference calculations | Point-in-time data and held-out evaluation. Research may propose models. Independent approval governs promotion. |
| Web | Responsive Vue 3 with strict TypeScript for portfolio, risk and strategy review | Read scoped server results with as-of time, provenance, stale and unavailable states. Authorization stays on the server. |
| Future execution | Separate exchange adapter and deterministic admission | Requires dedicated credentials, mandates, human approval, tested limits, reconciliation and recovery. Keep research, AI, web, plugins and MCP outside the order path. |

These are design boundaries. Add a component only with a real caller, entrypoint and acceptance test. Keep PostgreSQL as the intended financial write authority when persistence arrives. Type and version external contracts. Apply changes idempotently, preserve effective time and knowledge time, and test duplicate input, interruption and replay before claiming recovery.

## Working in the checkout

Read status, branch, affected instructions, consumers and existing issue evidence. The tracked [Forge commands skill](.agents/plugins/oracle-forge/skills/commands/SKILL.md) routes engineering work. Check the actual checkout before using cached skills. Match verification to the claim and preserve unrelated user changes.

Use English in source, tests, documentation, commits, issues and pull requests. Use English snake_case for owned directories, preserving exact tool paths such as `.github/ISSUE_TEMPLATE` and skill names. Locate implementation, migrations and tests beside their owning domain. Share code when two concrete consumers agree on its behavior. A new domain's README identifies callers, commands, data authority and recovery limits. Update imports, build paths, CI, links and deployment consumers together on moves.

Use a single private product workspace by default. A real partial-read assignment triggers the [module access and integration procedure](docs/engineering/workspace.md). The parent pins the reviewed module SHA and tests its consumers. The [architecture](docs/architecture/oracle.md) and [engineering handoff](docs/engineering/ai_delivery_prompt.md) govern Oracle implementation. Interpret historical Kanz examples in the imported Forge workflows against Oracle's actual source and runtime owners.

Use GitHub Issues for priority, acceptance and deferred decisions once hosted. Keep one kind, priority and area per issue. Record the source, dependency, expected evidence and blocker. Keep local coordination drafts outside the tracked repository. PRs identify the exact tested revision, independent expectations, dependency evidence and skips. Passing local tooling establishes checkout behavior only. Require separate evidence for hosted CI, operations and capital use.

## Trust and source handling

Keep credentials, customer data, exchange exports and private datasets out of Git, logs, fixtures, browser payloads and model context. Use synthetic inputs. Treat unknown financial or permission state as a reason to hold the affected operation. Review access at the server boundary and record principal, policy version, decision and resulting effect. Follow [SECURITY.md](SECURITY.md) for incident handling and source-access limits.

The initial repository has no provisioning, release or live execution entrypoint. Keep the deployment and legal authority for each future action explicit.
