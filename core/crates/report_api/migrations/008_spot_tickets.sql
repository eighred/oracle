BEGIN;
CREATE SCHEMA oracle_spot_paper;
REVOKE ALL ON SCHEMA oracle_spot_paper FROM PUBLIC;

CREATE TABLE oracle_spot_paper.tickets (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(), member text NOT NULL,
    pair text NOT NULL, currency text NOT NULL, side text NOT NULL CHECK(side IN ('buy','sell')),
    qty numeric NOT NULL CHECK(qty>0), customer_price numeric NOT NULL CHECK(customer_price>0),
    venue_price numeric NOT NULL CHECK(venue_price>0), venue text NOT NULL, fee numeric NOT NULL CHECK(fee>=0),
    spread numeric NOT NULL CHECK(spread>=0), total numeric NOT NULL CHECK(total>0),
    books jsonb NOT NULL, observed_at timestamptz NOT NULL, expires timestamptz NOT NULL,
    settled timestamptz, event_no bigint UNIQUE
);
CREATE INDEX member_ticket_history ON oracle_spot_paper.tickets(member,event_no DESC) WHERE settled IS NOT NULL;
CREATE TABLE oracle_spot_paper.positions (
    member text, pair text, currency text NOT NULL, qty numeric NOT NULL CHECK(qty>=0),
    mark numeric NOT NULL CHECK(mark>0), as_of timestamptz NOT NULL, PRIMARY KEY(member,pair)
);
CREATE FUNCTION oracle_spot_paper.summary(t oracle_spot_paper.tickets) RETURNS jsonb
LANGUAGE sql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
    SELECT jsonb_build_object('id',t.id,'pair',t.pair,'side',t.side,'quantity',t.qty::text,
        'price',t.customer_price::text,'venuePrice',t.venue_price::text,'venue',t.venue,
        'fee',t.fee::text,'spread',t.spread::text,'total',t.total::text,'currency',t.currency,
        'expires',t.expires,'asOf',t.observed_at,'settled',t.settled)
$$;
CREATE FUNCTION oracle_spot_paper.enabled(v text) RETURNS boolean LANGUAGE sql STABLE SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
    SELECT v IN ('binance','okx') AND coalesce((SELECT enabled FROM oracle_admin.venue_connections WHERE venue=v),true)
$$;

-- The API supplies validated server-side levels and modeled venue fees in quote currency.
CREATE FUNCTION oracle_spot_paper.quote(symbol text, direction text, units numeric, levels jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text:=oracle_read.principal(); buy boolean:=direction='buy'; reference numeric;
    best jsonb; px numeric; fees numeric; gross numeric; seen timestamptz; t oracle_spot_paper.tickets;
BEGIN
    IF actor IS NULL THEN RAISE insufficient_privilege; END IF;
    IF direction NOT IN ('buy','sell') OR units<=0 OR jsonb_array_length(levels) NOT BETWEEN 1 AND 2 THEN RAISE invalid_parameter_value; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(actor,23));
    DELETE FROM oracle_spot_paper.tickets WHERE member=actor AND settled IS NULL AND expires<clock_timestamp()-interval '1 hour';
    IF (SELECT count(*) FROM oracle_spot_paper.tickets WHERE member=actor AND settled IS NULL AND expires>clock_timestamp())>=10 THEN RAISE serialization_failure; END IF;
    SELECT CASE WHEN buy THEN max((v->>'price')::numeric) ELSE min((v->>'price')::numeric) END,
        min(to_timestamp((v->>'received_ms')::bigint/1000.0)) INTO reference,seen
        FROM jsonb_array_elements(levels) v WHERE oracle_spot_paper.enabled(v->>'venue');
    IF seen IS NULL OR seen<clock_timestamp()-interval '12 seconds' OR seen>clock_timestamp()+interval '1 second' THEN RAISE serialization_failure; END IF;
    SELECT v INTO STRICT best FROM jsonb_array_elements(levels) v WHERE oracle_spot_paper.enabled(v->>'venue')
        ORDER BY CASE WHEN buy THEN units*(v->>'price')::numeric ELSE -units*(v->>'price')::numeric END
            +(v->>'fee')::numeric,v->>'venue' LIMIT 1;
    px:=(best->>'price')::numeric; fees:=(best->>'fee')::numeric;
    gross:=units*reference;
    INSERT INTO oracle_spot_paper.tickets(member,pair,currency,side,qty,customer_price,venue_price,venue,fee,spread,total,books,observed_at,expires)
        VALUES(actor,symbol,split_part(symbol,'/',2),direction,units,reference,px,best->>'venue',fees,abs(reference-px)*units,
        CASE WHEN buy THEN gross+fees ELSE gross-fees END,levels,seen,seen+interval '15 seconds') RETURNING * INTO t;
    RETURN oracle_spot_paper.summary(t);
