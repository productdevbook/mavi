-- Whoever runs the machine. Not a site's user: a different table, a different
-- cookie, a different session, so that one's token is not the other's and no
-- amount of grants on a site adds up to this.
create table operators (
    id            uuid primary key default gen_random_uuid(),
    email         text not null unique check (email = lower(email) and position('@' in email) > 1),
    name          text not null check (length(name) between 1 and 200),
    password_hash text,
    active        boolean not null default true,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now()
);

create trigger operators_touch before update on operators
    for each row execute function touch_updated_at();

create table operator_sessions (
    id           uuid primary key default gen_random_uuid(),
    operator_id  uuid not null references operators (id) on delete cascade,
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

create trigger operator_sessions_touch before update on operator_sessions
    for each row execute function touch_updated_at();

create index operator_sessions_operator_idx on operator_sessions (operator_id);
create index operator_sessions_expiry_idx on operator_sessions (expires_at)
    where revoked_at is null;

-- What a site is called and who to talk to about it. Beside `tenants` rather
-- than in it because `tenants` is what every other table points at, and a
-- column added here does not touch that.
create table site_settings (
    tenant_id   uuid primary key references tenants (id) on delete cascade,
    name        text not null check (length(name) between 1 and 200),
    -- Somebody to reach about the site. Held here, in the control plane, and
    -- never sent to the site's own people.
    contact     text,
    notes       text,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger site_settings_touch before update on site_settings
    for each row execute function touch_updated_at();

-- What an operator did. Separate from `audit_log`, which belongs to a tenant:
-- what happens here is about every site or about none.
create table console_log (
    id          uuid primary key default gen_random_uuid(),
    operator_id uuid references operators (id) on delete set null,
    action      text not null,
    subject     text not null,
    subject_id  text,
    detail      jsonb not null default '{}'::jsonb,
    request_id  text not null,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger console_log_touch before update on console_log
    for each row execute function touch_updated_at();

create index console_log_at_idx on console_log (created_at desc);
create index console_log_operator_idx on console_log (operator_id);
create index console_log_subject_idx on console_log (subject, subject_id);
