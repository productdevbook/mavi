-- Nothing sweeps a `done` job yet, so asking "what is the queue holding"
-- without an index scoped to what is still open would mean scanning every
-- job a site has ever queued to find the handful still waiting, running or
-- stuck.
create index jobs_tenant_backlog_idx on jobs (tenant_id, state) where state <> 'done';
