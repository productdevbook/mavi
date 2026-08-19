create table automation_flows (
    site_id      uuid not null references site_catalog(site_id),
    id           uuid not null,
    name         text not null check (char_length(btrim(name)) between 1 and 200),
    trigger      text not null check (char_length(trigger) between 1 and 120),
    enabled      boolean not null default false,
    version      integer not null default 1 check (version > 0),
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    deleted_at   timestamptz,
    primary key (site_id, id)
);

create unique index automation_flows_site_name_active
    on automation_flows (site_id, lower(name))
    where deleted_at is null;

create index automation_flows_site_trigger_enabled
    on automation_flows (site_id, trigger, id)
    where enabled and deleted_at is null;

create index automation_flows_site_recent
    on automation_flows (site_id, created_at desc, id desc)
    where deleted_at is null;

create table automation_flow_steps (
    site_id    uuid not null references site_catalog(site_id),
    id         uuid not null,
    flow_id    uuid not null,
    position   integer not null check (position >= 0),
    kind       text not null check (char_length(kind) between 1 and 80),
    config     jsonb not null default '{}'::jsonb
               check (jsonb_typeof(config) = 'object'),
    primary key (site_id, id),
    foreign key (site_id, flow_id) references automation_flows(site_id, id) on delete cascade,
    unique (site_id, flow_id, position)
);

create index automation_flow_steps_site_order
    on automation_flow_steps (site_id, flow_id, position, id);

create table automation_runs (
    site_id           uuid not null references site_catalog(site_id),
    id                uuid not null,
    flow_id           uuid not null,
    trigger           text not null,
    source_key        text,
    event             jsonb not null default '{}'::jsonb
                      check (jsonb_typeof(event) = 'object'),
    definition        jsonb not null
                      check (jsonb_typeof(definition) = 'array'),
    state             text not null default 'running'
                      check (state in ('running', 'waiting', 'succeeded', 'failed')),
    current_position  integer not null default 0 check (current_position >= 0),
    retry_count       integer not null default 0 check (retry_count >= 0),
    last_error        text,
    started_at        timestamptz not null default now(),
    updated_at        timestamptz not null default now(),
    finished_at       timestamptz,
    primary key (site_id, id),
    foreign key (site_id, flow_id) references automation_flows(site_id, id) on delete restrict,
    constraint automation_runs_finished_state check (
        (state in ('succeeded', 'failed')) = (finished_at is not null)
    )
);

create index automation_runs_site_flow_recent
    on automation_runs (site_id, flow_id, started_at desc, id desc);

create index automation_runs_site_state_recent
    on automation_runs (site_id, state, started_at desc, id desc);

create unique index automation_runs_site_flow_source
    on automation_runs (site_id, flow_id, source_key)
    where source_key is not null;

create table automation_run_steps (
    site_id      uuid not null references site_catalog(site_id),
    id           uuid not null,
    run_id       uuid not null,
    position     integer not null check (position >= 0),
    attempt      integer not null check (attempt >= 1),
    kind         text not null,
    outcome      text not null check (outcome in ('succeeded', 'waiting', 'failed')),
    detail       jsonb not null default '{}'::jsonb
                check (jsonb_typeof(detail) = 'object'),
    error        text,
    started_at   timestamptz not null default now(),
    finished_at  timestamptz not null,
    primary key (site_id, id),
    foreign key (site_id, run_id) references automation_runs(site_id, id) on delete cascade,
    unique (site_id, run_id, position, attempt)
);

create index automation_run_steps_site_run_order
    on automation_run_steps (site_id, run_id, position, attempt);

do $$
declare
    table_name text;
begin
    foreach table_name in array array[
        'automation_flows',
        'automation_flow_steps',
        'automation_runs',
        'automation_run_steps'
    ]
    loop
        execute format('alter table %I enable row level security', table_name);
        execute format('alter table %I force row level security', table_name);
        execute format(
            'create policy %I_scope on %I using (site_id = current_setting(''app.site_id'', true)::uuid) with check (site_id = current_setting(''app.site_id'', true)::uuid)',
            table_name,
            table_name
        );
    end loop;
end $$;
