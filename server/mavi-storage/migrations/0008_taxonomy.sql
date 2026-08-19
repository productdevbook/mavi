create table taxonomy_terms (
    site_id    uuid not null,
    id         uuid not null,
    kind       text not null check (kind in ('category', 'tag')),
    language   text not null check (length(language) between 2 and 35),
    slug       text not null check (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    name       text not null check (length(btrim(name)) between 1 and 100),
    parent_id  uuid,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,
    primary key (site_id, id),
    foreign key (site_id) references site_catalog(site_id),
    foreign key (site_id, parent_id) references taxonomy_terms(site_id, id)
        on delete set null,
    constraint taxonomy_terms_parent_kind check (parent_id is null or kind = 'category'),
    constraint taxonomy_terms_not_self_parent check (parent_id is distinct from id)
);

create unique index taxonomy_terms_site_kind_language_slug
    on taxonomy_terms (site_id, kind, language, slug)
    where deleted_at is null;

create index taxonomy_terms_site_recent
    on taxonomy_terms (site_id, created_at desc, id desc)
    where deleted_at is null;

create index taxonomy_terms_site_parent
    on taxonomy_terms (site_id, parent_id, created_at desc, id desc)
    where deleted_at is null and parent_id is not null;

create table content_term_assignments (
    site_id    uuid not null,
    content_id uuid not null,
    term_id    uuid not null,
    assigned_at timestamptz not null default now(),
    primary key (site_id, content_id, term_id),
    foreign key (site_id, content_id) references content_entries(site_id, id)
        on delete cascade,
    foreign key (site_id, term_id) references taxonomy_terms(site_id, id)
        on delete cascade
);

create index content_term_assignments_site_term
    on content_term_assignments (site_id, term_id, assigned_at desc, content_id desc);

alter table taxonomy_terms enable row level security;
alter table taxonomy_terms force row level security;
create policy taxonomy_terms_scope on taxonomy_terms
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);

alter table content_term_assignments enable row level security;
alter table content_term_assignments force row level security;
create policy content_term_assignments_scope on content_term_assignments
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
