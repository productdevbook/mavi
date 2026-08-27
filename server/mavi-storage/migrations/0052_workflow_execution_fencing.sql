-- Hatchet delivers at least once. These fields fence the Rust side effect
-- section without making Mavi a second queue: one delivery owns the execution
-- claim until it finishes or the bounded lease expires after a crash.
alter table workflow_runs
    add column if not exists attempts integer not null default 0,
    add column if not exists claim_token uuid,
    add column if not exists claim_worker text,
    add column if not exists claim_until timestamptz;

alter table workflow_runs
    drop constraint if exists workflow_runs_attempts_check;

alter table workflow_runs
    add constraint workflow_runs_attempts_check check (attempts >= 0);

create index if not exists workflow_runs_claim_expiry
    on workflow_runs (site_id, claim_until)
    where claim_token is not null;
