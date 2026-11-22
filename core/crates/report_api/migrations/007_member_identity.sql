BEGIN;
CREATE SCHEMA oracle_member;
REVOKE ALL ON SCHEMA oracle_member FROM PUBLIC;
CREATE TABLE oracle_member.members (
    issuer text NOT NULL, subject text NOT NULL, handle text NOT NULL UNIQUE,
    PRIMARY KEY(issuer,subject), CHECK(handle ~ '^@[a-z0-9_]{3,32}$')
);
CREATE TABLE oracle_member.sessions (
    token_digest text PRIMARY KEY CHECK(token_digest ~ '^[0-9a-f]{64}$'),
    handle text REFERENCES oracle_member.members(handle) NOT NULL, expires_at timestamptz NOT NULL, provider_session text NOT NULL
);
CREATE TABLE oracle_member.signed_out (handle text,provider_session text,until_time timestamptz NOT NULL,PRIMARY KEY(handle,provider_session));
CREATE FUNCTION oracle_member.issue(iss text, sub text, preferred text, proof text, expiry bigint, sid text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE name text;
BEGIN
    IF expiry <= extract(epoch FROM now()) OR expiry > extract(epoch FROM now()) + 600 THEN RAISE invalid_parameter_value; END IF;
    INSERT INTO oracle_member.members VALUES(iss,sub,'@'||lower(preferred))
        ON CONFLICT (issuer,subject) DO NOTHING;
    SELECT handle INTO STRICT name FROM oracle_member.members WHERE issuer=iss AND subject=sub;
    IF EXISTS(SELECT FROM oracle_read.sessions WHERE principal=name) THEN RAISE unique_violation; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(name||sid,0));
    DELETE FROM oracle_member.signed_out WHERE until_time<=now();
    IF EXISTS(SELECT FROM oracle_member.signed_out WHERE handle=name AND provider_session=sid) THEN RAISE insufficient_privilege; END IF;
    DELETE FROM oracle_member.sessions WHERE expires_at<=now();
    INSERT INTO oracle_member.sessions VALUES (proof,name,to_timestamp(expiry),sid);
    RETURN name;
END $$;
CREATE OR REPLACE FUNCTION oracle_read.principal() RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
    SELECT principal FROM oracle_read.sessions
    WHERE token_digest=current_setting('oracle.token_digest',true) AND principal=current_setting('oracle.operator',true)
    AND revoked_at IS NULL AND expires_at>statement_timestamp()
    UNION ALL
    SELECT handle FROM oracle_member.sessions WHERE token_digest=current_setting('oracle.token_digest',true)
    AND handle=current_setting('oracle.operator',true) AND expires_at>statement_timestamp()
$$;
CREATE FUNCTION oracle_member.logout() RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text:=oracle_read.principal(); sid text;
BEGIN
    SELECT provider_session INTO STRICT sid FROM oracle_member.sessions WHERE handle=actor AND token_digest=current_setting('oracle.token_digest',true);
    PERFORM pg_advisory_xact_lock(hashtextextended(actor||sid,0));
    INSERT INTO oracle_member.signed_out VALUES(actor,sid,now()+interval '10 hours') ON CONFLICT DO NOTHING;
    DELETE FROM oracle_member.sessions WHERE handle=actor AND provider_session=sid;
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA oracle_member FROM PUBLIC;
GRANT USAGE ON SCHEMA oracle_member TO oracle_report_reader;
GRANT EXECUTE ON FUNCTION oracle_member.issue(text,text,text,text,bigint,text),oracle_member.logout() TO oracle_report_reader;
COMMIT;
