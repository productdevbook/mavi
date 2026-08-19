create table content_entries (
    site_id uuid not null references site_catalog(site_id),
    id uuid not null,
    kind text not null check (kind ~ '^[a-z][a-z0-9_]{0,30}$'),
    language text not null check (length(language) between 2 and 35),
    slug text not null check (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    title text not null check (length(btrim(title)) between 1 and 200),
    excerpt text,
    body text not null default '',
    fields jsonb not null default '{}'::jsonb check (jsonb_typeof(fields) = 'object'),
    status text not null default 'draft' check (status in ('draft', 'scheduled', 'published', 'archived')),
    scheduled_at timestamptz,
    published_at timestamptz,
    revision integer not null default 1 check (revision > 0),
    deleted_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (site_id, id),
    constraint content_entries_publication_state check (
        (status = 'draft' and scheduled_at is null and published_at is null)
        or (status = 'scheduled' and scheduled_at is not null and published_at is null)
        or (status = 'published' and scheduled_at is null and published_at is not null)
        or (status = 'archived' and scheduled_at is null and published_at is null)
    )
);

create unique index content_entries_site_language_slug
    on content_entries (site_id, language, slug)
    where deleted_at is null;

create index content_entries_site_status_created
    on content_entries (site_id, status, created_at desc, id desc)
    where deleted_at is null;

alter table content_entries enable row level security;
alter table content_entries force row level security;
create policy content_entries_scope on content_entries
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);

create table content_revisions (
    site_id uuid not null,
    content_id uuid not null,
    revision integer not null check (revision > 0),
    kind text not null,
    language text not null,
    slug text not null,
    title text not null,
    excerpt text,
    body text not null,
    fields jsonb not null check (jsonb_typeof(fields) = 'object'),
    status text not null check (status in ('draft', 'scheduled', 'published', 'archived')),
    scheduled_at timestamptz,
    published_at timestamptz,
    created_at timestamptz not null default now(),
    primary key (site_id, content_id, revision),
    foreign key (site_id, content_id) references content_entries(site_id, id),
    constraint content_revisions_publication_state check (
        (status = 'draft' and scheduled_at is null and published_at is null)
        or (status = 'scheduled' and scheduled_at is not null and published_at is null)
        or (status = 'published' and scheduled_at is null and published_at is not null)
        or (status = 'archived' and scheduled_at is null and published_at is null)
    )
);

alter table content_revisions enable row level security;
alter table content_revisions force row level security;
create policy content_revisions_scope on content_revisions
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
