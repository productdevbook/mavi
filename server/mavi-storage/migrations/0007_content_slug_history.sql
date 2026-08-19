create table content_slug_history (
    site_id     uuid not null,
    content_id  uuid not null,
    language    text not null check (length(language) between 2 and 35),
    slug        text not null check (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    created_at  timestamptz not null default now(),
    primary key (site_id, content_id, language, slug),
    foreign key (site_id, content_id) references content_entries (site_id, id)
        on delete restrict
);

create index content_slug_history_lookup
    on content_slug_history (site_id, language, slug, created_at desc);

alter table content_slug_history enable row level security;
alter table content_slug_history force row level security;

create policy content_slug_history_scope on content_slug_history
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
