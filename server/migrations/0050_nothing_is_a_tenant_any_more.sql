-- The last of it: the table every other table pointed at, the addresses that
-- decided which site a request was about, and the function the policies read.
--
-- Nothing above this file computes a site any more — the router stopped asking
-- which one a request was for before any of this, and 0049 took the column out
-- of every table — so what is left here is furniture.
--
-- `tenant_domains` is dropped whole rather than flattened. Its rows were the
-- answer to "which site is this address", and there is no such question now.
-- What a self-hosted installation still wants to know — whether its own
-- address points at it and whether the certificate is right — is asked of
-- `domain_checks`, which stays and which 0049 already unscoped.
--
-- `tenants` had 69 foreign keys pointing at it, every one of them
-- `on delete cascade`, and that cascade *was* the delete-a-site operation: one
-- delete emptied 69 tables. It needs no replacement. Removing the site is
-- dropping the database.
--
-- `current_tenant_id()` is read by nothing once 0049's policies are gone, and
-- `app.tenant_id` — the session setting it read, the only place that name
-- appears in the whole tree — is set by nothing once the application stops
-- setting it, which is the same change as this one.

drop table tenant_domains;

drop table tenants;

drop type tenant_state;

drop function current_tenant_id();
