create extension if not exists pgcrypto;

create or replace function current_tenant_id() returns uuid
language sql stable as $$
    select nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

create or replace function touch_updated_at() returns trigger
language plpgsql as $$
begin
    new.updated_at := now();
    return new;
end $$;

create type tenant_state as enum ('provisioning', 'live', 'suspended', 'removed');

create table tenants (
    id          uuid primary key default gen_random_uuid(),
    slug        text not null unique check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])$'),
    state       tenant_state not null default 'provisioning',
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    removed_at  timestamptz,
    check ((state = 'removed') = (removed_at is not null))
);

create trigger tenants_touch before update on tenants
    for each row execute function touch_updated_at();

create table tenant_domains (
    host        text primary key check (host = lower(host)),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    is_primary  boolean not null default false,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger tenant_domains_touch before update on tenant_domains
    for each row execute function touch_updated_at();

create index tenant_domains_tenant_id_idx on tenant_domains (tenant_id);
create unique index tenant_domains_one_primary on tenant_domains (tenant_id) where is_primary;

create table audit_log (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    actor_id    uuid,
    actor_kind  text not null check (actor_kind in ('user', 'student', 'system', 'operator')),
    action      text not null,
    subject     text not null,
    subject_id  text,
    before      jsonb,
    after       jsonb,
    request_id  text not null,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger audit_log_touch before update on audit_log
    for each row execute function touch_updated_at();

create index audit_log_tenant_at_idx on audit_log (tenant_id, created_at desc);
create index audit_log_subject_idx on audit_log (tenant_id, subject, subject_id);

create type outbox_state as enum ('pending', 'delivering', 'delivered', 'failed');

create table outbox (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    event        text not null,
    subject_id   text,
    payload      jsonb not null,
    state        outbox_state not null default 'pending',
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    delivered_at timestamptz
);

create trigger outbox_touch before update on outbox
    for each row execute function touch_updated_at();

create index outbox_pending_idx on outbox (tenant_id, created_at) where state = 'pending';

create type job_state as enum ('ready', 'running', 'done', 'failed', 'dead');

create table jobs (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid references tenants (id) on delete cascade,
    kind         text not null,
    payload      jsonb not null default '{}'::jsonb,
    state        job_state not null default 'ready',
    run_at       timestamptz not null default now(),
    -- A claim that lapses releases the job; a worker that dies does not take it
    -- with it.
    claimed_until timestamptz,
    claimed_by   text,
    attempts     integer not null default 0 check (attempts >= 0),
    max_attempts integer not null default 5 check (max_attempts > 0),
    last_error   text,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    finished_at  timestamptz,
    check ((state = 'running') = (claimed_until is not null))
);

create trigger jobs_touch before update on jobs
    for each row execute function touch_updated_at();

create index jobs_claimable_idx on jobs (run_at) where state in ('ready', 'running');
create index jobs_tenant_idx on jobs (tenant_id, created_at desc);

alter table audit_log enable row level security;
alter table outbox    enable row level security;
alter table jobs      enable row level security;

alter table audit_log force row level security;
alter table outbox    force row level security;
alter table jobs      force row level security;

create policy tenant_isolation on audit_log
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on outbox
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

-- The one table with a second way in: a worker claims across every tenant and
-- takes control-plane work, which belongs to none. `app.worker` is set only by
-- the claim path in `kernel::queue`.
create policy tenant_isolation on jobs
    using (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    )
    with check (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    );
