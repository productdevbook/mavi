create table content_types (
    site_id     uuid not null references site_catalog (site_id) on delete restrict,
    kind        text not null check (kind ~ '^[a-z][a-z0-9_]{0,30}$'),
    name        text not null check (length(btrim(name)) between 1 and 100),
    fields      jsonb not null default '[]'::jsonb
                check (jsonb_typeof(fields) = 'array'),
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    primary key (site_id, kind)
);

create index content_types_site_created
    on content_types (site_id, created_at, kind);

alter table content_types enable row level security;
alter table content_types force row level security;

create policy content_types_scope on content_types
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
