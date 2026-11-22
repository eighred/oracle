BEGIN;
CREATE SCHEMA oracle_social;
REVOKE ALL ON SCHEMA oracle_social FROM PUBLIC;
CREATE TABLE oracle_social.curators (principal text PRIMARY KEY);
CREATE TABLE oracle_social.baskets (
    id uuid PRIMARY KEY,
    owner_name text NOT NULL,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
    items jsonb NOT NULL,
    revision int NOT NULL DEFAULT 1,
    public boolean NOT NULL DEFAULT false,
    pinned boolean NOT NULL DEFAULT false CHECK (NOT pinned OR public),
    source_id uuid REFERENCES oracle_social.baskets(id),
    source_revision int,
    token_hash text UNIQUE,
    password_hash text,
    link_until timestamptz,
    UNIQUE(owner_name,source_id)
);
CREATE TABLE oracle_social.revisions (
    basket uuid REFERENCES oracle_social.baskets(id), revision int, name text NOT NULL, items jsonb NOT NULL,
    known_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY(basket,revision)
);
CREATE FUNCTION oracle_social.record_revision() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    INSERT INTO oracle_social.revisions(basket,revision,name,items) VALUES(NEW.id,NEW.revision,NEW.name,NEW.items);
    RETURN NEW;
END $$;
CREATE TRIGGER record_revision AFTER INSERT OR UPDATE OF revision ON oracle_social.baskets
    FOR EACH ROW EXECUTE FUNCTION oracle_social.record_revision();
CREATE INDEX baskets_source ON oracle_social.baskets(source_id);
CREATE TABLE oracle_social.invites (
    basket uuid REFERENCES oracle_social.baskets(id), member text,
    PRIMARY KEY(basket,member)
);
CREATE TABLE oracle_social.link_members (
    basket uuid REFERENCES oracle_social.baskets(id), member text, token_hash text NOT NULL,
    PRIMARY KEY(basket,member)
);
CREATE TABLE oracle_social.memberships (
    basket uuid REFERENCES oracle_social.baskets(id), member text, joined_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(basket,member)
);
CREATE TABLE oracle_social.events (
    event_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    at timestamptz NOT NULL DEFAULT clock_timestamp(), actor text NOT NULL,
    action text NOT NULL, basket uuid NOT NULL, revision int NOT NULL
);
CREATE FUNCTION oracle_social.visible(b oracle_social.baskets, actor text) RETURNS boolean
LANGUAGE sql STABLE SET search_path=pg_catalog,pg_temp AS $$
    SELECT b.owner_name=actor OR b.public OR EXISTS (
        SELECT FROM oracle_social.invites WHERE basket=b.id AND member=actor
    ) OR (b.link_until>statement_timestamp() AND EXISTS (
        SELECT FROM oracle_social.link_members m WHERE m.basket=b.id AND m.member=actor AND m.token_hash=b.token_hash
    ))
$$;
CREATE FUNCTION oracle_social.library() RETURNS jsonb
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
            UNION SELECT member FROM oracle_social.memberships WHERE basket=b.id AND oracle_social.visible(b,member)
        ) participants) c
        WHERE oracle_social.visible(b,actor)
    ) x;
    RETURN jsonb_build_object('baskets',result,'curator',EXISTS(SELECT FROM oracle_social.curators WHERE principal=actor));
END $$;
CREATE FUNCTION oracle_social.change(op text, p jsonb) RETURNS uuid
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE actor text := oracle_read.principal(); target uuid := (p->>'id')::uuid;
    b oracle_social.baskets; answer uuid; member_name text := p->>'member'; title text := trim(p->>'name');
