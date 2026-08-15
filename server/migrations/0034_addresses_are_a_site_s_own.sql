-- Which addresses a site answers on is that site's business.
--
-- `tenant_domains` was made with the control plane and left without a policy,
-- so a query from any site's connection listed every hostname on the machine —
-- which is a list of who else is here. The table is still read before anybody
-- is a site, when an address is being resolved; that read says out loud that it
-- reaches across sites, and nothing else can.
alter table tenant_domains enable row level security;
alter table tenant_domains force row level security;

create policy tenant_isolation on tenant_domains
    using (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    )
    with check (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    );
