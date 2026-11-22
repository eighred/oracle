BEGIN;
CREATE SCHEMA oracle_admin;
REVOKE ALL ON SCHEMA oracle_admin FROM PUBLIC;
CREATE TABLE oracle_admin.administrators (principal text PRIMARY KEY);
CREATE TABLE oracle_admin.venue_connections (
    venue text PRIMARY KEY CHECK (venue IN ('binance','okx')),
    secret_ref text NOT NULL CHECK (secret_ref = 'vault:oracle/company/' || venue),
    enabled boolean NOT NULL, revision int NOT NULL
);
CREATE TABLE oracle_admin.fee_proposals (
    request_id uuid PRIMARY KEY, bps int NOT NULL CHECK(bps BETWEEN 0 AND 10000),
    effective_at timestamptz NOT NULL, proposed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    proposed_by text NOT NULL, CHECK(effective_at>proposed_at)
);
CREATE TABLE oracle_admin.venue_events (
    request_id uuid PRIMARY KEY, actor text NOT NULL, occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    venue text NOT NULL, enabled boolean NOT NULL, revision int NOT NULL, UNIQUE(venue,revision)
);
CREATE FUNCTION oracle_admin.settings(op text,p jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text:=oracle_read.principal(); v text:=p->>'venue'; req uuid:=(p->>'request')::uuid; rev int;
BEGIN
    IF actor IS NULL OR NOT EXISTS(SELECT FROM oracle_admin.administrators WHERE principal=actor) THEN RAISE insufficient_privilege; END IF;
    IF op IN ('connection','fee') THEN
        IF req IS NULL THEN RAISE invalid_parameter_value; END IF;
        PERFORM pg_advisory_xact_lock(hashtextextended(req::text,0));
    END IF;
    IF op='connection' THEN
        IF NOT EXISTS(SELECT FROM oracle_admin.venue_events WHERE request_id=req) THEN
            INSERT INTO oracle_admin.venue_connections VALUES(v,'vault:oracle/company/'||v,(p->>'enabled')::boolean,1)
                ON CONFLICT(venue) DO UPDATE SET enabled=excluded.enabled, revision=oracle_admin.venue_connections.revision+1
                RETURNING revision INTO rev;
            INSERT INTO oracle_admin.venue_events(request_id,actor,venue,enabled,revision) VALUES(req,actor,v,(p->>'enabled')::boolean,rev);
        ELSIF NOT EXISTS(SELECT FROM oracle_admin.venue_events e WHERE request_id=req AND e.actor=oracle_read.principal() AND venue=v AND enabled=(p->>'enabled')::boolean) THEN
            RAISE serialization_failure;
        END IF;
    ELSIF op='fee' THEN
        IF NOT EXISTS(SELECT FROM oracle_admin.fee_proposals WHERE request_id=req) THEN
            INSERT INTO oracle_admin.fee_proposals VALUES(req,(p->>'bps')::int,(p->>'effective')::timestamptz,clock_timestamp(),actor);
        ELSIF NOT EXISTS(SELECT FROM oracle_admin.fee_proposals WHERE request_id=req AND bps=(p->>'bps')::int AND effective_at=(p->>'effective')::timestamptz AND proposed_by=actor) THEN RAISE serialization_failure; END IF;
    ELSIF op<>'list' THEN RAISE invalid_parameter_value;
    END IF;
    RETURN jsonb_build_object('connections',(SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY venue),'[]') FROM oracle_admin.venue_connections c),
        'fees',(SELECT coalesce(jsonb_agg(to_jsonb(f) ORDER BY proposed_at DESC),'[]') FROM oracle_admin.fee_proposals f));
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA oracle_admin FROM PUBLIC;
GRANT USAGE ON SCHEMA oracle_admin TO oracle_report_reader;
GRANT EXECUTE ON FUNCTION oracle_admin.settings(text,jsonb) TO oracle_report_reader;
COMMIT;
