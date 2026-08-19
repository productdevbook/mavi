create table site_languages (
    site_id     uuid not null references site_catalog(site_id) on delete restrict,
    tag         text not null check (tag ~ '^[A-Za-z]{2,8}(-[A-Za-z0-9]{1,8})*$'),
    name        text not null check (length(btrim(name)) between 1 and 120),
    is_default  boolean not null default false,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    primary key (site_id, tag)
);

create unique index site_languages_one_default
    on site_languages (site_id)
    where is_default;

alter table site_languages enable row level security;
alter table site_languages force row level security;

create policy site_languages_scope on site_languages
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);

create index site_languages_site_created
    on site_languages (site_id, created_at, tag);
