alter table role_grants drop constraint role_grants_capability_check;
alter table role_grants add constraint role_grants_capability_check check (
    capability in (
        'audit', 'analytics', 'automation', 'boards', 'content', 'courses',
        'credentials', 'design', 'forms', 'mail', 'media', 'people',
        'portable', 'publish', 'settings', 'shop', 'taxonomy', 'trash'
    )
);

alter table api_key_grants drop constraint api_key_grants_capability_check;
alter table api_key_grants add constraint api_key_grants_capability_check check (
    capability in (
        'audit', 'analytics', 'automation', 'boards', 'content', 'courses',
        'credentials', 'design', 'forms', 'mail', 'media', 'people',
        'portable', 'publish', 'settings', 'shop', 'taxonomy', 'trash'
    )
);

create table site_credentials (
    site_id        uuid not null,
    id             uuid not null,
    provider       text not null check (provider ~ '^[a-z][a-z0-9_-]{0,63}$'),
    name           text not null check (name ~ '^[a-z][a-z0-9_-]{0,119}$'),
    sealed_payload bytea not null check (octet_length(sealed_payload) > 0),
    version        bigint not null default 1 check (version > 0),
    revoked_at     timestamptz,
    created_at     timestamptz not null default now(),
    updated_at     timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id) references site_catalog(site_id)
);

create unique index site_credentials_active_name
    on site_credentials (site_id, provider, lower(name))
    where revoked_at is null;

create index site_credentials_site_recent
    on site_credentials (site_id, created_at asc, id asc);

alter table site_credentials enable row level security;
alter table site_credentials force row level security;
create policy site_credentials_scope on site_credentials
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
