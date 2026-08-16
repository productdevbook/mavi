-- Where a page went.
--
-- Renaming a writing leaves its old address behind. The slug is kept rather
-- than the whole address, because nothing in this software knows where a
-- design puts its posts: `/blog/old` becoming `/blog/new` and `/writing/old`
-- becoming `/writing/new` are the same row.

create table redirects (
    was        text not null,
    language   text not null,
    -- What it is called now. Not a reference to the writing: the point of this
    -- row is to outlive the rename, and following it to a row that has been
    -- renamed again should land where that one now is, which it does because
    -- renaming writes another row.
    now_at     text not null,
    created_at timestamptz not null default now(),

    -- One answer per name per language. Renaming something back to a name it
    -- had before replaces the row rather than making a second one that
    -- contradicts it.
    -- What the edge asks on an address it has no page for is the last part of
    -- it, and this key answers that on its own: an index on `was` alone would
    -- be a second copy of this one's first column.
    primary key (was, language)
);
