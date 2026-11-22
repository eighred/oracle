-- All accounts, market evidence and role settings are local to this transaction.
BEGIN;
SET LOCAL ROLE oracle_report_reader;
SELECT set_config('oracle.operator',oracle_member.issue('https://test.invalid','mark','valuation_test',repeat('b',64),
    floor(extract(epoch FROM now()))::bigint+120,'valuation-test'),true),set_config('oracle.token_digest',repeat('b',64),true);
DO $$
DECLARE book jsonb; ticket jsonb;
BEGIN
    IF has_function_privilege(current_user,'oracle_spot_paper.finish_mark(text,uuid,jsonb)','EXECUTE')
       OR has_table_privilege(current_user,'oracle_spot_paper.minute_marks','SELECT') THEN
        RAISE EXCEPTION 'Member can access internal marks'; END IF;
    PERFORM oracle_social.community_change('profile','{"public":true,"ranking":true,"bio":"","avatar":""}');
    PERFORM oracle_basket_paper.change('deposit',jsonb_build_object('request',gen_random_uuid(),'currency','USD','amount','60000'));
    book:=jsonb_build_array(jsonb_build_object('venue','binance','price','55000','fee','5','received_ms',floor(extract(epoch FROM now())*1000)),
                            jsonb_build_object('venue','okx','price','54999','fee','5','received_ms',floor(extract(epoch FROM now())*1000)));
    ticket:=oracle_spot_paper.quote('BTC/USD','buy',1,book);
    PERFORM oracle_spot_paper.confirm((ticket->>'id')::uuid);
END $$;
RESET ROLE;
UPDATE oracle_spot_paper.positions SET as_of=now()-interval '11 minutes' WHERE member='@valuation_test';
CREATE TEMP TABLE mark_check AS SELECT oracle_basket_paper.profile(NULL)->'activity' AS activity;
DO $$ BEGIN
    IF oracle_social.rankings('USD')->'rankings' <> '[]' THEN RAISE EXCEPTION 'Stale position entered rankings'; END IF;
END $$;
DO $$
DECLARE job jsonb; point bigint; common jsonb; samples jsonb; old_lease uuid; wallet jsonb; ranked jsonb;
BEGIN
    job:=oracle_spot_paper.claim_mark();
    IF job->>'pair' IS DISTINCT FROM 'BTC/USD' OR oracle_spot_paper.claim_mark() IS NOT NULL THEN
        RAISE EXCEPTION 'Claim did not isolate a single open pair'; END IF;
    old_lease:=(job->>'token')::uuid;
    -- An interrupted worker cannot submit after another worker recovers the lease.
    UPDATE oracle_spot_paper.mark_jobs SET due=now()-interval '1 minute';
    job:=oracle_spot_paper.claim_mark();
    point:=(job->>'closeMs')::bigint;
    common:=jsonb_build_object('pair','BTC/USD','closeMs',point,'receivedMs',point+1,'metadataDigest',repeat('a',64),'candleDigest',repeat('b',64));
    samples:=jsonb_build_array(common||'{"venue":"binance","price":"55105"}',common||'{"venue":"okx","price":"55101"}');
    PERFORM oracle_spot_paper.finish_mark('BTC/USD',old_lease,samples);
    IF EXISTS(SELECT FROM oracle_spot_paper.minute_marks) THEN RAISE EXCEPTION 'Expired worker published a mark'; END IF;
    PERFORM oracle_spot_paper.finish_mark('BTC/USD',(job->>'token')::uuid,'[]');
    IF (SELECT mark FROM oracle_spot_paper.positions WHERE member='@valuation_test')<>54999
       OR oracle_social.rankings('USD')->'rankings'<>'[]' THEN RAISE EXCEPTION 'Missing market data refreshed a stale holding'; END IF;
    UPDATE oracle_spot_paper.mark_jobs SET due=now()-interval '1 minute';
    job:=oracle_spot_paper.claim_mark();
    point:=(job->>'closeMs')::bigint;
    common:=common||jsonb_build_object('closeMs',point,'receivedMs',point+1);
    samples:=jsonb_build_array(common||'{"venue":"binance","price":"55105"}',common||'{"venue":"okx","price":"55101"}');
    PERFORM oracle_spot_paper.finish_mark('BTC/USD',(job->>'token')::uuid,samples);
    PERFORM oracle_spot_paper.finish_mark('BTC/USD',(job->>'token')::uuid,samples);
    wallet:=oracle_basket_paper.profile(NULL);
    ranked:=oracle_social.rankings('USD');
    IF (wallet#>>'{balances,0,available}')::numeric IS DISTINCT FROM 4995
       OR (wallet#>>'{balances,0,spotValue}')::numeric IS DISTINCT FROM 55105
       OR (wallet#>>'{balances,0,total}')::numeric IS DISTINCT FROM 60100
       OR (wallet#>>'{balances,0,net}')::numeric IS DISTINCT FROM 100
       OR (wallet#>>'{coins,0,quantity}')::numeric IS DISTINCT FROM 1
       OR (wallet#>>'{coins,0,asOf}')::bigint IS DISTINCT FROM point
       OR wallet->'activity' IS DISTINCT FROM (SELECT activity FROM mark_check)
       OR ranked#>>'{rankings,0,handle}' IS DISTINCT FROM '@valuation_test'
       OR (ranked#>>'{rankings,0,returnPct}')::numeric IS DISTINCT FROM 0.1667
       OR (SELECT count(*) FROM oracle_spot_paper.minute_marks)<>1
    THEN RAISE EXCEPTION 'Marked wallet, ranking or immutable receipt failed to reconcile'; END IF;
END $$;
-- A connection can be disabled after a claim. Only admissible observations
-- contribute. A newer fill on the same pair takes precedence for its holder.
DELETE FROM oracle_spot_paper.minute_marks;
DELETE FROM oracle_spot_paper.mark_jobs;
UPDATE oracle_spot_paper.positions SET as_of=now()-interval '11 minutes';
INSERT INTO oracle_spot_paper.positions VALUES('second','BTC/USD','USD',2,56000,clock_timestamp());
DO $$
DECLARE job jsonb:=oracle_spot_paper.claim_mark(); point bigint:=(job->>'closeMs')::bigint; s jsonb;
BEGIN
    INSERT INTO oracle_admin.venue_connections VALUES('binance','vault:oracle/company/binance',false,1);
    s:=jsonb_build_object('pair','BTC/USD','closeMs',point,'receivedMs',point+1,'metadataDigest',repeat('a',64),'candleDigest',repeat('b',64));
    PERFORM oracle_spot_paper.finish_mark('BTC/USD',(job->>'token')::uuid,
        jsonb_build_array(s||'{"venue":"binance","price":"99999"}',s||'{"venue":"okx","price":"54900"}'));
    IF (SELECT mark FROM oracle_spot_paper.positions WHERE member='@valuation_test') IS DISTINCT FROM 54900
       OR (SELECT mark FROM oracle_spot_paper.positions WHERE member='second') IS DISTINCT FROM 56000
       OR (SELECT jsonb_array_length(observations) FROM oracle_spot_paper.minute_marks) IS DISTINCT FROM 1
    THEN RAISE EXCEPTION 'Disabled source or late observation changed a position'; END IF;
END $$;
ROLLBACK;
