-- What a writing is filed under.
--
-- One table for categories and tags, because a post's relationship to either
-- is the same relationship: it is filed under a term. Two tables would be the
-- same columns twice and a union on every screen that shows both.
--
-- What separates them is `sort`, and what that word means is a decision rather
-- than a name: a **category** is somewhere a writing lives and a **tag** is
-- something it is about. That difference shows up in exactly one place — a
-- category may have a parent — and nowhere else, which is why it is a column
-- and not a table.

create table terms (
    id         uuid primary key,
    sort       text not null check (sort in ('category', 'tag')),
    language   text not null check (language ~ '^[a-z]{2}(-[A-Za-z0-9]{2,8})*$'),
    slug       text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,126}[a-z0-9])?$'),
    name       text not null check (length(name) between 1 and 100),
    -- Only a category has one. Checked here rather than hoped for: a tag with
    -- a parent is a tree nobody meant to build and nothing knows how to draw.
    parent     uuid references terms (id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,

    constraint only_a_category_has_a_parent
        check (parent is null or sort = 'category'),
    -- A term cannot be its own parent. The deeper cycle — a is under b is
    -- under a — this cannot see, and there is no cheap way to; it is checked
    -- where the parent is set.
    constraint nothing_is_under_itself check (parent is distinct from id)
);

-- The address, within a language and a sort. Partial for the same reason the
-- writings' one is: a slug freed by a deletion can be used again.
create unique index terms_address
    on terms (sort, language, slug)
    where deleted_at is null;

-- What the panel lists, and the keyset it is ordered by, column for column.
create index terms_recent
    on terms (sort, created_at desc, id desc)
    where deleted_at is null;

create index terms_under
    on terms (parent)
    where parent is not null and deleted_at is null;

-- What is filed under what.
--
-- The pair is the key, so filing something twice is filing it once — which is
-- what an editor pressing a button twice means, rather than an error they have
-- to be told about.
create table filed_under (
    writing_id uuid not null references writings (id) on delete cascade,
    term_id    uuid not null references terms (id) on delete cascade,
    created_at timestamptz not null default now(),

    primary key (writing_id, term_id)
);

-- Both directions are asked for. "What is this filed under" is the primary
-- key's own order; "what is filed under this" is not, and without this index
-- it is a scan of everything ever filed.
create index filed_under_the_term on filed_under (term_id, writing_id);
