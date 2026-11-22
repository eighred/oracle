# Oracle Forge

Oracle Forge 0.1.0 is the repository-owned development plugin for Eighred Oracle. Its [commands](skills/commands/SKILL.md) route changes to the appropriate evidence. It runs locally with Git and Python 3.11+. It requests no credentials, MCP server or background process.

## Discover and install

The repository discovery skill works in a trusted Oracle checkout. For an installed package, run in the reviewed repository root:

```sh
codex plugin marketplace add .
codex plugin add oracle-forge@oracle-local
codex plugin list --json
python -B .agents/plugins/oracle-forge/scripts/forge.py doctor
python -B .agents/plugins/oracle-forge/scripts/forge.py verify
```

Open a fresh Oracle chat to load the installed skills. Ask “Use Oracle Forge to verify this checkout.” The doctor reports the checkout revision, package version and implemented workflows. Verify checks the source package, license digest, discovery wiring, tracked layout and local documentation links. Neither command connects to a remote.

When changing packaged files, increment the plugin version, run the local checks and install from the reviewed revision using the supported CLI. For rollback use a separate checkout of the reviewed prior version, remove the plugin and its marketplace with `codex plugin remove oracle-forge@oracle-local` and `codex plugin marketplace remove oracle-local`, then repeat installation there and verify the reported version. Keep the marketplace source checkout at a stable path.
