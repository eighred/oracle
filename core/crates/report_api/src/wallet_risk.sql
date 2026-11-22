-- The authenticated profile, eligibility and capture time share a statement snapshot.
WITH account AS MATERIALIZED (SELECT oracle_basket_paper.profile(NULL) AS profile),
balance AS (
    SELECT b FROM account, jsonb_array_elements(profile->'balances') b WHERE b->>'currency'=$1
), basket_cash AS (
    SELECT coalesce(sum((p->>'cash')::numeric),0) AS amount
    FROM account, jsonb_array_elements(profile->'positions') p
    WHERE p->>'currency'=$1 AND p->>'status'<>'Closed'
), holdings AS (
    SELECT coalesce(jsonb_agg(jsonb_build_object(
        'pair',(c->>'asset')||'/'||$1,'source',c->>'venue','value',c->>'value',
        'quantity',c->>'quantity','as_of_ms',c->'asOf') ORDER BY c->>'asset',c->>'venue'),'[]') AS list
    FROM account, jsonb_array_elements(profile->'coins') c WHERE c->>'currency'=$1
)
SELECT jsonb_build_object('currency',$1,'available',b->>'available','basket_cash',amount::text,
    'cash',((b->>'available')::numeric+amount)::text,'nav',b->>'total','holdings',list,
    'captured_ms',floor(extract(epoch FROM statement_timestamp())*1000)::bigint,
    'first',oracle_spot_paper.enabled('binance'),'second',oracle_spot_paper.enabled('okx'))::text
FROM balance,basket_cash,holdings
