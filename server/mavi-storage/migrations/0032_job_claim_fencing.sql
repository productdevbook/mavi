-- A worker name identifies a process role, not one particular claim. A
-- restarted process may intentionally use the same name, so the name alone
-- cannot fence an old process from completing a newer claim for the same job.
-- Every claim therefore gets a fresh token and all lease mutations must carry
-- that token back.

alter table jobs
    drop constraint jobs_running_has_lease;

alter table jobs
    add column claim_token uuid;

-- A running row can exist while this migration is applied. Its job id is a
-- stable, unique one-time value for the already-held legacy claim. New
-- claims always receive a fresh token in the application transaction.
-- Migrations run as the application database owner, while jobs uses FORCE RLS
-- for normal requests. Temporarily remove only the FORCE behavior so this
-- one-time backfill can see every site; the policy remains enabled and FORCE
-- is restored before the migration commits.
alter table jobs no force row level security;

update jobs
   set claim_token = id
 where state = 'running';

alter table jobs
    add constraint jobs_running_has_lease check (
        (state = 'running') = (
            claimed_until is not null
            and claimed_by is not null
            and claim_token is not null
        )
        and (state = 'running' or claim_token is null)
    );

alter table jobs force row level security;
