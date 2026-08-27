-- Product capabilities are compiled into Mavi. This table only records the
-- per-instance activation decision and configuration; it never loads code.
create table site_plugins (
    site_id     uuid not null references site_catalog(site_id) on delete restrict,
    plugin_id   text not null check (plugin_id in (
        'core', 'writing', 'commerce', 'learning', 'forms', 'messaging',
        'automation', 'boards', 'analytics', 'governance'
    )),
    enabled     boolean not null default false,
    config      jsonb not null default '{}'::jsonb,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    primary key (site_id, plugin_id)
);

alter table site_plugins enable row level security;
alter table site_plugins force row level security;

create policy site_plugins_scope on site_plugins
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);

create index site_plugins_enabled on site_plugins (site_id, enabled, plugin_id);
