BEGIN;
CREATE ROLE oracle_basket_worker NOLOGIN;
CREATE SCHEMA oracle_basket_paper;
REVOKE ALL ON SCHEMA oracle_basket_paper FROM PUBLIC;

CREATE SEQUENCE oracle_basket_paper.activity_no;
CREATE TABLE oracle_basket_paper.wallets (
    member text, currency text, available numeric NOT NULL DEFAULT 0 CHECK (available >= 0),
    PRIMARY KEY (member,currency)
);
CREATE TABLE oracle_basket_paper.deposits (
    id uuid PRIMARY KEY, member text NOT NULL, currency text NOT NULL, amount numeric NOT NULL CHECK (amount>0),
    at timestamptz NOT NULL DEFAULT clock_timestamp(),
    event_no bigint NOT NULL DEFAULT nextval('oracle_basket_paper.activity_no'),
    FOREIGN KEY(member,currency) REFERENCES oracle_basket_paper.wallets
);
CREATE TABLE oracle_basket_paper.accounts (
    id uuid PRIMARY KEY, basket uuid NOT NULL REFERENCES oracle_social.baskets(id),
    member text NOT NULL, currency text NOT NULL, initial_revision int NOT NULL,
    contribution numeric NOT NULL CHECK (contribution > 0),
    cash numeric NOT NULL CHECK (cash >= 0), holdings jsonb NOT NULL DEFAULT '[]',
    applied int NOT NULL DEFAULT 0, exiting boolean NOT NULL DEFAULT false,
    closed boolean NOT NULL DEFAULT false, requested timestamptz NOT NULL DEFAULT clock_timestamp(),
    entered_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    event_no bigint NOT NULL DEFAULT nextval('oracle_basket_paper.activity_no'),
    retry_at timestamptz NOT NULL DEFAULT '-infinity', lease uuid,
    note text, last_receipt jsonb, valuation jsonb, mark_due timestamptz,
    FOREIGN KEY (member,currency) REFERENCES oracle_basket_paper.wallets
);
CREATE UNIQUE INDEX one_open_basket ON oracle_basket_paper.accounts(member,basket) WHERE NOT closed;
CREATE TABLE oracle_basket_paper.postings (
    account uuid REFERENCES oracle_basket_paper.accounts(id), revision int, redemption boolean,
    posted_at timestamptz NOT NULL DEFAULT clock_timestamp(), receipt jsonb NOT NULL,
    event_no bigint NOT NULL DEFAULT nextval('oracle_basket_paper.activity_no'),
    PRIMARY KEY(account,revision,redemption)
);

CREATE FUNCTION oracle_basket_paper.change(op text,p jsonb) RETURNS uuid
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text := oracle_read.principal(); b oracle_social.baskets; a oracle_basket_paper.accounts;
    key uuid := (p->>'request')::uuid; requested_amount numeric; quote text;
