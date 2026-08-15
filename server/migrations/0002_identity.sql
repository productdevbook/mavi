-- A role is data the site edits: what it carries is a set of grants, and the
-- policy asks only whether the grant needed is in it. Adding a role is not a
-- deploy.
create table roles (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    key        text not null check (key ~ '^[a-z][a-z0-9_]{0,30}$'),
    name       text not null check (length(name) between 1 and 100),
    grants     text[] not null default '{}',
    built_in   boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, key)
);

create trigger roles_touch before update on roles
    for each row execute function touch_updated_at();

create type user_state as enum ('invited', 'active', 'suspended');

create table users (
    id            uuid primary key default gen_random_uuid(),
    tenant_id     uuid not null references tenants (id) on delete cascade,
    role_id       uuid not null references roles (id) on delete restrict,
    email         text not null check (email = lower(email) and position('@' in email) > 1),
    name          text not null check (length(name) between 1 and 200),
    password_hash text,
    state         user_state not null default 'invited',
    email_proved_at timestamptz,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    deleted_at    timestamptz,
    unique (tenant_id, email)
);

create trigger users_touch before update on users
    for each row execute function touch_updated_at();

create index users_tenant_idx on users (tenant_id);
create index users_role_idx on users (role_id, tenant_id);

-- A panel user and a site's student are different people with different
-- sessions: separate tables, separate cookie names, so one's token is not the
-- other's.
create table sessions (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    user_id      uuid not null references users (id) on delete cascade,
    -- Only what the token hashes to: a copy of the table is not a drawer of
    -- working sessions.
    token_hash   bytea not null unique,
    issued_at    timestamptz not null default now(),
    expires_at   timestamptz not null,
    last_seen_at timestamptz,
    user_agent   text,
    revoked_at   timestamptz,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    check (expires_at > issued_at)
);

create trigger sessions_touch before update on sessions
    for each row execute function touch_updated_at();

create index sessions_user_idx on sessions (user_id, tenant_id);
create index sessions_tenant_idx on sessions (tenant_id);
create index sessions_expiry_idx on sessions (expires_at) where revoked_at is null;

-- One use, an expiry, hashed, and gone once spent.
create type ticket_purpose as enum ('invitation', 'password_reset', 'email_proof');

create table tickets (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    user_id     uuid not null references users (id) on delete cascade,
    purpose     ticket_purpose not null,
    token_hash  bytea not null unique,
    expires_at  timestamptz not null,
    spent_at    timestamptz,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger tickets_touch before update on tickets
    for each row execute function touch_updated_at();

create index tickets_user_idx on tickets (user_id, tenant_id, purpose);
create index tickets_tenant_idx on tickets (tenant_id);

alter table roles    enable row level security;
alter table users    enable row level security;
alter table sessions enable row level security;
alter table tickets  enable row level security;

alter table roles    force row level security;
alter table users    force row level security;
alter table sessions force row level security;
alter table tickets  force row level security;

create policy tenant_isolation on roles
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on users
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on sessions
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on tickets
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());