BEGIN
    IF actor IS NULL THEN RAISE insufficient_privilege; END IF;
    IF op IN ('create','edit') THEN
        IF title IS NULL OR length(title) NOT BETWEEN 1 AND 80 OR jsonb_typeof(p->'items') IS DISTINCT FROM 'array'
            THEN RAISE EXCEPTION 'invalid basket' USING ERRCODE='22023'; END IF;
        IF jsonb_array_length(p->'items') NOT BETWEEN 1 AND 8 OR EXISTS (
            SELECT FROM jsonb_array_elements(p->'items') t WHERE
                coalesce(t->>'venue','') NOT IN ('binance','okx') OR coalesce(t->>'pair','') !~ '^[^/[:space:][:cntrl:]]{1,64}/[^/[:space:][:cntrl:]]{1,64}$'
                OR split_part(t->>'pair','/',1)=split_part(t->>'pair','/',2)
                OR coalesce(t->>'bps','') !~ '^[1-9][0-9]{0,4}$' OR jsonb_typeof(t->'bps') IS DISTINCT FROM 'number'
                OR (t - 'venue' - 'pair' - 'bps') <> '{}'::jsonb
        ) OR (SELECT sum((t->>'bps')::int)<>10000
              OR count(DISTINCT (t->>'venue', t->>'pair'))<>count(*)
              OR count(DISTINCT split_part(t->>'pair','/',2))<>1
              FROM jsonb_array_elements(p->'items') t)
            THEN RAISE EXCEPTION 'invalid allocations' USING ERRCODE='22023'; END IF;
    END IF;
    IF op='create' THEN
        INSERT INTO oracle_social.baskets(id,owner_name,name,items) VALUES(target,actor,title,p->'items')
            ON CONFLICT(id) DO NOTHING RETURNING id INTO answer;
        IF answer IS NULL THEN
            SELECT id INTO answer FROM oracle_social.baskets WHERE id=target AND owner_name=actor AND name=title AND items=p->'items';
            IF answer IS NULL THEN RAISE serialization_failure; END IF;
            RETURN answer;
        END IF;
    ELSE
        IF op='unlock' THEN
            SELECT * INTO b FROM oracle_social.baskets WHERE token_hash=p->>'token_hash'
                AND password_hash=p->>'password_hash' AND link_until>clock_timestamp() FOR UPDATE;
        ELSE
            SELECT * INTO b FROM oracle_social.baskets WHERE id=target FOR UPDATE;
        END IF;
        IF b.id IS NULL THEN RAISE no_data_found; END IF;
        answer:=b.id;
        IF op='unlock' THEN
            INSERT INTO oracle_social.link_members VALUES(b.id,actor,b.token_hash)
                ON CONFLICT(basket,member) DO UPDATE SET token_hash=excluded.token_hash;
        ELSIF op='join' THEN
            IF oracle_social.visible(b,actor) IS NOT TRUE OR b.owner_name=actor THEN RAISE no_data_found; END IF;
            INSERT INTO oracle_social.memberships(basket,member) VALUES(b.id,actor) ON CONFLICT DO NOTHING;
        ELSIF op='leave' THEN
            IF oracle_social.visible(b,actor) IS NOT TRUE THEN RAISE no_data_found; END IF;
            DELETE FROM oracle_social.memberships WHERE basket=b.id AND member=actor;
        ELSIF op='clone' THEN
            IF oracle_social.visible(b,actor) IS NOT TRUE OR b.owner_name=actor THEN RAISE no_data_found; END IF;
            IF p->>'revision' IS NOT NULL AND (p->>'revision')::int<>b.revision THEN RAISE serialization_failure; END IF;
            INSERT INTO oracle_social.baskets(id,owner_name,name,items,source_id,source_revision)
                VALUES(gen_random_uuid(),actor,b.name,b.items,b.id,b.revision)
                ON CONFLICT(owner_name,source_id) DO NOTHING RETURNING id INTO answer;
            IF answer IS NULL THEN SELECT id INTO answer FROM oracle_social.baskets WHERE owner_name=actor AND source_id=b.id; RETURN answer; END IF;
        ELSE
            IF b.owner_name<>actor THEN RAISE no_data_found; END IF;
            CASE op
                WHEN 'edit' THEN
                    IF b.revision IS DISTINCT FROM (p->>'revision')::int THEN RAISE serialization_failure; END IF;
                    UPDATE oracle_social.baskets SET name=title,items=p->'items',revision=revision+1 WHERE id=b.id;
                WHEN 'publish' THEN UPDATE oracle_social.baskets SET public=(p->>'enabled')::boolean,pinned=pinned AND (p->>'enabled')::boolean WHERE id=b.id;
                WHEN 'pin' THEN
                    IF NOT b.public OR NOT EXISTS(SELECT FROM oracle_social.curators WHERE principal=actor) THEN RAISE insufficient_privilege; END IF;
                    UPDATE oracle_social.baskets SET pinned=(p->>'enabled')::boolean WHERE id=b.id;
                WHEN 'invite' THEN
                    IF member_name=actor OR NOT EXISTS(SELECT FROM oracle_read.sessions WHERE principal=member_name AND revoked_at IS NULL AND expires_at>statement_timestamp()) THEN RAISE no_data_found; END IF;
                    INSERT INTO oracle_social.invites VALUES(b.id,member_name) ON CONFLICT DO NOTHING;
                WHEN 'uninvite' THEN DELETE FROM oracle_social.invites WHERE basket=b.id AND member=member_name;
                WHEN 'link' THEN UPDATE oracle_social.baskets SET token_hash=p->>'token_hash',password_hash=p->>'password_hash',link_until=clock_timestamp()+interval '7 days' WHERE id=b.id;
                WHEN 'unlink' THEN UPDATE oracle_social.baskets SET token_hash=NULL,password_hash=NULL,link_until=NULL WHERE id=b.id;
                ELSE RAISE invalid_parameter_value;
            END CASE;
        END IF;
    END IF;
    IF op IN ('publish','uninvite','link','unlink') THEN
        SELECT * INTO b FROM oracle_social.baskets WHERE id=target;
        DELETE FROM oracle_social.memberships m WHERE m.basket=b.id AND oracle_social.visible(b,m.member) IS NOT TRUE;
    END IF;
    INSERT INTO oracle_social.events(actor,action,basket,revision) SELECT actor,op,id,revision FROM oracle_social.baskets WHERE id=answer;
    RETURN answer;
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA oracle_social FROM PUBLIC;
GRANT USAGE ON SCHEMA oracle_social TO oracle_report_reader;
GRANT EXECUTE ON FUNCTION oracle_social.library(),oracle_social.change(text,jsonb) TO oracle_report_reader;
COMMIT;
