BEGIN;
-- Staged company connections may name a future CCXT adapter. Paper routing only
-- admits the compiled and tested implementations in oracle_spot_paper.enabled.
ALTER TABLE oracle_admin.venue_connections DROP CONSTRAINT venue_connections_venue_check;
ALTER TABLE oracle_admin.venue_connections ADD CHECK(venue ~ '^[a-z][a-z0-9]{1,31}$');
CREATE TABLE oracle_social.traders (
    handle text PRIMARY KEY, public boolean NOT NULL DEFAULT false,
    bio text NOT NULL CHECK(length(bio)<=240), avatar text NOT NULL CHECK(length(avatar)<=32768),
    rank_opt_in boolean NOT NULL DEFAULT false
);
CREATE TABLE oracle_social.relationships (
    actor text NOT NULL, target text NOT NULL, kind text NOT NULL CHECK(kind IN ('follow','mute','block')),
    PRIMARY KEY(actor,target,kind), CHECK(actor<>target)
);
CREATE TABLE oracle_social.posts (
    seq bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, request uuid NOT NULL UNIQUE,
    author text REFERENCES oracle_social.traders(handle) NOT NULL, pair text NOT NULL,
    body text NOT NULL CHECK(length(trim(body)) BETWEEN 1 AND 600),
    at timestamptz NOT NULL DEFAULT clock_timestamp(), status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','published','hidden'))
);
CREATE INDEX posts_by_author ON oracle_social.posts(author,seq DESC);

CREATE FUNCTION oracle_social.public_baskets(key uuid DEFAULT NULL) RETURNS jsonb LANGUAGE sql STABLE SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
    SELECT jsonb_build_object('baskets',coalesce(jsonb_agg(to_jsonb(list) ORDER BY pinned DESC,users DESC,name,id),'[]')) FROM (
        SELECT b.id,b.name,b.owner_name AS creator,b.pinned,b.revision,b.items,
            (SELECT count(DISTINCT a.member) FROM oracle_basket_paper.accounts a
             WHERE a.basket=b.id AND NOT a.closed AND a.member<>b.owner_name) AS users
        FROM oracle_social.baskets b WHERE b.public AND (key IS NULL OR b.id=key)
        ORDER BY pinned DESC,users DESC,b.name,b.id LIMIT 50
    ) list
$$;
CREATE FUNCTION oracle_social.member_invite(key uuid, recipient text) RETURNS uuid LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text:=oracle_read.principal(); current_revision int;
BEGIN
    SELECT revision INTO STRICT current_revision FROM oracle_social.baskets WHERE id=key AND owner_name=actor FOR UPDATE;
    IF actor=recipient OR NOT EXISTS(SELECT FROM oracle_member.members WHERE handle=recipient) THEN RAISE no_data_found; END IF;
    INSERT INTO oracle_social.invites VALUES(key,recipient) ON CONFLICT DO NOTHING;
    IF FOUND THEN INSERT INTO oracle_social.events(actor,action,basket,revision) VALUES(actor,'invite',key,current_revision); END IF;
    RETURN key;
END $$;
CREATE FUNCTION oracle_social.can_read_trader(viewer text,owner text) RETURNS boolean LANGUAGE sql STABLE
SET search_path=pg_catalog,pg_temp AS $$
    SELECT viewer=owner OR (EXISTS(SELECT FROM oracle_social.traders WHERE handle=owner AND public)
        AND NOT EXISTS(SELECT FROM oracle_social.relationships WHERE (actor=viewer AND target=owner AND kind IN ('block','mute'))
        OR (actor=owner AND target=viewer AND kind='block')))
