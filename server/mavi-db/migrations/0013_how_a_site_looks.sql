-- How a site looks, and how it goes live.
--
-- A site's look is a project of its own, and what the panel writes goes into a
-- set of changes rather than onto the live site. A broken layout is every page
-- of the site at once, and whoever notices is a visitor.

create table changes (
    id         uuid primary key,
    name       text not null check (length(name) between 1 and 200),
    at         text not null default 'writing'
        check (at in ('writing', 'to_look_at', 'broken', 'published')),
    -- Where somebody can look at it, once it has been built. Null until then,
    -- and null again for the published one, which is looked at by going to the
    -- site.
    look_at    text,
    -- What went wrong, kept: "it failed" is not something anybody can act on,
    -- and the file and the line are what fixing it starts from.
    went_wrong text,
    built_at   timestamptz,
    published_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index changes_recent on changes (created_at desc, id desc);

-- At most one set of changes is the published one. The site has one look.
create unique index changes_one_is_published
    on changes ((true))
    where at = 'published';

-- What is in a set of changes: the files it changes, and nothing else. A file
-- nobody touched is whatever the published set says it is.
create table design_files (
    change_id  uuid not null references changes (id) on delete cascade,
    -- Under `src/` or `public/`. Checked in the code rather than here, because
    -- what makes a path safe is a list of rules about its parts rather than
    -- something a regular expression says honestly.
    path       text not null check (length(path) between 1 and 200),
    contents   bytea not null,
    -- Set when the file is removed in this set of changes: a deletion is a
    -- change like any other, and a row missing would mean "unchanged".
    removed    boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),

    primary key (change_id, path)
);

create index design_files_of_a_change on design_files (change_id);
