create table people (
    site_id uuid not null,
    id uuid not null,
    email text not null check (length(email) between 3 and 254),
    name text not null check (length(btrim(name)) between 1 and 120),
    password_hash text not null,
    status text not null default 'active' check (status in ('active', 'suspended', 'removed')),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, email),
    foreign key (site_id) references site_catalog(site_id)
);

create unique index people_site_email_lower on people (site_id, lower(email));

create table roles (
    site_id uuid not null,
    id uuid not null,
    name text not null check (name ~ '^[a-z][a-z0-9_-]{0,63}$'),
    created_at timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, name),
    foreign key (site_id) references site_catalog(site_id)
);

create table role_grants (
    site_id uuid not null,
    role_id uuid not null,
    capability text not null check (capability in ('audit', 'boards', 'content', 'courses', 'design', 'forms', 'mail', 'media', 'people', 'publish', 'settings', 'shop', 'taxonomy', 'trash')),
    action text not null check (action in ('view', 'write', 'delete')),
    primary key (site_id, role_id, capability, action),
    foreign key (site_id, role_id) references roles(site_id, id)
);

create table person_roles (
    site_id uuid not null,
    person_id uuid not null,
    role_id uuid not null,
    primary key (site_id, person_id, role_id),
    foreign key (site_id, person_id) references people(site_id, id),
    foreign key (site_id, role_id) references roles(site_id, id)
);

create table sessions (
    site_id uuid not null,
    id uuid not null,
    person_id uuid not null,
    token_hash bytea not null,
    expires_at timestamptz not null,
    revoked_at timestamptz,
    created_at timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, token_hash),
    foreign key (site_id, person_id) references people(site_id, id)
);

create index sessions_active_lookup on sessions (site_id, token_hash, expires_at)
    where revoked_at is null;

create table api_keys (
    site_id uuid not null,
    id uuid not null,
    person_id uuid not null,
    name text not null check (length(btrim(name)) between 1 and 120),
    prefix text not null check (length(prefix) between 6 and 32),
    secret_hash bytea not null,
    expires_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, prefix),
    foreign key (site_id, person_id) references people(site_id, id)
);

create table api_key_grants (
    site_id uuid not null,
    key_id uuid not null,
    capability text not null check (capability in ('audit', 'boards', 'content', 'courses', 'design', 'forms', 'mail', 'media', 'people', 'publish', 'settings', 'shop', 'taxonomy', 'trash')),
    action text not null check (action in ('view', 'write', 'delete')),
    primary key (site_id, key_id, capability, action),
    foreign key (site_id, key_id) references api_keys(site_id, id)
);

do $$
declare
    table_name text;
begin
    foreach table_name in array array['people', 'roles', 'role_grants', 'person_roles', 'sessions', 'api_keys', 'api_key_grants']
    loop
        execute format('alter table %I enable row level security', table_name);
        execute format('alter table %I force row level security', table_name);
        execute format(
            'create policy %I_scope on %I using (site_id = current_setting(''app.site_id'', true)::uuid) with check (site_id = current_setting(''app.site_id'', true)::uuid)',
            table_name,
            table_name
        );
    end loop;
end $$;
