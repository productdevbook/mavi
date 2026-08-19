create table site_catalog (
    site_id     uuid primary key,
    status      text not null default 'active'
                check (status in ('provisioning', 'active', 'suspended', 'removed')),
    created_at  timestamptz not null default now()
);

create table site_settings (
    site_id     uuid primary key references site_catalog (site_id) on delete restrict,
    name        text not null check (length(name) between 1 and 200),
    timezone    text not null default 'UTC',
    updated_at  timestamptz not null default now()
);

alter table site_settings enable row level security;
alter table site_settings force row level security;

create policy site_settings_scope on site_settings
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);

create index site_catalog_status on site_catalog (status, created_at);
