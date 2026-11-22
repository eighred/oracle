# Private GitHub hosting baseline

This directory describes the intended controls for private `eighred/oracle`. The JSON files are reviewable REST ruleset request bodies. Hosting requires a separate authorized operation. Local checks do not activate any settings.

## Reference and adaptations

The reference was the public [Eighred Kanz repository](https://github.com/eighred/kanz/tree/e8998af758d38ded6c25e853ef21e451e0fe425b) and its read-only settings on 2026-11-22. Protection used three active rulesets. The legacy branch-protection endpoint had no separate protection object.

| Setting | Intended Oracle setting and readback |
|---|---|
| Identity and visibility | Verify exact organization, repository name, private visibility and restricted membership before sending source. Disable private forking. Confirm outside collaborators and organization base permissions. |
| Repository features | Default branch main, issues and projects enabled, wiki and discussions disabled. Merge, squash, rebase and auto-merge enabled. Delete merged branches. Update-branch button and web commit signoff disabled, as on Kanz. |
| Actions | Enabled. Reference allows all actions with full SHA pinning required. Default token read-only. Disallow workflow PR approval. Oracle checkout steps disable credential persistence. |
| Security products | Verify plan availability and enable dependency graph, Dependabot security updates, secret scanning and push protection, validity checks and non-provider patterns. Enable private code scanning for the Actions and Python jobs. Record unavailable features and resulting protection gaps. |
| Ownership | Confirm that the CODEOWNERS custodian has write access on Oracle. The single-maintainer reference requires a PR, resolved threads, current required checks and zero approving reviews. Retain an audit record for the inherited administrator bypass. Revisit independent review with a second maintainer. |
| Release tags | Only the administrator bypass can create v* tags. Updates and deletions have no bypass. No release publishing workflow exists in this foundation. |

The [main ruleset](main.json) requires `oracle-ci-required`, `gitleaks`, `secret-scan-selftest`, `Analyze (actions)` and `Analyze (python)`. The foundation job replaces the Kanz-specific aggregate. Add language and infrastructure jobs with their first actual sources. [Release creation](tag_creation.json) and [release immutability](tag_immutability.json) follow the reference's ref scope and bypass behavior.

## Hosting acceptance

The hosting owner verifies private visibility first. After authorized initial publication, enable eligible security products, observe each check on an actual head, apply the reviewed rulesets and compare their readback, including strictness and bypass actors. Exercise a harmless PR using a non-bypass identity and record the refused merge with a failing required check, then the successful reviewed path with passing checks. Check code-owner resolution. Record private-plan limitations as blockers for the affected control. Mark hosted enforcement complete only when the behavior was observed.
