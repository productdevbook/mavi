-- A pause is a Mavi control-plane operation: a pending intent is withheld
-- from the relay, while an already published Hatchet run is cancelled before
-- resume creates a fresh idempotent delivery. Hatchet has no per-run pause
-- endpoint in the supported Go SDK, so this boundary never pretends that a
-- cancelled run is still executing.
alter table workflow_outbox
    drop constraint if exists workflow_outbox_status_check;

alter table workflow_outbox
    add constraint workflow_outbox_status_check
    check (status in ('pending', 'publishing', 'published', 'failed', 'paused', 'cancelled'));

alter table workflow_runs
    drop constraint if exists workflow_runs_status_check;

alter table workflow_runs
    add constraint workflow_runs_status_check
    check (status in ('pending', 'running', 'completed', 'failed', 'paused', 'cancelled'));
