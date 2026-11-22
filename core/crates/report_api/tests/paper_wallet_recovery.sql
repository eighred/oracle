-- Run within the disposable pilot's rolled-back transaction under its synthetic operator identity.
DO $$
DECLARE basket uuid := gen_random_uuid(); account uuid := gen_random_uuid(); original jsonb; replacement jsonb;
    mark_job jsonb; statement jsonb; page jsonb; balances jsonb;
    receipt jsonb := '{"cash":"0.1","nav":"9.9","asOfMs":120000,"holdings":[{"venue":"okx","pair":"SOL/TRY","quantity":"0.2"}],
        "fills":[{"venue":"okx","pair":"SOL/TRY","after":"0.2","mark":"49"}]}';
BEGIN
    IF has_function_privilege('oracle_report_reader','oracle_basket_paper.claim()','execute') OR
       has_table_privilege('oracle_report_reader','oracle_basket_paper.deposits','SELECT') OR
       has_function_privilege('oracle_basket_worker','oracle_basket_paper.change(text,jsonb)','execute')
        THEN RAISE EXCEPTION 'scope leak'; END IF;
    PERFORM oracle_basket_paper.change('deposit',jsonb_build_object('request',gen_random_uuid(),'currency','TRY','amount','10'));
    PERFORM oracle_social.change('create',jsonb_build_object('id',basket,'name','Recovery',
        'items','[{"venue":"okx","pair":"SOL/TRY","bps":10000}]'::jsonb));
    PERFORM oracle_basket_paper.change('fund',jsonb_build_object('id',basket,'request',account,'revision',1,'amount','10'));
    original := oracle_basket_paper.claim();
    PERFORM oracle_basket_paper.finish(account,(original->>'token')::uuid,1,false,NULL,'Price unavailable');
    SELECT oracle_basket_paper.statement() INTO statement;
    IF statement->'accounts'->0->>'cash'<>'10' OR statement->'accounts'->0->>'revision'<>'0' OR
       statement->'wallets'->>'TRY'<>'0' THEN RAISE EXCEPTION 'missing price changed economic state'; END IF;

    UPDATE oracle_basket_paper.accounts SET retry_at='-infinity' WHERE id=account;
    original := oracle_basket_paper.claim();
    IF oracle_basket_paper.claim() IS NOT NULL THEN RAISE EXCEPTION 'leased work reclaimed early'; END IF;
    UPDATE oracle_basket_paper.accounts SET retry_at='-infinity' WHERE id=account;
    replacement := oracle_basket_paper.claim();
    PERFORM oracle_basket_paper.finish(account,(original->>'token')::uuid,1,false,receipt,NULL);
    IF EXISTS(SELECT FROM oracle_basket_paper.postings) THEN RAISE EXCEPTION 'superseded lease posted'; END IF;
    PERFORM oracle_basket_paper.finish(account,(replacement->>'token')::uuid,1,false,receipt,NULL);
    PERFORM oracle_basket_paper.finish(account,(replacement->>'token')::uuid,1,false,receipt,NULL);
    UPDATE oracle_basket_paper.accounts SET mark_due='-infinity' WHERE id=account;
    mark_job := oracle_basket_paper.claim();
    IF mark_job->>'markOnly' IS DISTINCT FROM 'true' THEN RAISE EXCEPTION 'missing periodic valuation'; END IF;
    PERFORM oracle_basket_paper.finish(account,(mark_job->>'token')::uuid,1,true,
        '{"asOfMs":420000,"nav":"10.5","marks":[{"venue":"okx","pair":"SOL/TRY","after":"0.2","mark":"52"}]}',NULL);
    SELECT oracle_basket_paper.profile(NULL) INTO page;
    balances := page->'balances'->0;
    IF balances->>'total' IS DISTINCT FROM '10.5' OR balances->>'net' IS DISTINCT FROM '0.5'
        OR page->'positions'->0->>'netReturnPct' IS DISTINCT FROM '5.0000'
        OR (page->'coins'->0->>'value')::numeric IS DISTINCT FROM 10.4
        OR jsonb_array_length(page->'activity')<>3 THEN RAISE EXCEPTION 'mark-only revaluation changed postings or totals'; END IF;
    FOR i IN 1..26 LOOP
        PERFORM oracle_basket_paper.change('deposit',jsonb_build_object('request',gen_random_uuid(),'currency','USD','amount','1'));
    END LOOP;
    page := oracle_basket_paper.profile(NULL)->'activity';
    IF jsonb_array_length(page)<>26 OR jsonb_array_length(
        oracle_basket_paper.profile((page->24->>'id')::bigint)->'activity')<>4 THEN RAISE EXCEPTION 'history cursor skipped entries'; END IF;
END $$;