BEGIN
    IF actor IS NULL THEN RAISE insufficient_privilege; END IF;
    IF key IS NULL THEN RAISE invalid_parameter_value; END IF;
    IF op='redeem' THEN
        SELECT * INTO a FROM oracle_basket_paper.accounts WHERE id=key AND member=actor FOR UPDATE;
        IF NOT FOUND THEN RAISE no_data_found; END IF;
        IF NOT a.exiting AND NOT a.closed THEN
            UPDATE oracle_basket_paper.accounts SET exiting=true,requested=clock_timestamp(),
                retry_at='-infinity',lease=NULL,note=NULL WHERE id=key;
        END IF;
        RETURN key;
    END IF;
    IF op NOT IN ('fund','deposit') OR coalesce(p->>'amount','') !~ '^[0-9]{1,12}(\.[0-9]{1,8})?$' THEN RAISE invalid_parameter_value; END IF;
    requested_amount := (p->>'amount')::numeric;
    IF requested_amount<=0 THEN RAISE invalid_parameter_value; END IF;
    IF op='deposit' THEN
        quote := p->>'currency';
        IF coalesce(quote,'') !~ '^[A-Z0-9]{1,16}$' THEN RAISE invalid_parameter_value; END IF;
        INSERT INTO oracle_basket_paper.wallets(member,currency) VALUES(actor,quote) ON CONFLICT DO NOTHING;
        INSERT INTO oracle_basket_paper.deposits(id,member,currency,amount) VALUES(key,actor,quote,requested_amount)
            ON CONFLICT DO NOTHING;
        IF FOUND THEN
            UPDATE oracle_basket_paper.wallets SET available=available+requested_amount WHERE member=actor AND currency=quote;
        ELSIF NOT EXISTS(SELECT FROM oracle_basket_paper.deposits d WHERE d.id=key AND
            (d.member,d.currency,d.amount)=(actor,quote,requested_amount)) THEN RAISE serialization_failure; END IF;
        RETURN key;
    END IF;
    SELECT * INTO b FROM oracle_social.baskets WHERE id=(p->>'id')::uuid FOR UPDATE;
    IF NOT FOUND THEN RAISE no_data_found; END IF;
    SELECT * INTO a FROM oracle_basket_paper.accounts WHERE id=key;
    IF FOUND THEN
        IF (a.member,a.basket,a.contribution,a.initial_revision) IS DISTINCT FROM
            (actor,b.id,requested_amount,(p->>'revision')::int) THEN RAISE serialization_failure; END IF;
        RETURN key;
    END IF;
    IF oracle_social.visible(b,actor) IS NOT TRUE THEN RAISE no_data_found; END IF;
    IF b.revision IS DISTINCT FROM (p->>'revision')::int OR EXISTS (
        SELECT FROM oracle_basket_paper.accounts WHERE member=actor AND basket=b.id AND NOT closed
    ) THEN RAISE serialization_failure; END IF;
    quote := split_part(b.items->0->>'pair','/',2);
    INSERT INTO oracle_basket_paper.wallets(member,currency) VALUES(actor,quote) ON CONFLICT DO NOTHING;
    UPDATE oracle_basket_paper.wallets SET available=available-requested_amount
        WHERE member=actor AND currency=quote AND available>=requested_amount;
    IF NOT FOUND THEN RAISE EXCEPTION 'insufficient paper cash' USING ERRCODE='22003'; END IF;
    INSERT INTO oracle_basket_paper.accounts(id,basket,member,currency,initial_revision,contribution,cash)
        VALUES(key,b.id,actor,quote,b.revision,requested_amount,requested_amount);
    RETURN key;
END $$;

-- A revision cannot spend an existing mandate in a different currency.
CREATE FUNCTION oracle_basket_paper.keep_currency() RETURNS trigger LANGUAGE plpgsql
SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF split_part(NEW.items->0->>'pair','/',2) <> split_part(OLD.items->0->>'pair','/',2)
       AND EXISTS (SELECT FROM oracle_basket_paper.accounts WHERE basket=NEW.id AND NOT closed) THEN
        RAISE serialization_failure;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER keep_funded_currency BEFORE UPDATE OF items ON oracle_social.baskets
    FOR EACH ROW EXECUTE FUNCTION oracle_basket_paper.keep_currency();

