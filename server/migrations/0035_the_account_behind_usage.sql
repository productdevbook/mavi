-- What a site owes, and what it has paid.
--
-- Usage worked out a charge each month and nothing said whether it had been
-- paid, so "what does this site owe" had no answer on this machine. One line
-- per thing that happened, and the balance is their sum: a number that is
-- stored and a number that is derived cannot disagree if there is only one of
-- them.
create type ledger_kind as enum ('charge', 'payment', 'adjustment');

create table ledger (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    kind        ledger_kind not null,
    -- What this did to what the site owes, in the smallest unit: a charge adds,
    -- a payment takes away, an adjustment does either.
    amount_minor bigint not null,
    currency    currency not null default 'TRY',
    -- The month this is about, where it is about one.
    period      date,
    -- The charge it came from, so a settled month and its line are one thing.
    charge_id   uuid references charges (id) on delete set null,
    note        text,
    -- Whoever wrote it, where somebody did. A charge writes itself.
    by_operator uuid references operators (id) on delete set null,
    created_at  timestamptz not null default now()
);

create index ledger_tenant_idx on ledger (tenant_id, created_at desc);
create index ledger_charge_idx on ledger (charge_id);
create index ledger_operator_idx on ledger (by_operator);

-- One line per charge, per site. A charge id is a uuid and could stand alone,
-- but every unique index here says which site it is about: that is the rule,
-- and an index that is the exception is the one somebody copies.
create unique index ledger_one_line_per_charge on ledger (tenant_id, charge_id)
    where charge_id is not null;

alter table ledger enable row level security;
alter table ledger force row level security;

-- A site sees its own; the machine's own screens see every site's.
create policy tenant_isolation on ledger
    using (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    )
    with check (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    );
