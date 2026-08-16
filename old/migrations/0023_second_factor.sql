-- A second thing somebody has, besides the password they know.
--
-- One per account: two authenticator apps for one login is a way to be locked
-- out by whichever one was set up second.
create table second_factors (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    user_id      uuid not null references users (id) on delete cascade,
    -- Sealed with the machine's keyring, like every other stored secret. A
    -- copy of this table is not a drawer of working authenticators.
    sealed       text not null,
    -- Null until the six digits have been shown to work once. An unconfirmed
    -- row does not stand between anybody and their account.
    confirmed_at timestamptz,
    -- The last step whose code was taken. A code that works twice is a code
    -- somebody read over a shoulder.
    last_step    bigint,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    unique (tenant_id, user_id)
);

create index second_factors_user_idx on second_factors (user_id);

create trigger second_factors_touch before update on second_factors
    for each row execute function touch_updated_at();

-- What gets somebody back in when the phone is gone. Hashed, like a session
-- token: shown once, at the moment the factor is confirmed, and never again.
create table recovery_codes (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    user_id    uuid not null references users (id) on delete cascade,
    code_hash  bytea not null,
    used_at    timestamptz,
    created_at timestamptz not null default now(),
    unique (tenant_id, code_hash)
);

create index recovery_codes_user_idx on recovery_codes (user_id) where used_at is null;

alter table second_factors enable row level security;
alter table second_factors force row level security;
alter table recovery_codes enable row level security;
alter table recovery_codes force row level security;

create policy tenant_isolation on second_factors
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());

create policy tenant_isolation on recovery_codes
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
