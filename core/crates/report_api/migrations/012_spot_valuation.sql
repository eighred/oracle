BEGIN;
CREATE INDEX spot_open_pairs ON oracle_spot_paper.positions(pair,as_of) WHERE qty>0;
CREATE TABLE oracle_spot_paper.mark_jobs (
    pair text PRIMARY KEY, token uuid NOT NULL, close_ms bigint NOT NULL,
    due timestamptz NOT NULL, outcome text NOT NULL
);
CREATE TABLE oracle_spot_paper.minute_marks (
    pair text NOT NULL, close_ms bigint NOT NULL, mark numeric NOT NULL CHECK(mark>0),
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    observations jsonb NOT NULL, PRIMARY KEY(pair,close_ms)
);

-- One bounded work item per native pair, independent of member count.
CREATE FUNCTION oracle_spot_paper.claim_mark() RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE point bigint := floor(extract(epoch FROM clock_timestamp()-interval '4 seconds')/60)::bigint*60000;
    job oracle_spot_paper.mark_jobs;
BEGIN
    INSERT INTO oracle_spot_paper.mark_jobs AS j(pair,token,close_ms,due,outcome)
    SELECT p.pair,gen_random_uuid(),point,clock_timestamp()+interval '45 seconds','reading'
    FROM oracle_spot_paper.positions p LEFT JOIN oracle_spot_paper.mark_jobs prior ON prior.pair=p.pair
    WHERE p.qty>0 AND p.as_of<to_timestamp(point/1000) AND coalesce(prior.due,'-infinity')<=clock_timestamp()
    ORDER BY prior.due NULLS FIRST,p.pair LIMIT 1
    ON CONFLICT(pair) DO UPDATE SET token=excluded.token,close_ms=excluded.close_ms,due=excluded.due,outcome=excluded.outcome
    WHERE j.due<=clock_timestamp() RETURNING * INTO job;
    IF NOT FOUND THEN RETURN NULL; END IF;
    RETURN jsonb_build_object('pair',job.pair,'token',job.token,'closeMs',point,
        'first',oracle_spot_paper.enabled('binance'),'second',oracle_spot_paper.enabled('okx'));
END $$;

CREATE FUNCTION oracle_spot_paper.finish_mark(symbol text,lease uuid,samples jsonb) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE job oracle_spot_paper.mark_jobs; eligible jsonb; price numeric; settled_at timestamptz;
BEGIN
    SELECT * INTO job FROM oracle_spot_paper.mark_jobs WHERE pair=symbol AND token=lease AND outcome='reading' FOR UPDATE;
    IF NOT FOUND OR job.due<=clock_timestamp() THEN RETURN; END IF;
    settled_at:=to_timestamp(job.close_ms/1000);
    IF jsonb_typeof(samples) IS DISTINCT FROM 'array' OR jsonb_array_length(samples)>2 THEN RAISE invalid_parameter_value; END IF;
    LOCK TABLE oracle_admin.venue_connections IN SHARE MODE;
    SELECT jsonb_agg(s ORDER BY s->>'venue'),max((s->>'price')::numeric) INTO eligible,price
    FROM jsonb_array_elements(samples) AS s
    WHERE oracle_spot_paper.enabled(s->>'venue') AND s->>'pair'=symbol
      AND (s->>'closeMs')::bigint=job.close_ms AND (s->>'price') ~ '^[0-9]+([.][0-9]+)?$' AND (s->>'price')::numeric>0
      AND (s->>'receivedMs')::bigint BETWEEN job.close_ms AND floor(extract(epoch FROM clock_timestamp())*1000)
      AND s->>'metadataDigest' ~ '^[0-9a-f]{64}$' AND s->>'candleDigest' ~ '^[0-9a-f]{64}$';
    IF price IS NULL OR clock_timestamp()>=settled_at+interval '105 seconds' THEN
        UPDATE oracle_spot_paper.mark_jobs SET due=clock_timestamp()+interval '12 seconds',outcome='unavailable' WHERE pair=symbol;
        RETURN;
    END IF;

    INSERT INTO oracle_spot_paper.minute_marks(pair,close_ms,mark,observations) VALUES(symbol,job.close_ms,price,eligible)
    ON CONFLICT DO NOTHING;
    SELECT m.mark INTO STRICT price FROM oracle_spot_paper.minute_marks m WHERE m.pair=symbol AND m.close_ms=job.close_ms;
    UPDATE oracle_spot_paper.positions SET mark=price,as_of=settled_at WHERE pair=symbol AND qty>0 AND as_of<settled_at;
    UPDATE oracle_spot_paper.mark_jobs SET due=settled_at+interval '64 seconds',outcome='marked' WHERE pair=symbol;
END $$;
REVOKE ALL ON FUNCTION oracle_spot_paper.claim_mark(),oracle_spot_paper.finish_mark(text,uuid,jsonb) FROM PUBLIC;
GRANT USAGE ON SCHEMA oracle_spot_paper TO oracle_basket_worker;
GRANT EXECUTE ON FUNCTION oracle_spot_paper.claim_mark(),oracle_spot_paper.finish_mark(text,uuid,jsonb) TO oracle_basket_worker;
COMMIT;
