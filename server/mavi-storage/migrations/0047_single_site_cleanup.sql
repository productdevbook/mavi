-- The single-site runtime no longer owns relocation fences or a lifecycle
-- state machine. The tenant/control-plane repository owns those concerns;
-- Mavi keeps only the immutable site identity required by foreign keys/RLS.
drop table if exists site_write_fences;

drop index if exists site_catalog_status;
alter table site_catalog
    drop column if exists status,
    drop column if exists created_at;

drop index if exists workflow_outbox_pending;
create index workflow_outbox_pending
    on workflow_outbox (site_id, available_at, created_at)
    where status in ('pending', 'publishing', 'failed');
