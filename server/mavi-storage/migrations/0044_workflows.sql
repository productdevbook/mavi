-- Outbox records are written in the same SiteTx as the domain mutation. The
-- Hatchet bridge later publishes the small payload and may safely retry it.
create table workflow_outbox (
    id              uuid not null unique,
    idempotency_key text primary key,
    site_id         uuid not null references site_catalog(site_id) on delete restrict,
    plugin_id       text not null check (plugin_id in (
        'core', 'writing', 'commerce', 'learning', 'forms', 'messaging',
        'automation', 'boards', 'analytics', 'governance'
    )),
    workflow        text not null check (length(workflow) between 1 and 160),
    payload         jsonb not null default '{}'::jsonb,
    status          text not null default 'pending'
                    check (status in ('pending', 'publishing', 'published', 'failed', 'cancelled')),
    attempts        integer not null default 0 check (attempts >= 0),
    available_at    timestamptz not null default now(),
    last_error      text,
    created_at      timestamptz not null default now(),
    published_at    timestamptz,
    unique (site_id, idempotency_key)
);

create index workflow_outbox_pending
    on workflow_outbox (site_id, available_at, created_at)
    where status in ('pending', 'publishing', 'failed');

alter table workflow_outbox enable row level security;
alter table workflow_outbox force row level security;

create policy workflow_outbox_scope on workflow_outbox
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);

create table workflow_runs (
    idempotency_key text primary key,
    site_id         uuid not null references site_catalog(site_id) on delete restrict,
    plugin_id       text not null check (plugin_id in (
        'core', 'writing', 'commerce', 'learning', 'forms', 'messaging',
        'automation', 'boards', 'analytics', 'governance'
    )),
    workflow        text not null check (length(workflow) between 1 and 160),
    hatchet_run_id  text,
    status          text not null default 'pending'
                    check (status in ('pending', 'running', 'completed', 'failed', 'cancelled')),
    created_at      timestamptz not null default now(),
    updated_at      timestamptz not null default now(),
    foreign key (site_id, idempotency_key)
        references workflow_outbox(site_id, idempotency_key) on delete restrict
);

create index workflow_runs_site_status on workflow_runs (site_id, status, updated_at desc);

alter table workflow_runs enable row level security;
alter table workflow_runs force row level security;

create policy workflow_runs_scope on workflow_runs
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
