-- What a site plugs into: its own mail server, its own payment provider.
--
-- Not a settings bag. Which integrations exist is a list in the code, and a
-- key nobody declared is refused — a table anything can be written into is a
-- table nothing can be said about.
create table plugins (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    key        text not null,
    enabled    boolean not null default true,
    -- The half that is not a secret, and the only half any endpoint reads back.
    settings   jsonb not null default '{}'::jsonb,
    -- The other half, sealed with the machine's keyring. Null where the
    -- integration has no secret to it.
    sealed     text,
    -- What happened when somebody last asked whether it works. A site whose
    -- mail stopped arriving is otherwise a site that finds out from a customer.
    checked_at timestamptz,
    working    boolean,
    note       text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, key)
);

create trigger plugins_touch before update on plugins
    for each row execute function touch_updated_at();

alter table plugins enable row level security;
alter table plugins force row level security;

create policy tenant_isolation on plugins
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
