BEGIN;
CREATE ROLE oracle_report_reader NOLOGIN;
CREATE ROLE oracle_report_writer NOLOGIN;
CREATE SCHEMA oracle_read;
REVOKE ALL ON SCHEMA oracle_read FROM PUBLIC;
GRANT USAGE ON SCHEMA oracle_read TO oracle_report_reader, oracle_report_writer;

CREATE TABLE oracle_read.sessions (
    token_digest text PRIMARY KEY CHECK (token_digest ~ '^[a-f0-9]{64}$'),
    principal text NOT NULL,
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz
);
CREATE TABLE oracle_read.portfolio_grants (
    principal text NOT NULL,
    portfolio text NOT NULL,
    PRIMARY KEY (principal, portfolio)
);
CREATE TABLE oracle_read.reports (
    portfolio text NOT NULL,
    report_id text NOT NULL CHECK (report_id = encode(sha256(payload), 'hex')),
    payload bytea NOT NULL,
    published_at timestamptz NOT NULL DEFAULT statement_timestamp(),
    PRIMARY KEY (portfolio, report_id)
);
CREATE INDEX reports_latest ON oracle_read.reports (portfolio, published_at DESC, report_id);

CREATE FUNCTION oracle_read.principal() RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
    SELECT principal FROM oracle_read.sessions
    WHERE token_digest = current_setting('oracle.token_digest', true)
      AND principal = current_setting('oracle.operator', true)
      AND revoked_at IS NULL AND expires_at > statement_timestamp()
$$;
CREATE FUNCTION oracle_read.allowed(resource text) RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
    SELECT EXISTS (SELECT 1 FROM oracle_read.portfolio_grants
                   WHERE principal = oracle_read.principal() AND portfolio = resource)
$$;
REVOKE ALL ON FUNCTION oracle_read.principal(), oracle_read.allowed(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION oracle_read.principal(), oracle_read.allowed(text) TO oracle_report_reader;

ALTER TABLE oracle_read.reports ENABLE ROW LEVEL SECURITY;
ALTER TABLE oracle_read.reports FORCE ROW LEVEL SECURITY;
CREATE POLICY scoped_report ON oracle_read.reports FOR SELECT TO oracle_report_reader
    USING (oracle_read.allowed(portfolio));
CREATE POLICY publish_report ON oracle_read.reports FOR INSERT TO oracle_report_writer WITH CHECK (true);
GRANT SELECT ON oracle_read.reports TO oracle_report_reader;
GRANT INSERT (portfolio, report_id, payload) ON oracle_read.reports TO oracle_report_writer;
COMMIT;
