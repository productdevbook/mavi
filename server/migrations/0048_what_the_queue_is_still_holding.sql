-- A site's own read of its queue counts how many jobs are ready, running,
-- failed or dead, and asks how old the oldest waiting one is — the "what is
-- the queue holding" question, answered without paging through the queue by
-- hand. Nothing sweeps a `done` job yet, so `jobs (tenant_id, created_at)`
-- alone would make that scan every job a site has ever queued to find the
-- handful still open. This index holds only the rows still worth asking
-- about, so the count stays a lookup into the backlog rather than a walk
-- through the site's whole job history.
create index jobs_tenant_backlog_idx on jobs (tenant_id, state) where state <> 'done';
