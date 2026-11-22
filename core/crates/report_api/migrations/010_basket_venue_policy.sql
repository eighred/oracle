BEGIN;

ALTER FUNCTION oracle_basket_paper.finish(uuid,uuid,int,boolean,jsonb,text) RENAME TO finish_internal;
REVOKE EXECUTE ON FUNCTION oracle_basket_paper.finish_internal(uuid,uuid,int,boolean,jsonb,text) FROM oracle_basket_worker;

CREATE FUNCTION oracle_basket_paper.finish(key uuid,token uuid,rev int,mark_only boolean,receipt jsonb,reason text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF reason IS NULL AND NOT mark_only THEN
        LOCK TABLE oracle_admin.venue_connections IN SHARE MODE;
        IF EXISTS (
            SELECT FROM jsonb_array_elements(receipt->'fills') AS fill
            WHERE coalesce((fill->>'quantity')::numeric,1)>0
                AND oracle_spot_paper.enabled(fill->>'venue') IS NOT TRUE
        ) THEN
            reason := 'A required venue is disabled. The paper allocation is pending.';
        END IF;
    END IF;
    PERFORM oracle_basket_paper.finish_internal(key,token,rev,mark_only,receipt,reason);
END $$;
REVOKE EXECUTE ON FUNCTION oracle_basket_paper.finish(uuid,uuid,int,boolean,jsonb,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION oracle_basket_paper.finish(uuid,uuid,int,boolean,jsonb,text) TO oracle_basket_worker;
COMMIT;