$$;
CREATE FUNCTION oracle_social.community(view_name text,person text,cursor_id bigint) RETURNS jsonb LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE reader text:=oracle_read.principal(); moderator boolean; items jsonb; profile jsonb;
BEGIN
    IF reader IS NULL THEN RAISE insufficient_privilege; END IF;
    IF view_name NOT IN ('global','following','profile','review') THEN RAISE invalid_parameter_value; END IF;
    SELECT EXISTS(SELECT FROM oracle_social.curators WHERE principal=reader) INTO moderator;
    IF view_name='review' AND NOT moderator THEN RAISE insufficient_privilege; END IF;
    IF view_name='profile' THEN
        SELECT jsonb_build_object('handle',t.handle,'bio',t.bio,'avatar',t.avatar,'public',t.public,
            'ranking',t.rank_opt_in,'following',EXISTS(SELECT FROM oracle_social.relationships WHERE actor=oracle_read.principal() AND target=t.handle AND kind='follow'))
            INTO profile FROM oracle_social.traders t WHERE t.handle=person AND oracle_social.can_read_trader(reader,person);
        IF profile IS NULL AND person<>reader THEN RAISE no_data_found; END IF;
    END IF;
    SELECT coalesce(jsonb_agg(to_jsonb(page) ORDER BY seq DESC),'[]') INTO items FROM (
        SELECT p.seq::text id,p.seq,p.author,p.body,p.pair,p.at,p.status,t.avatar FROM oracle_social.posts p JOIN oracle_social.traders t ON t.handle=p.author
        WHERE (cursor_id IS NULL OR p.seq<cursor_id) AND (
        (view_name='review' AND p.status='pending' AND t.public) OR
        (view_name<>'review' AND oracle_social.can_read_trader(reader,p.author) AND (p.status='published' OR (p.author=reader AND view_name='profile'))
          AND (view_name<>'profile' OR p.author=person)
          AND (view_name<>'following' OR EXISTS(SELECT FROM oracle_social.relationships r WHERE r.actor=oracle_read.principal() AND r.target=p.author AND r.kind='follow'))))
        ORDER BY p.seq DESC LIMIT 26
    ) page;
    RETURN jsonb_build_object('viewer',reader,'moderator',moderator,'profile',profile,'posts',items);
END $$;
CREATE FUNCTION oracle_social.community_change(op text,p jsonb) RETURNS void LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE viewer text:=oracle_read.principal(); person text:=p->>'handle'; enabled boolean:=(p->>'enabled')::boolean;
BEGIN
    IF viewer IS NULL THEN RAISE insufficient_privilege; END IF;
    IF op='profile' THEN
        INSERT INTO oracle_social.traders VALUES(viewer,(p->>'public')::boolean,trim(p->>'bio'),p->>'avatar',(p->>'ranking')::boolean)
        ON CONFLICT(handle) DO UPDATE SET public=excluded.public,bio=excluded.bio,avatar=excluded.avatar,rank_opt_in=excluded.rank_opt_in;
    ELSIF op IN ('follow','mute','block') THEN
        IF viewer=person THEN RAISE invalid_parameter_value; END IF;
        IF enabled THEN
            IF NOT EXISTS(SELECT FROM oracle_social.traders WHERE handle=person AND public) OR (op='follow' AND NOT oracle_social.can_read_trader(viewer,person)) THEN RAISE no_data_found; END IF;
            INSERT INTO oracle_social.relationships VALUES(viewer,person,op) ON CONFLICT DO NOTHING;
        ELSE DELETE FROM oracle_social.relationships WHERE actor=viewer AND target=person AND kind=op;
        END IF;
    ELSIF op='post' THEN
        IF NOT EXISTS(SELECT FROM oracle_social.traders WHERE handle=viewer AND public) THEN RAISE insufficient_privilege; END IF;
        INSERT INTO oracle_social.posts(request,author,pair,body) VALUES((p->>'request')::uuid,viewer,p->>'pair',trim(p->>'body')) ON CONFLICT(request) DO NOTHING;
        IF NOT FOUND AND NOT EXISTS(SELECT FROM oracle_social.posts WHERE request=(p->>'request')::uuid AND author=viewer AND body=trim(p->>'body') AND pair=p->>'pair') THEN RAISE serialization_failure; END IF;
    ELSIF op='remove' THEN
        UPDATE oracle_social.posts SET status='hidden' WHERE seq=(p->>'id')::bigint AND author=viewer;
        IF NOT FOUND THEN RAISE no_data_found; END IF;
    ELSIF op='moderate' THEN
        IF NOT EXISTS(SELECT FROM oracle_social.curators WHERE principal=viewer) THEN RAISE insufficient_privilege; END IF;
        UPDATE oracle_social.posts SET status=CASE WHEN enabled THEN 'published' ELSE 'hidden' END WHERE seq=(p->>'id')::bigint AND status='pending';
        IF NOT FOUND THEN RAISE no_data_found; END IF;
    ELSE RAISE invalid_parameter_value;
    END IF;