CREATE FUNCTION oracle_basket_paper.statement() RETURNS jsonb LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text := oracle_read.principal();
BEGIN
    IF actor IS NULL THEN RAISE insufficient_privilege; END IF;
    RETURN jsonb_build_object('wallets',coalesce((
        SELECT jsonb_object_agg(currency,available::text) FROM oracle_basket_paper.wallets WHERE member=actor
    ),'{}'),'accounts',coalesce((SELECT jsonb_agg(jsonb_build_object(
        'id',a.id,'basket',a.basket,'name',r.name,'currency',a.currency,'contribution',a.contribution::text,
        'enteredAt',a.entered_at,'asOfMs',coalesce(a.valuation,a.last_receipt)->'asOfMs','netReturnPct',round(
            (coalesce((coalesce(a.valuation,a.last_receipt)->>'nav')::numeric,a.contribution)-a.contribution)*100/a.contribution,4)::text,
        'cash',a.cash::text,'holdings',a.holdings,'revision',a.applied,'receipt',a.last_receipt,
        'status',CASE WHEN a.closed THEN 'Closed' WHEN a.exiting THEN 'Redeeming'
            WHEN oracle_social.visible(b,actor) IS NOT TRUE THEN 'Access paused'
            WHEN a.applied<b.revision THEN 'Awaiting rebalance' WHEN a.holdings='[]' THEN 'Held in cash' ELSE 'Invested' END,
        'note',CASE WHEN NOT a.closed AND (a.exiting OR oracle_social.visible(b,actor)) THEN a.note END
    ) ORDER BY a.requested,a.id) FROM oracle_basket_paper.accounts a
    JOIN oracle_social.baskets b ON b.id=a.basket
    JOIN oracle_social.revisions r ON r.basket=a.basket AND r.revision=greatest(a.initial_revision,a.applied)
    WHERE a.member=actor),'[]'));
END $$;

-- The latest eligible revision supersedes older, unfilled revisions. Lease and revision are checked at posting.
CREATE FUNCTION oracle_basket_paper.claim() RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE a oracle_basket_paper.accounts; b oracle_social.baskets; token uuid := gen_random_uuid(); cutoff timestamptz; mark_only boolean; job_revision int;
BEGIN
    SELECT account.* INTO a FROM oracle_basket_paper.accounts account JOIN oracle_social.baskets src ON src.id=account.basket
    WHERE NOT account.closed AND account.retry_at<=clock_timestamp() AND (account.exiting OR
        (account.applied<src.revision AND oracle_social.visible(src,account.member)) OR
        (account.holdings<>'[]' AND account.mark_due<=clock_timestamp()))
    ORDER BY account.retry_at,account.requested,account.id LIMIT 1 FOR UPDATE OF account SKIP LOCKED;
    IF NOT FOUND THEN RETURN NULL; END IF;
    SELECT * INTO b FROM oracle_social.baskets WHERE id=a.basket;
    mark_only := NOT a.exiting AND (a.applied=b.revision OR oracle_social.visible(b,a.member) IS NOT TRUE);
    job_revision := CASE WHEN a.exiting OR mark_only THEN a.applied ELSE b.revision END;
    SELECT greatest(a.requested,known_at) INTO cutoff FROM oracle_social.revisions
        WHERE basket=b.id AND revision=greatest(job_revision,a.initial_revision);
    UPDATE oracle_basket_paper.accounts SET lease=token,retry_at=clock_timestamp()+interval '4 minutes' WHERE id=a.id;
    RETURN jsonb_build_object('id',a.id,'token',token,'revision',job_revision,'markOnly',mark_only,
        'cutoff',ceil(extract(epoch FROM cutoff)*1000)::bigint,
        'cash',a.cash::text,'holdings',a.holdings,
        'targets',CASE WHEN a.exiting OR mark_only THEN '[]'::jsonb ELSE b.items END);
END $$;

