# Eighred Oracle

Oracle is Eighred's internal research and risk workstation. The first delivery should let an Eighred operator evaluate one own-account strategy from dated inputs through a reproducible backtest, cost-adjusted paper portfolio and browser report. It must work without customers, subscriptions or funded exchange credentials.

This checkout contains engineering governance and Oracle Forge. It has no application runtime. Product design follows EOR-ARCH-001 v2.1, subject to the owner's priority above. Use the supplied architecture as a design reference. Record implementation decisions against actual source and evidence in the owning issue.

## Local development

From the repository root, with Git and Python 3.11 or later:

```sh
python -B .agents/plugins/oracle-forge/scripts/forge.py doctor
python -B .agents/plugins/oracle-forge/scripts/forge.py verify
```

[Contributing](CONTRIBUTING.md) covers the full local check sequence. See the [engineering contract](AGENTS.md), [security policy](SECURITY.md) and [Oracle Forge guide](.agents/plugins/oracle-forge/README.md). The designated hosted destination is private `eighred/oracle`. [Hosting requirements](.github/governance/README.md) describe the controls to verify there.

The [license](LICENSE.md) is the exact Eighred license file at [the specified Kanz revision](https://github.com/eighred/kanz/blob/e8998af758d38ded6c25e853ef21e451e0fe425b/LICENSE.md). External distribution and the terms applicable to private contributions require owner and legal review.
