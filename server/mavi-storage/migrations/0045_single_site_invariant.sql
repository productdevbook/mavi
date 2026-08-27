-- This binary is intentionally one site per process/database. Existing
-- multi-site data is not deleted: migration stops with an actionable error so
-- an operator can export/reprovision it in a tenant/control-plane service.
do $$
begin
    if (select count(*) from site_catalog) > 1 then
        raise exception using
            message = 'mavi_single_site_invariant: multiple site_catalog rows exist; export and reprovision through the tenant repository before upgrading';
    end if;
end;
$$;

create unique index site_catalog_single_instance on site_catalog ((true));
