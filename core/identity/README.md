# Member accounts

The Vue workstation and native clients authenticate through the Oracle realm. Keycloak owns passwords, verified addresses and authenticator secrets. PostgreSQL owns the binding from the issuer and immutable subject to the public handle. Oracle wallets and social APIs use that handle. An email address is never a public wallet identifier.

## Run the loopback pilot

1. Run `docker compose -f core/identity/compose.yaml up -d` from the repository root. The realm is imported on first startup. The named identity volume retains members between container restarts.
2. Set `ORACLE_OIDC_ISSUER=http://127.0.0.1:8180/realms/oracle` in the environment of the Rust API. Start the workstation on port 8040 as described in the [web runbook](../../web/README.md).
3. Open [Oracle](http://127.0.0.1:8040/) and choose **Register**. Choose a public username and enter an email address. Open the verification message in the [development inbox](http://127.0.0.1:8125/). Set a password of at least 12 characters, add the authenticator, then sign in with email or username and a current code.
4. To check the complete browser journey with a synthetic recipient, run `npm run test:member` from `web` with Playwright Chromium installed. The test checks registration, SMTP delivery, mandatory TOTP, access to the new member's own wallet, absence of the email address from its representation, and revocation on logout.
5. Stop the identity and inbox services with `docker compose -f core/identity/compose.yaml stop`. The separate integration test runner uses a disposable financial database. A persistent member ledger is available with the command below. For recovery of customer balances the financial database and its roles need independent backups and restore checks.

For a persistent local paper workspace, build the web distribution and the Rust API. Run the following from the repository root as a dedicated Linux user. The state directory must belong to that user with mode 0700. The launcher starts PostgreSQL, applies transactional migrations once and runs the API and basket worker with separate database roles. Stop with Ctrl+C. Restart with the same directory to reopen accounts, wallet history and posts. To assign local staff duties after a verified first sign-in, pass `--platform-admin @handle` or `--basket-curator @handle` to the launcher. These assignments persist in PostgreSQL.

```sh
export ORACLE_OIDC_ISSUER=http://127.0.0.1:8180/realms/oracle
python3 core/crates/report_api/tools/member_workspace.py \
  --pg-bin "$(pg_config --bindir)" --api-bin core/target/debug/oracle-report-api \
  --dist web/dist --state-dir ~/.local/state/oracle-members
```

The native public client uses an external browser, the authorization-code flow with S256 PKCE and the registered return URI `com.eighred.oracle://identity/callback`. An Android device testing a loopback build needs both the identity and API ports forwarded. An interrupted flow can be started again. Tokens and the verifier are held in memory.

## Deployment inputs

Use a dedicated HTTPS issuer, production Keycloak configuration, backed-up database, explicit application origins and a transactional SMTP service with a verified sender domain. Set the SMTP credential through the provider's secret configuration. The bundled SMTP catcher accepts local test messages and exposes them to developers on this machine. Realm import skips an existing realm. Review and apply changes through the identity administrator when upgrading an existing installation.

The browser and mobile clients have no client secret. The OIDC exchange checks the token at the configured issuer, verifies the subject, client, expiry, email verification and both authentication methods, then issues a short-lived Oracle credential. Member registration creates an ordinary account with a zero paper balance. Portfolio grants and platform administration are assigned separately. Password recovery preserves the enrolled OTP. Recovery for a lost authenticator requires an independently verified operator procedure.

The realm includes disabled Google and Apple broker entries with the post-login flow **Oracle broker MFA**. Register the exact HTTPS callback generated for each entry. Supply the Google client ID and secret, or the Apple Services ID and signed, expiring Apple client secret in the identity administration service. Validate the Apple authorization response mode and relay email delivery on the registered domain. Enable each entry after testing the provider, account linking and local authenticator with a dedicated user. The API accepts the provider-bound OTP evidence only for these two aliases. [Keycloak identity brokering](https://www.keycloak.org/docs/latest/server_admin/#_identity_broker) and [Apple Services ID and configuration](https://developer.apple.com/documentation/signinwithapple/configuring-your-environment-for-sign-in-with-apple) Email sign-in works independently of those providers.
