-- What a site used, written down as it happens rather than worked out later
-- from whatever is still there. A site removed halfway through a month has
-- half a month of these, which is the whole reason they exist.
create type usage_kind as enum ('storage_bytes_day', 'mail_sent', 'build_seconds', 'bandwidth_bytes');

create table usage_events (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    kind       usage_kind not null,
    quantity   bigint not null check (quantity >= 0),
    -- The day it belongs to, so that a reading taken twice is one reading.
    on_day     date not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, kind, on_day)
);

create trigger usage_events_touch before update on usage_events
    for each row execute function touch_updated_at();

create index usage_events_tenant_idx on usage_events (tenant_id, on_day desc);

create type charge_state as enum ('open', 'settled', 'void');

-- One per site per month. Settled once, from the events, and never worked out
-- again from live state — which is what made the old bill unreproducible.
create table charges (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    period      date not null,
    state       charge_state not null default 'open',
    lines       jsonb not null default '[]'::jsonb check (jsonb_typeof(lines) = 'array'),
    total_minor bigint not null default 0 check (total_minor >= 0),
    currency    currency not null default 'TRY',
    settled_at  timestamptz,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    unique (tenant_id, period),
    check ((state = 'settled') = (settled_at is not null))
);

create trigger charges_touch before update on charges
    for each row execute function touch_updated_at();

create index charges_tenant_idx on charges (tenant_id, period desc);

alter table usage_events enable row level security;
alter table charges      enable row level security;
alter table usage_events force row level security;
alter table charges      force row level security;

create policy tenant_isolation on usage_events
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on charges
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