END $$;
CREATE FUNCTION oracle_social.rankings(unit text) RETURNS jsonb LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path=pg_catalog,pg_temp AS $$
DECLARE viewer text:=oracle_read.principal(); list jsonb;
BEGIN
    IF viewer IS NULL THEN RAISE insufficient_privilege; END IF;
    SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY rank,handle),'[]') INTO list FROM (
        SELECT handle,round(pct,4)::text AS "returnPct",started,rank() OVER (ORDER BY pct DESC) AS rank FROM (
            SELECT t.handle,d.started,100*(w.available+b.nav+p.nav-d.capital)/d.capital AS pct
            FROM oracle_social.traders t JOIN oracle_basket_paper.wallets w ON w.member=t.handle AND w.currency=unit
            CROSS JOIN LATERAL (SELECT sum(amount) capital,min(at) started FROM oracle_basket_paper.deposits
                WHERE member=t.handle AND currency=unit) d
            CROSS JOIN LATERAL (SELECT coalesce(sum(coalesce((coalesce(a.valuation,a.last_receipt)->>'nav')::numeric,a.cash)),0) nav,
                bool_or(a.holdings<>'[]' AND coalesce((coalesce(a.valuation,a.last_receipt)->>'asOfMs')::bigint,0) < extract(epoch FROM statement_timestamp()-interval '10 min')*1000) stale
                FROM oracle_basket_paper.accounts a WHERE a.member=t.handle AND a.currency=unit AND NOT a.closed) b
            CROSS JOIN LATERAL (SELECT coalesce(sum(qty*mark),0) nav,bool_or(as_of<statement_timestamp()-interval '10 min') stale
                FROM oracle_spot_paper.positions WHERE member=t.handle AND currency=unit AND qty>0) p
            WHERE t.public AND t.rank_opt_in AND oracle_social.can_read_trader(viewer,t.handle) AND d.capital>0
                AND coalesce(b.stale,false)=false AND coalesce(p.stale,false)=false
                AND (EXISTS(SELECT FROM oracle_spot_paper.tickets WHERE member=t.handle AND currency=unit AND settled IS NOT NULL)
                    OR EXISTS(SELECT FROM oracle_basket_paper.accounts WHERE member=t.handle AND currency=unit AND applied>0))
        ) valued ORDER BY rank,handle LIMIT 50
    ) r;
    RETURN jsonb_build_object('rankings',list,'currency',unit,'asOf',statement_timestamp());
END $$;
REVOKE ALL ON FUNCTION oracle_social.rankings(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION oracle_social.rankings(text) TO oracle_report_reader;
REVOKE ALL ON FUNCTION oracle_social.can_read_trader(text,text),oracle_social.community(text,text,bigint),oracle_social.community_change(text,jsonb),
    oracle_social.member_invite(uuid,text),oracle_social.public_baskets(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION oracle_social.community(text,text,bigint),oracle_social.community_change(text,jsonb),
    oracle_social.member_invite(uuid,text),oracle_social.public_baskets(uuid) TO oracle_report_reader;
COMMIT;