CREATE FUNCTION oracle_basket_paper.finish(key uuid,token uuid,rev int,mark_only boolean,receipt jsonb,reason text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE a oracle_basket_paper.accounts; b oracle_social.baskets;
BEGIN
    SELECT src.* INTO b FROM oracle_social.baskets src JOIN oracle_basket_paper.accounts account ON account.basket=src.id
        WHERE account.id=key FOR UPDATE OF src;
    SELECT * INTO a FROM oracle_basket_paper.accounts WHERE id=key AND lease=token AND NOT closed FOR UPDATE;
    IF NOT FOUND THEN RETURN; END IF;
    UPDATE oracle_basket_paper.accounts SET lease=NULL,retry_at=clock_timestamp()+interval '5 seconds',note=reason WHERE id=key;
    IF reason IS NOT NULL THEN RETURN; END IF;
    IF mark_only THEN
        IF NOT a.exiting AND rev=a.applied THEN
            UPDATE oracle_basket_paper.accounts SET valuation=receipt,mark_due=clock_timestamp()+interval '5 minutes',
                retry_at=clock_timestamp() WHERE id=key;
        END IF;
        RETURN;
    END IF;
    IF (NOT a.exiting AND (rev<>b.revision OR oracle_social.visible(b,a.member) IS NOT TRUE))
       OR (a.exiting AND rev<>a.applied) THEN RETURN; END IF;
    INSERT INTO oracle_basket_paper.postings(account,revision,redemption,posted_at,receipt) VALUES(key,rev,a.exiting,clock_timestamp(),receipt);
    IF a.exiting THEN
        UPDATE oracle_basket_paper.wallets SET available=available+(receipt->>'cash')::numeric
            WHERE member=a.member AND currency=a.currency;
    END IF;
    UPDATE oracle_basket_paper.accounts SET applied=rev,holdings=receipt->'holdings',
        cash=CASE WHEN a.exiting THEN 0 ELSE (receipt->>'cash')::numeric END,
        closed=a.exiting,last_receipt=receipt,valuation=NULL,mark_due=clock_timestamp()+interval '5 minutes',retry_at='-infinity' WHERE id=key;
END $$;
CREATE OR REPLACE FUNCTION oracle_social.library() RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text := oracle_read.principal(); result jsonb;
BEGIN
    IF actor IS NULL THEN RAISE insufficient_privilege; END IF;
    SELECT coalesce(jsonb_agg(x.entry ORDER BY x.pinned DESC,x.users DESC,x.name,x.id),'[]') INTO result FROM (
        SELECT b.id,b.name,b.pinned,c.users,jsonb_build_object(
            'id',b.id,'name',b.name,'creator',b.owner_name,'items',b.items,'revision',b.revision,'mine',b.owner_name=actor,
            'public',b.public,'pinned',b.pinned,'users',c.users,'joined',EXISTS(SELECT FROM oracle_social.memberships m WHERE m.basket=b.id AND m.member=actor),'sourceRevision',b.source_revision,
            'invites',CASE WHEN b.owner_name=actor THEN (SELECT coalesce(jsonb_agg(member ORDER BY member),'[]') FROM oracle_social.invites WHERE basket=b.id) END,
            'linkActive',b.owner_name=actor AND coalesce(b.link_until>statement_timestamp(),false)
        ) AS entry FROM oracle_social.baskets b
        CROSS JOIN LATERAL (SELECT count(*) AS users FROM (
            SELECT owner_name FROM oracle_social.baskets WHERE source_id=b.id
            UNION SELECT a.member FROM oracle_basket_paper.accounts a WHERE a.basket=b.id AND NOT a.closed AND a.member<>b.owner_name AND oracle_social.visible(b,a.member)
            UNION SELECT member FROM oracle_social.memberships WHERE basket=b.id AND oracle_social.visible(b,member)
        ) participants) c
        WHERE oracle_social.visible(b,actor)
    ) x;
    RETURN jsonb_build_object('baskets',result,'curator',EXISTS(SELECT FROM oracle_social.curators WHERE principal=actor));
END $$;
CREATE FUNCTION oracle_basket_paper.profile(before_event bigint) RETURNS jsonb LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text := oracle_read.principal(); balances jsonb; coins jsonb; activity jsonb;
BEGIN
    IF actor IS NULL THEN RAISE insufficient_privilege; END IF;
    SELECT coalesce(jsonb_agg(jsonb_build_object('currency',w.currency,'available',w.available::text,
        'deposited',d.total::text,'basketValue',p.total::text,'total',(w.available+p.total)::text,
        'net',(w.available+p.total-d.total)::text,'asOf',p.oldest) ORDER BY w.currency),'[]') INTO balances
    FROM oracle_basket_paper.wallets w
    CROSS JOIN LATERAL (SELECT coalesce(sum(amount),0) total FROM oracle_basket_paper.deposits
        WHERE member=actor AND currency=w.currency) d
    CROSS JOIN LATERAL (SELECT coalesce(sum(coalesce((coalesce(valuation,last_receipt)->>'nav')::numeric,cash)),0) total,
        min((coalesce(valuation,last_receipt)->>'asOfMs')::bigint) FILTER (WHERE holdings <> '[]'::jsonb) oldest
        FROM oracle_basket_paper.accounts WHERE member=actor AND currency=w.currency AND NOT closed) p
    WHERE w.member=actor;
    SELECT coalesce(jsonb_agg(jsonb_build_object('asset',asset,'currency',currency,'venue',venue,'quantity',quantity::text,
        'value',valuation::text,'asOf',oldest) ORDER BY currency,valuation DESC,asset,venue),'[]') INTO coins FROM (
        SELECT split_part(f->>'pair','/',1) asset,a.currency,f->>'venue' venue,sum((f->>'after')::numeric) quantity,
            sum((f->>'after')::numeric*(f->>'mark')::numeric) valuation,min((coalesce(a.valuation,a.last_receipt)->>'asOfMs')::bigint) oldest
        FROM oracle_basket_paper.accounts a CROSS JOIN LATERAL jsonb_array_elements(coalesce(a.valuation->'marks',a.last_receipt->'fills')) f
        WHERE a.member=actor AND NOT a.closed AND (f->>'after')::numeric>0 GROUP BY 1,2,3
    ) totals;
    SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY e.event DESC),'[]') INTO activity FROM (
        SELECT * FROM (
            SELECT event_no::text AS id,event_no AS event,at,'Paper deposit' AS kind,currency,amount::text,NULL::text AS name,NULL::jsonb AS receipt
            FROM oracle_basket_paper.deposits WHERE member=actor
            UNION ALL
            SELECT a.event_no::text,a.event_no,a.entered_at,'Basket entry',a.currency,a.contribution::text,r.name,NULL
            FROM oracle_basket_paper.accounts a JOIN oracle_social.revisions r ON (r.basket,r.revision)=(a.basket,a.initial_revision) WHERE member=actor
            UNION ALL
            SELECT p.event_no::text,p.event_no,p.posted_at,CASE WHEN p.redemption THEN 'Redemption' ELSE 'Paper allocation' END,a.currency,
                p.receipt->>'nav',r.name,p.receipt
            FROM oracle_basket_paper.postings p JOIN oracle_basket_paper.accounts a ON a.id=p.account
            JOIN oracle_social.revisions r ON (r.basket,r.revision)=(a.basket,greatest(a.initial_revision,p.revision)) WHERE a.member=actor
        ) events WHERE before_event IS NULL OR event<before_event ORDER BY event DESC LIMIT 26
    ) e;
    RETURN jsonb_build_object('principal',actor,'balances',balances,'coins',coins,'positions',oracle_basket_paper.statement()->'accounts','activity',activity);
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA oracle_basket_paper FROM PUBLIC;
GRANT USAGE ON SCHEMA oracle_basket_paper TO oracle_report_reader,oracle_basket_worker;
GRANT EXECUTE ON FUNCTION oracle_basket_paper.change(text,jsonb),oracle_basket_paper.statement(),oracle_basket_paper.profile(bigint) TO oracle_report_reader;
GRANT EXECUTE ON FUNCTION oracle_basket_paper.claim(),oracle_basket_paper.finish(uuid,uuid,int,boolean,jsonb,text) TO oracle_basket_worker;
COMMIT;
