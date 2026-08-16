-- Work that happens after the answer.
--
-- One table. What kinds of work exist is not written here: it is a fact about
-- the code — which handlers this binary has — and a check constraint listing
-- them would be a second copy of that, kept up to date by whoever remembers.
-- The queue refuses a kind it does not run where the job is added instead.

create table jobs (
    id            uuid primary key,
    kind          text not null,
    payload       jsonb not null default '{}'::jsonb,
    state         text not null default 'ready'
        check (state in ('ready', 'running', 'done', 'dead')),
    -- When it may be taken. Now, for work queued as something is answered;
    -- later for a retry, and later still for a thing scheduled.
    run_at        timestamptz not null default now(),
    -- A claim that lapses releases the job, so a worker that dies does not
    -- take it with it. Only a running job has one, which is checked rather
    -- than assumed: a claim on a finished job would make it claimable again
    -- the moment the lease ran out.
    claimed_until timestamptz,
    claimed_by    text,
    tries         integer not null default 0 check (tries >= 0),
    went_wrong    text,
    created_at    timestamptz not null default now(),
    finished_at   timestamptz,

    constraint only_running_work_is_claimed
        check ((state = 'running') = (claimed_until is not null))
);

-- What a worker asks for, in the order the claim asks it: the kinds this
-- worker runs, then the oldest that is due. Partial, because `done` and `dead`
-- are most of the table within a week and none of them is ever claimed.
create index jobs_to_take
    on jobs (kind, run_at)
    where state in ('ready', 'running');

-- What somebody looks at when they want to know what is failing. Not partial:
-- the question is asked about the dead ones, which is exactly what the index
-- above leaves out.
create index jobs_recent on jobs (created_at desc, id desc);
