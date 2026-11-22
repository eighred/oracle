BEGIN;
-- A data owner records a reviewed scope for internal BTC/USDT computation, retention and operator display.
-- No application role can create or extend a source grant.
CREATE TABLE oracle_paper.binance_scopes (
    scope_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    evidence_ref text NOT NULL CHECK (length(trim(evidence_ref)) > 0),
    allowed_until timestamptz NOT NULL DEFAULT 'infinity',
    revoked boolean NOT NULL DEFAULT false
);
CREATE FUNCTION oracle_read.source_permitted(grant_id bigint) RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
    SELECT EXISTS (SELECT FROM oracle_paper.binance_scopes WHERE scope_id=grant_id
                   AND NOT revoked AND allowed_until > statement_timestamp())
$$;
REVOKE ALL ON FUNCTION oracle_read.source_permitted(bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION oracle_read.source_permitted(bigint) TO oracle_report_reader;
GRANT SELECT ON oracle_paper.binance_scopes TO oracle_paper_operator, oracle_report_writer;
ALTER TABLE oracle_read.reports ADD COLUMN source_scope bigint REFERENCES oracle_paper.binance_scopes;
GRANT INSERT (source_scope) ON oracle_read.reports TO oracle_report_writer;
CREATE POLICY venue_source_scope ON oracle_read.reports AS RESTRICTIVE FOR SELECT TO oracle_report_reader
    USING (source_scope IS NULL OR oracle_read.source_permitted(source_scope));
COMMIT;
