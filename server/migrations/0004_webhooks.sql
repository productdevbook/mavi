create table webhook_endpoints (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    -- Plain http is refused before anything is sent unless the destination is
    -- private, which is only ever a test's own receiver.
    url         text not null check (url ~ '^https?://.'),
    -- What a receiver checks the signature with. Rotating it is a new row plus
    -- a window where both are sent, so the version travels with it.
    secret      text not null,
    secret_version integer not null default 1 check (secret_version > 0),
    events      text[] not null default '{}',
    active      boolean not null default true,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger webhook_endpoints_touch before update on webhook_endpoints
    for each row execute function touch_updated_at();

create index webhook_endpoints_tenant_idx on webhook_endpoints (tenant_id) where active;

create table webhook_deliveries (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    endpoint_id  uuid not null references webhook_endpoints (id) on delete cascade,
    outbox_id    uuid not null references outbox (id) on delete cascade,
    attempt      integer not null check (attempt > 0),
    status_code  integer,
    response     text,
    failure      text,
    sent_at      timestamptz not null default now(),
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now()
);

create trigger webhook_deliveries_touch before update on webhook_deliveries
    for each row execute function touch_updated_at();

create index webhook_deliveries_endpoint_idx on webhook_deliveries (endpoint_id, sent_at desc);
create index webhook_deliveries_outbox_idx on webhook_deliveries (outbox_id);
create index webhook_deliveries_tenant_idx on webhook_deliveries (tenant_id, sent_at desc);

alter table webhook_endpoints  enable row level security;
alter table webhook_deliveries enable row level security;
alter table webhook_endpoints  force row level security;
alter table webhook_deliveries force row level security;

create policy tenant_isolation on webhook_endpoints
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on webhook_deliveries
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());
