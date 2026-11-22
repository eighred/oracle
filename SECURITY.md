# Security policy

Report vulnerabilities privately to security@eighred.com using a redacted description, affected revision, impact and a safe observation. Keep credentials, private source extracts and client or trading data out of tickets. Maintainers should acknowledge reports within two business days, rotate exposed secrets promptly, and preserve restricted incident evidence. Remediation targets from triage are three days for critical, 14 for high and 30 for medium severity. Document the owner, compensating control and review date for an exception.

## Repository controls and limits

The checkout supplies review ownership, pinned Actions workflows, a history scan with a synthetic detection check and dependency-update configuration. Local scans run on demand. Hosted enforcement requires private-repository settings, an eligible plan and observed results on the protected branch. See the [hosting guide](.github/governance/README.md).

Anyone with repository read access can normally retrieve its tracked contents and history. CODEOWNERS routes reviews. It supplies no directory-level read restriction. Give access to this repository only to people authorized for its entire contents. Before onboarding a restricted module team, move its boundary into a separately permissioned private repository or service and test access with representative identities. Distribute reviewed, signed interfaces or artifacts to consumers. Keep sensitive algorithms server-side. Obfuscation of distributed code supplies limited resistance to inspection.

## Organizational controls awaiting implementation

| Owner | Proposed control | Evidence needed before claiming enforcement |
|---|---|---|
| Identity and repository administrators | Least privilege, organization SSO/MFA, time-bound module access and prompt offboarding | Approved access matrix, authorized and denied identity tests, revocation and audit events |
| Endpoint and network security | Managed VDI, controlled clipboard, print and USB, DLP rules for personal mail, cloud uploads and unmanaged endpoints | Synthetic authorized transfer and blocked export per channel, exception expiry, evidence of log delivery |
| Security operations and privacy | Restricted audit trail, bulk-download anomaly alerts, incident triage and retention | Approved scope, redacted simulated event, measured alert delivery, reviewer response, documented access and deletion |
| Legal and HR | Review applicable confidentiality, invention assignment, monitoring notice, retention and jurisdiction-specific remedies | Counsel approval of the applicable documents and process before enforcement or employee monitoring |

A repository cannot enforce endpoint controls or guarantee that authorized readers never copy source. No employee monitoring or contractual sanctions are activated by this policy.
