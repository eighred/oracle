-- Disposable PostgreSQL test. All records and synthetic credentials roll back.
BEGIN;
SET LOCAL ROLE oracle_report_reader;
SELECT set_config('oracle.operator',oracle_member.issue('https://test.invalid','test-case','ticket_test',repeat('a',64),
    floor(extract(epoch FROM now()))::bigint+120,'synthetic-ticket-check'),true),set_config('oracle.token_digest',repeat('a',64),true);
DO $$
DECLARE book jsonb; buy jsonb; sell jsonb; result jsonb; profile jsonb;
BEGIN
    PERFORM oracle_basket_paper.change('deposit',jsonb_build_object('request',gen_random_uuid(),'currency','USD','amount','60000'));
    book:=jsonb_build_array(jsonb_build_object('venue','binance','price','55000','fee','5','received_ms',floor(extract(epoch FROM now())*1000)),
                            jsonb_build_object('venue','okx','price','54999','fee','5','received_ms',floor(extract(epoch FROM now())*1000)));
    buy:=oracle_spot_paper.quote('BTC/USD','buy',1,book);
    IF (buy->>'total')::numeric<>55005 OR (buy->>'fee')::numeric<>5 OR (buy->>'spread')::numeric<>1 OR buy->>'venue'<>'okx' THEN
        RAISE EXCEPTION 'Incorrect buy quote';
    END IF;
    result:=oracle_spot_paper.confirm((buy->>'id')::uuid);
    IF result IS DISTINCT FROM oracle_spot_paper.confirm((buy->>'id')::uuid) THEN RAISE EXCEPTION 'Duplicate fill'; END IF;
    profile:=oracle_basket_paper.profile(NULL);
    IF (profile#>>'{balances,0,available}')::numeric<>4995 OR (profile#>>'{balances,0,total}')::numeric<>59994
        OR (profile#>>'{coins,0,quantity}')::numeric<>1 THEN RAISE EXCEPTION 'Incorrect funded wallet'; END IF;
    sell:=oracle_spot_paper.quote('BTC/USD','sell',1,book);
    IF (sell->>'total')::numeric<>54994 OR (sell->>'spread')::numeric<>1 OR sell->>'venue'<>'binance' THEN RAISE EXCEPTION 'Incorrect sale'; END IF;
    PERFORM oracle_spot_paper.confirm((sell->>'id')::uuid);
    profile:=oracle_basket_paper.profile(NULL);
    IF (profile#>>'{balances,0,available}')::numeric<>59989 OR jsonb_array_length(profile->'coins')<>0
        OR jsonb_array_length(profile->'activity')<>3 THEN RAISE EXCEPTION 'Sell did not reconcile'; END IF;
    sell:=oracle_spot_paper.quote('BTC/USD','sell',1,book);
    BEGIN
        PERFORM oracle_spot_paper.confirm((sell->>'id')::uuid);
        RAISE EXCEPTION 'Unfunded sale accepted';
    EXCEPTION WHEN numeric_value_out_of_range THEN NULL;
    END;
    book:=jsonb_set(book,'{0,received_ms}','1');
    BEGIN
        PERFORM oracle_spot_paper.quote('BTC/USD','buy',1,book);
        RAISE EXCEPTION 'Expired book accepted';
    EXCEPTION WHEN serialization_failure THEN NULL;
    END;
END $$;
ROLLBACK;
