# Work in one Oracle checkout

The private parent repository owns the product workspace. Change an ordinary module and its API and web consumers in one PR and test the affected entrypoints together. The foundation includes a receiver for future module events. No restricted module is registered at bootstrap.

## Full-source developer

The [financial core](../../core/Cargo.toml) is a Cargo workspace. Its crates own portfolio calculations and replay. Place new financial domains here when they have a caller and an acceptance test. Keep independent responsibilities such as the Vue workstation with their own owners.

```sh
git clone --recurse-submodules https://github.com/eighred/oracle.git
cd oracle
git pull --ff-only --recurse-submodules
git submodule update --init --recursive
```

The first command checks out the complete authorized workspace at reviewed revisions. Assign full-source engineers read access to each relevant repository and write access where needed. Grant administration independently. When editing a submodule from the parent checkout, create a branch inside its directory from the module's current main. Commit and open the PR in the module repository. Keep local work committed or stashed before updating an existing checkout.

## First restricted assignment

1. Identify the module, source owner, callers and approved readers. Inspect organization base permission, team grants, outside collaborators, apps, deploy keys, artifacts and caches. An inherited read grant can defeat the boundary. Organization-wide permission changes require their own decision. Source previously committed in the parent remains in its history.
2. Give the restricted module its own private repository, protected main branch, standalone build, versioned interfaces, synthetic fixtures and contract tests. Its contributors can develop without access to Oracle. The module owns implementation issues. The parent tracks the integration and retains a pinned gitlink.
3. In a reviewed parent PR register a lowercase snake_case module name at `modules/<name>`, with an HTTPS Eighred GitHub URL ending in `.git` and tree entry mode `160000`. Set up an integration check that actually builds and tests the parent consumer at the proposed pin. Limit read credentials to the necessary module and discard them before executing module code. Use isolated workers for source with different readership.
4. Verify two identities. A full-source engineer can clone the complete workspace. A module-only engineer can clone and test the module while Git, API, archives, Actions logs and artifacts deny access to the parent and siblings. Revoke the grant and verify new requests fail. Keep redacted observations with the owning issue. [Access acceptance](https://github.com/eighred/oracle/issues/6)

## Update events

At onboarding, configure a trusted GitHub App receiver for merged PRs on the registered module's protected main. Verify webhook signature, installation, source repository and merge target before sending a repository dispatch to Oracle. Keep the dispatcher credential outside module jobs. Its payload contains a module name, for example `{"event_type":"oracle-module-merged","client_payload":{"module":"pricing"}}`.

Configure the trusted sender login in `ORACLE_MODULE_EVENT_ACTOR` and the updater App identity in the parent workflow's named variable and secret. Restrict installations to approved repositories and preserve the parent ruleset. Validate actual event delivery and the effective sender with a real module during onboarding.

The [workflow](../../.github/workflows/module_update.yml) resolves the name from the trusted parent tree before requesting short-lived tokens. It grants read access to the registered module and proposal permissions on Oracle. The [proposal tool](../../tools/module_update.py) resolves the current protected source head, requires forward ancestry and creates a pin-only parent PR. Duplicate events reuse a proposal. After a partially completed attempt it verifies the existing proposal tree. If the parent moved, retry against current main. Non-forward updates require a reviewed manual PR.

Require a failing consumer test to block integration. After a compatible change, merge the parent PR and verify the resulting SHA in a clean recursive clone. For a breaking contract, add a compatible second version, migrate consumers in linked PRs, measure remaining usage and then retire the old version. For rollback, select the previous compatible gitlink in a reviewed parent PR and check persisted data and migrations. A full-source checkout shows both projects in one editor while Git preserves their respective ownership and histories. [Git submodules](https://git-scm.com/docs/gitsubmodules)
