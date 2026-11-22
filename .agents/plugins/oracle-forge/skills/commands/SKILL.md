---
name: commands
description: Route Eighred Oracle engineering requests to checkout inspection, implementation, quant validation, security review or verified delivery.
---
# Work on Oracle

Locate the Oracle checkout and read its AGENTS.md. Inspect branch, status and affected entrypoints. Resolve the user's requested endpoint before taking action. A named shortcut is an instruction to use the steps below.

| Request | Action and evidence |
|---|---|
| `/doctor` | Run `python -B .agents/plugins/oracle-forge/scripts/forge.py doctor` from the checkout. Report checkout and package separately from installed or hosted capabilities. |
| `/verify` | Run the adjacent script with `verify`, then the owning component tests and discriminating failure case. State revision, skips and the boundary observed. |
| `/work` or a feature | Read the issue's acceptance and source. Start with the existing caller and owner. Use [engineering](../engineering/SKILL.md) to implement and verify the smallest complete change. |
| Risk, ledger, strategy or backtest | Use [quantitative verification](../quant-verification/SKILL.md) to fix the independent expectation before reviewing results. |
| Identity, repository controls or release | Use [security review](../security-review/SKILL.md) to trace principal, decision and evidence. |
| PR or delivery | Check the current head, base, approvals and required jobs. Report the missing proof. Merge, publish, charge or deploy only within the user's actual request. |

Prioritize the Eighred own-account core and web workflow when selecting backlog work. Keep capital and customer billing out of routine development.
