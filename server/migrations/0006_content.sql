-- The languages a site writes in. Not the panel's language, which is English
-- or Turkish and is not the site's business.
create table languages (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    code        text not null check (code ~ '^[a-z]{2}(-[A-Z]{2})?$'),
    name        text not null check (length(name) between 1 and 100),
    is_default  boolean not null default false,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    unique (tenant_id, code)
);

create trigger languages_touch before update on languages
    for each row execute function touch_updated_at();

create index languages_tenant_idx on languages (tenant_id);
create unique index languages_one_default on languages (tenant_id) where is_default;

-- What a site's own kind of thing looks like: the fields it adds beyond what
-- every post has. The fields themselves live in `posts.fields` as jsonb, which
-- is what makes them something the CMS can be asked about rather than only
-- something it can store.
create table content_types (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    key         text not null check (key ~ '^[a-z][a-z0-9_]{0,30}$'),
    name        text not null check (length(name) between 1 and 100),
    fields      jsonb not null default '[]'::jsonb check (jsonb_typeof(fields) = 'array'),
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    unique (tenant_id, key)
);

create trigger content_types_touch before update on content_types
    for each row execute function touch_updated_at();

create index content_types_tenant_idx on content_types (tenant_id);

-- A post is in the feed; a page is not. One table, because everything else
-- about them is the same and two would drift.
create type post_kind as enum ('post', 'page');

create type post_state as enum ('draft', 'scheduled', 'published', 'archived');

create table posts (
    id            uuid primary key default gen_random_uuid(),
    tenant_id     uuid not null references tenants (id) on delete cascade,
    content_type_id uuid references content_types (id) on delete restrict,
    author_id     uuid references users (id) on delete set null,
    language      text not null,
    kind          post_kind not null default 'post',
    state         post_state not null default 'draft',
    slug          text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$'),
    title         text not null check (length(title) between 1 and 300),
    excerpt       text,
    body          text not null default '',
    -- What the site's own kind of thing adds. jsonb, so that "every recipe
    -- under thirty minutes" is a query rather than a scan through text.
    fields        jsonb not null default '{}'::jsonb check (jsonb_typeof(fields) = 'object'),
    seo           jsonb not null default '{}'::jsonb check (jsonb_typeof(seo) = 'object'),
    published_at  timestamptz,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    deleted_at    timestamptz,
    -- A published post has a moment it was published at, and nothing else has.
    check ((state = 'published') = (published_at is not null and published_at <= now())
           or state = 'scheduled'),
    unique (tenant_id, language, slug)
);

create trigger posts_touch before update on posts
    for each row execute function touch_updated_at();

create index posts_tenant_idx on posts (tenant_id, created_at desc);
create index posts_feed_idx on posts (tenant_id, language, published_at desc)
    where state = 'published' and deleted_at is null;
create index posts_author_idx on posts (author_id);
create index posts_type_idx on posts (content_type_id);
-- What makes a custom field something to ask about.
create index posts_fields_idx on posts using gin (fields);

-- One table for both, because a category and a tag differ in whether they nest
-- and in nothing else. Two tables is how the two came to disagree.
create type term_kind as enum ('category', 'tag');

create table terms (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    parent_id   uuid references terms (id) on delete set null,
    kind        term_kind not null,
    language    text not null,
    slug        text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$'),
    name        text not null check (length(name) between 1 and 200),
    description text,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    -- A tag does not nest; only a category does.
    check (parent_id is null or kind = 'category'),
    unique (tenant_id, kind, language, slug)
);

create trigger terms_touch before update on terms
    for each row execute function touch_updated_at();

create index terms_tenant_idx on terms (tenant_id, kind, language);
create index terms_parent_idx on terms (parent_id);

create table post_terms (
    post_id     uuid not null references posts (id) on delete cascade,
    term_id     uuid not null references terms (id) on delete cascade,
    tenant_id   uuid not null references tenants (id) on delete cascade,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    primary key (post_id, term_id)
);

create trigger post_terms_touch before update on post_terms
    for each row execute function touch_updated_at();

create index post_terms_term_idx on post_terms (term_id);
create index post_terms_tenant_idx on post_terms (tenant_id);

-- Where a name used to answer. A slug that changes leaves one of these behind,
-- so that what somebody linked to keeps working.
create table redirects (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    post_id     uuid references posts (id) on delete cascade,
    language    text not null,
    was         text not null,
    now_at      text not null,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    unique (tenant_id, language, was)
);

create trigger redirects_touch before update on redirects
    for each row execute function touch_updated_at();

create index redirects_tenant_idx on redirects (tenant_id);
create index redirects_post_idx on redirects (post_id);

alter table languages     enable row level security;
alter table content_types enable row level security;
alter table posts         enable row level security;
alter table terms         enable row level security;
alter table post_terms    enable row level security;
alter table redirects     enable row level security;

alter table languages     force row level security;
alter table content_types force row level security;
alter table posts         force row level security;
alter table terms         force row level security;
alter table post_terms    force row level security;
alter table redirects     force row level security;

create policy tenant_isolation on languages
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on content_types
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on posts
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on terms
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on post_terms
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on redirects
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());
