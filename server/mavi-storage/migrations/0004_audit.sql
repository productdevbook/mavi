create table audit_events (
    site_id uuid not null references site_catalog(site_id),
    id uuid not null,
    request_id uuid not null,
    actor_kind text not null check (actor_kind in ('public', 'account', 'student', 'assistant')),
    actor_id text,
    action text not null check (length(action) between 1 and 160),
    resource_type text not null check (length(resource_type) between 1 and 80),
    resource_id uuid,
    payload jsonb not null default '{}'::jsonb check (jsonb_typeof(payload) = 'object'),
    created_at timestamptz not null default now(),
    primary key (site_id, id)
);

create index audit_events_site_created on audit_events (site_id, created_at desc, id desc);
create index audit_events_site_resource on audit_events (site_id, resource_type, resource_id, created_at desc);

alter table audit_events enable row level security;
alter table audit_events force row level security;
create policy audit_events_scope on audit_events
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
