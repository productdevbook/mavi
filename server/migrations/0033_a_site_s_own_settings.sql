-- The one table a site can read across every site.
--
-- `site_settings` was made with the control plane and never given a policy, so
-- a query against it from a site's own connection saw every site's row — and
-- one did: the name in a letter came out as whichever site was first. Nothing
-- secret was in it, and that is not the point: every other table is safe
-- because Postgres refuses, not because the query remembered.
alter table site_settings enable row level security;
alter table site_settings force row level security;

create policy tenant_isolation on site_settings
    using (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    )
    with check (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    );
