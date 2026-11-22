BEGIN;
CREATE TABLE oracle_social.watchlists (
    owner_name text PRIMARY KEY,
    revision integer NOT NULL DEFAULT 0 CHECK (revision >= 0),
    pairs text[] NOT NULL DEFAULT '{}' CHECK (cardinality(pairs) <= 100)
);
REVOKE ALL ON oracle_social.watchlists FROM PUBLIC;

CREATE FUNCTION oracle_social.watchlist(expected integer, desired text[])
RETURNS TABLE(revision integer, pairs text[], conflict boolean)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE
    viewer text := oracle_read.principal();
    saved oracle_social.watchlists%ROWTYPE;
BEGIN
    IF viewer IS NULL THEN RAISE insufficient_privilege; END IF;
    IF expected IS NULL AND desired IS NULL THEN
        SELECT w.* INTO saved FROM oracle_social.watchlists w WHERE w.owner_name=viewer;
        RETURN QUERY SELECT coalesce(saved.revision,0), coalesce(saved.pairs,'{}'::text[]), false;
        RETURN;
    END IF;
    IF expected IS NULL OR expected < 0 OR desired IS NULL OR cardinality(desired)>100
       OR cardinality(desired) <> (SELECT count(DISTINCT p) FROM unnest(desired) p)
    THEN RAISE invalid_parameter_value; END IF;

    INSERT INTO oracle_social.watchlists(owner_name) VALUES(viewer) ON CONFLICT DO NOTHING;
    SELECT w.* INTO STRICT saved FROM oracle_social.watchlists w WHERE w.owner_name=viewer FOR UPDATE;
    IF saved.pairs = desired THEN
        RETURN QUERY SELECT saved.revision, saved.pairs, false;
    ELSIF saved.revision <> expected THEN
        RETURN QUERY SELECT saved.revision, saved.pairs, true;
    ELSE
        UPDATE oracle_social.watchlists w SET pairs=desired, revision=w.revision+1
        WHERE w.owner_name=viewer RETURNING w.* INTO saved;
        RETURN QUERY SELECT saved.revision, saved.pairs, false;
    END IF;
END $$;
REVOKE ALL ON FUNCTION oracle_social.watchlist(integer,text[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION oracle_social.watchlist(integer,text[]) TO oracle_report_reader;
COMMIT;
