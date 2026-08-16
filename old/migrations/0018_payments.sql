-- What a provider was asked to take, and what it said. No card number, no
-- fragment of one, no token that stands for one: what this holds is the
-- provider's own name for a payment and nothing that could be used to charge
-- anybody.
create type payment_state as enum ('waiting', 'paid', 'failed', 'refunded');

create table payments (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    order_id     uuid not null references orders (id) on delete cascade,
    provider     text not null,
    -- What the provider calls this attempt. Unique, so the same callback
    -- arriving twice is one payment.
    provider_ref text not null,
    state        payment_state not null default 'waiting',
    amount_minor bigint not null check (amount_minor >= 0),
    currency     currency not null,
    -- Where somebody is sent to pay. Kept because it is what a panel shows
    -- when asked "where did they get to".
    pay_at       text,
    failure      text,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    settled_at   timestamptz,
    unique (tenant_id, provider, provider_ref)
);

create trigger payments_touch before update on payments
    for each row execute function touch_updated_at();

create index payments_order_idx on payments (order_id);
create index payments_tenant_idx on payments (tenant_id, created_at desc);
create index payments_waiting_idx on payments (created_at) where state = 'waiting';

alter table payments enable row level security;
alter table payments force row level security;

create policy tenant_isolation on payments
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
