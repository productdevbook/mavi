-- Workflow idempotency belongs to the site boundary. The single-site runtime
-- currently has one site per database, but keeping the key composite makes
-- RLS fixtures, exports and future reprovisioning safe by construction.
alter table workflow_outbox
    drop constraint if exists workflow_outbox_pkey;

alter table workflow_outbox
    add primary key (site_id, idempotency_key);

alter table workflow_runs
    drop constraint if exists workflow_runs_pkey;

alter table workflow_runs
    add primary key (site_id, idempotency_key);