END $$;
CREATE FUNCTION oracle_spot_paper.confirm(key uuid) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text:=oracle_read.principal(); t oracle_spot_paper.tickets;
BEGIN
    IF actor IS NULL THEN RAISE insufficient_privilege; END IF;
    SELECT * INTO STRICT t FROM oracle_spot_paper.tickets WHERE id=key AND member=actor FOR UPDATE;
    IF t.settled IS NOT NULL THEN RETURN oracle_spot_paper.summary(t); END IF;
    IF t.expires<=clock_timestamp() OR NOT oracle_spot_paper.enabled(t.venue) THEN RAISE serialization_failure; END IF;
    IF t.side='buy' THEN
        UPDATE oracle_basket_paper.wallets SET available=available-t.total WHERE member=actor AND currency=t.currency AND available>=t.total;
        IF NOT FOUND THEN RAISE numeric_value_out_of_range; END IF;
        INSERT INTO oracle_spot_paper.positions AS pos VALUES(actor,t.pair,t.currency,t.qty,t.venue_price,t.observed_at)
            ON CONFLICT(member,pair) DO UPDATE SET qty=pos.qty+excluded.qty,
            mark=CASE WHEN excluded.as_of>=pos.as_of THEN excluded.mark ELSE pos.mark END,as_of=greatest(pos.as_of,excluded.as_of);
    ELSE
        PERFORM 1 FROM oracle_basket_paper.wallets WHERE member=actor AND currency=t.currency FOR UPDATE;
        UPDATE oracle_spot_paper.positions SET qty=qty-t.qty,mark=CASE WHEN t.observed_at>=as_of THEN t.venue_price ELSE mark END,
            as_of=greatest(as_of,t.observed_at) WHERE member=actor AND pair=t.pair AND qty>=t.qty;
        IF NOT FOUND THEN RAISE numeric_value_out_of_range; END IF;
        UPDATE oracle_basket_paper.wallets SET available=available+t.total WHERE member=actor AND currency=t.currency;
    END IF;
    UPDATE oracle_spot_paper.tickets SET settled=clock_timestamp(),event_no=nextval('oracle_basket_paper.activity_no')
        WHERE id=key RETURNING * INTO t;
    RETURN oracle_spot_paper.summary(t);
END $$;

ALTER FUNCTION oracle_basket_paper.profile(bigint) RENAME TO basket_profile;
CREATE FUNCTION oracle_basket_paper.profile(before_event bigint) RETURNS jsonb LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE base jsonb:=oracle_basket_paper.basket_profile(before_event); actor text:=oracle_read.principal();
    balances jsonb; coins jsonb; activity jsonb;
BEGIN
    SELECT coalesce(jsonb_agg(b||jsonb_build_object('spotValue',v.amount::text,
        'total',((b->>'total')::numeric+v.amount)::text,'net',((b->>'net')::numeric+v.amount)::text,
        'asOf',least((b->>'asOf')::bigint,v.oldest)) ORDER BY b->>'currency'),'[]') INTO balances
        FROM jsonb_array_elements(base->'balances') b CROSS JOIN LATERAL
        (SELECT coalesce(sum(qty*mark),0) amount,min(extract(epoch FROM as_of)*1000)::bigint oldest
         FROM oracle_spot_paper.positions WHERE member=actor AND currency=b->>'currency' AND qty>0) v;
    SELECT coalesce(jsonb_agg(jsonb_build_object('asset',split_part(pair,'/',1),'currency',currency,'venue','Paper spot',
        'quantity',qty::text,'value',(qty*mark)::text,'asOf',(extract(epoch FROM as_of)*1000)::bigint) ORDER BY pair),'[]') INTO coins
        FROM oracle_spot_paper.positions WHERE member=actor AND qty>0;
    SELECT coalesce(jsonb_agg(entry ORDER BY event DESC),'[]') INTO activity FROM (
        SELECT entry,(entry->>'id')::bigint event FROM jsonb_array_elements(base->'activity') entry
        UNION ALL
        SELECT jsonb_build_object('id',event_no::text,'kind','Paper '||side,'currency',currency,'amount',total::text,
            'name',pair,'at',settled,'receipt',jsonb_build_object('fees',fee::text,'spread',spread::text,'fills',
                jsonb_build_array(jsonb_build_object('pair',pair,'side',side,'venue',venue,'quantity',qty::text,
                    'price',venue_price::text,'fee',fee::text,'customerPrice',customer_price::text)))),event_no
            FROM oracle_spot_paper.tickets WHERE member=actor AND settled IS NOT NULL AND (before_event IS NULL OR event_no<before_event)
        ORDER BY event DESC LIMIT 26
    ) page;
    RETURN base||jsonb_build_object('balances',balances,'coins',base->'coins'||coins,'activity',activity);
END $$;
REVOKE ALL ON FUNCTION oracle_basket_paper.profile(bigint) FROM PUBLIC;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA oracle_spot_paper FROM PUBLIC;
GRANT EXECUTE ON FUNCTION oracle_basket_paper.profile(bigint) TO oracle_report_reader;
GRANT USAGE ON SCHEMA oracle_spot_paper TO oracle_report_reader;
GRANT EXECUTE ON FUNCTION oracle_spot_paper.quote(text,text,numeric,jsonb),oracle_spot_paper.confirm(uuid),oracle_spot_paper.enabled(text) TO oracle_report_reader;
COMMIT;
