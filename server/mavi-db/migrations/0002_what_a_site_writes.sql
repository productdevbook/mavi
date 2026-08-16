-- What a site writes.
--
-- One table for everything with a title, a body and an address: a post, a
-- page, a course, a property. What kind it is is a column rather than a table,
-- because the alternative is a table per kind and a join per screen, and a
-- site that invents a kind on Tuesday should not need a migration on Wednesday.
--
-- What is *not* in the row is what makes that work: a kind's own fields live
-- in `fields`, as json, and what a kind may hold is declared elsewhere. This
-- table is what every kind has in common, and nothing else.

create table writings (
    id           uuid primary key,
    -- What kind of thing this is. `post` and `page` are here from the start;
    -- a site adds its own. Not an enum: an enum is a migration every time
    -- somebody has an idea.
    kind         text not null check (kind ~ '^[a-z][a-z0-9_]{0,30}$'),
    language     text not null check (language ~ '^[a-z]{2}(-[A-Za-z0-9]{2,8})*$'),
    -- The address, within its language. Unique among what has not been thrown
    -- away: a slug freed by a deletion can be used again, and a slug in use
    -- cannot.
    slug         text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,126}[a-z0-9])?$'),
    title        text not null check (length(title) between 1 and 200),
    -- What a search engine and a chat app show. Falls back to the opening of
    -- the body where a site has not written one.
    excerpt      text,
    body         text not null default '',
    -- A kind's own fields. Empty for a post; a course's price and level live
    -- here.
    fields       jsonb not null default '{}'::jsonb,
    state        text not null default 'draft' check (state in ('draft', 'published')),
    -- When it goes out. In the future for something scheduled; the scheduler
    -- publishes it within the minute.
    published_at timestamptz,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    deleted_at   timestamptz,

    -- Published means published *at* something. The pair is checked here
    -- rather than remembered at each write: a published row with no date is a
    -- row nothing can order, and a draft with one reads as scheduled.
    constraint published_says_when
        check ((state = 'published') = (published_at is not null))
);

-- The address, and the reason it is partial: something in the bin still holds
-- its row, and holding its address as well would mean a site could not write
-- a new page at an address it deleted last year.
create unique index writings_address
    on writings (language, slug)
    where deleted_at is null;

-- What a feed reads, and what the listing's keyset is ordered by. The columns
-- and their order are the keyset's, exactly — an index that does not match the
-- order is an index the planner ignores, and the listing walks the table
-- instead.
create index writings_feed
    on writings (kind, published_at desc, id desc)
    where deleted_at is null and state = 'published';

-- What the panel reads, which is everything including drafts, newest first.
create index writings_recent
    on writings (kind, created_at desc, id desc)
    where deleted_at is null;

-- What the bin reads.
create index writings_thrown
    on writings (deleted_at desc, id desc)
    where deleted_at is not null;
