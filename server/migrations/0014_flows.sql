create table flows (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    name       text not null check (length(name) between 1 and 200),
    -- What starts it: the name of an event the site emits. One flow, one
    -- trigger; two triggers is two flows and reads better than a condition.
    trigger    text not null,
    active     boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

create trigger flows_touch before update on flows
    for each row execute function touch_updated_at();

create index flows_tenant_idx on flows (tenant_id);
create index flows_trigger_idx on flows (tenant_id, trigger) where active and deleted_at is null;

create type step_kind as enum ('send_mail', 'call_webhook', 'wait', 'add_to_list');

create table flow_steps (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    flow_id    uuid not null references flows (id) on delete cascade,
    kind       step_kind not null,
    config     jsonb not null default '{}'::jsonb check (jsonb_typeof(config) = 'object'),
    position   integer not null check (position >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, flow_id, position)
);

create trigger flow_steps_touch before update on flow_steps
    for each row execute function touch_updated_at();

create index flow_steps_flow_idx on flow_steps (flow_id, position);
create index flow_steps_tenant_idx on flow_steps (tenant_id);

create type run_state as enum ('running', 'waiting', 'done', 'failed');

create table flow_runs (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    flow_id    uuid not null references flows (id) on delete cascade,
    state      run_state not null default 'running',
    -- What set it off, as it was at the time. A run reads this rather than
    -- going back to the row, which may have changed or gone.
    subject    jsonb not null default '{}'::jsonb,
    -- Where it has got to. The step about to run, not the one that ran.
    at_step    integer not null default 0 check (at_step >= 0),
    failure    text,
    started_at timestamptz not null default now(),
    finished_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger flow_runs_touch before update on flow_runs
    for each row execute function touch_updated_at();

create index flow_runs_flow_idx on flow_runs (flow_id, started_at desc);
create index flow_runs_tenant_idx on flow_runs (tenant_id, started_at desc);

create table flow_run_steps (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    run_id     uuid not null references flow_runs (id) on delete cascade,
    step_id    uuid references flow_steps (id) on delete set null,
    position   integer not null,
    outcome    text not null,
    detail     jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger flow_run_steps_touch before update on flow_run_steps
    for each row execute function touch_updated_at();

create index flow_run_steps_run_idx on flow_run_steps (run_id, position);
create index flow_run_steps_step_idx on flow_run_steps (step_id);
create index flow_run_steps_tenant_idx on flow_run_steps (tenant_id);

-- What a step needs to reach something outside. Sealed, with the key's version
-- travelling with it, and never returned by anything.
create table flow_credentials (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    name       text not null check (length(name) between 1 and 100),
    sealed     text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, name)
);

create trigger flow_credentials_touch before update on flow_credentials
    for each row execute function touch_updated_at();

create index flow_credentials_tenant_idx on flow_credentials (tenant_id);

alter table flows            enable row level security;
alter table flow_steps       enable row level security;
alter table flow_runs        enable row level security;
alter table flow_run_steps   enable row level security;
alter table flow_credentials enable row level security;

alter table flows            force row level security;
alter table flow_steps       force row level security;
alter table flow_runs        force row level security;
alter table flow_run_steps   force row level security;
alter table flow_credentials force row level security;

create policy tenant_isolation on flows
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on flow_steps
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on flow_runs
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on flow_run_steps
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on flow_credentials
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
