-- Workflow run history is exposed through an updated-at/idempotency keyset
-- cursor. Keep the ordering index site-scoped so a large run history does not
-- turn the automation screen into an unbounded sort.
create index workflow_runs_site_updated_key
    on workflow_runs (site_id, updated_at desc, idempotency_key desc);
