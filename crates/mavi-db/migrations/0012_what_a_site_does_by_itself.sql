-- What a site does by itself.
--
-- A flow is one trigger and a list of steps. One trigger, not a condition:
-- two triggers is two flows, and reads better than a rule somebody has to
-- work out in their head.

create table flows (
    id         uuid primary key,
    name       text not null check (length(name) between 1 and 200),
    -- What starts it. The list of what can is a fact about the code — which
    -- domains emit what — so it is checked there rather than copied into a
    -- constraint that a migration has to keep up with.
    trigger    text not null,
    -- Off until somebody turns it on. A flow that runs the moment it is saved
    -- is a flow that runs while somebody is still writing it.
    on_        boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

-- What the runner asks for when something happens: the flows that are on and
-- waiting for this. Partial, because most flows are not waiting for most
-- events.
create index flows_waiting_for
    on flows (trigger)
    where on_ and deleted_at is null;

create index flows_recent
    on flows (created_at desc, id desc)
    where deleted_at is null;

create table steps (
    id         uuid primary key,
    flow_id    uuid not null references flows (id) on delete cascade,
    does       text not null,
    -- What this step was told. Checked against what its kind needs before the
    -- flow is written, so a step that cannot run does not exist.
    told       jsonb not null default '{}'::jsonb check (jsonb_typeof(told) = 'object'),
    place      integer not null check (place >= 0),
    created_at timestamptz not null default now(),

    -- Deferred, for the reason the course's is: reordering steps means writing
    -- one into a place another is still in.
    constraint one_step_to_a_place unique (flow_id, place) deferrable initially deferred
);

create index steps_in_order on steps (flow_id, place);

create table runs (
    id         uuid primary key,
    flow_id    uuid not null references flows (id) on delete cascade,
    state      text not null default 'going'
        check (state in ('going', 'waiting', 'done', 'stuck')),
    -- What set it off, as it was at the time. A run reads this rather than
    -- going back to the row, which may have changed and may be gone: a receipt
    -- sent an hour later, about an order that was refunded in the meantime, is
    -- a letter nobody meant to send.
    about      jsonb not null default '{}'::jsonb,
    -- The step about to run, not the one that ran.
    at_step    integer not null default 0 check (at_step >= 0),
    went_wrong text,
    started_at timestamptz not null default now(),
    finished_at timestamptz,

    constraint finished_when_it_finished
        check ((state in ('done', 'stuck')) = (finished_at is not null))
);

create index runs_of_a_flow on runs (flow_id, started_at desc, id desc);

-- What each step of each run actually did. Kept after the run, because "it
-- says it sent the letter" and "the mail host took it" are different claims
-- and somebody will need the second one.
create table run_steps (
    id         uuid primary key,
    run_id     uuid not null references runs (id) on delete cascade,
    place      integer not null,
    went       text not null,
    detail     jsonb not null default '{}'::jsonb,
    at         timestamptz not null default now()
);

create index run_steps_of_a_run on run_steps (run_id, place);
