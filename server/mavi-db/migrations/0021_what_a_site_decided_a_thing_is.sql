-- What a site decided one of its kinds of writing is.
--
-- A writing's `kind` is free text on purpose: a CMS whose kinds are fixed at
-- compile time is a CMS for one site. But free text alone means a site can
-- have a `recipe` and nothing anywhere knows a recipe has a cooking time — so
-- `fields` was a `jsonb` column somebody typed into a box, checked by nothing
-- and drawable by nothing.
--
-- This is where a site says what one of its kinds asks for. A kind with no row
-- here keeps working exactly as before: whatever is in `fields` is kept. A
-- kind with one is checked against it, and a panel can draw the right boxes.
create table kinds (
    -- The kind itself, which is what a writing carries. Not an id: what points
    -- at this is a word in a column, and a second identifier for the same
    -- thing is two answers to which kind something is.
    kind       text primary key
        check (kind ~ '^[a-z][a-z0-9_]{0,30}$'),
    name       text not null check (length(name) between 1 and 100),
    -- The same shape a form's fields are, and the same vocabulary — two things
    -- that declare what they want and then take whatever arrives are one idea.
    fields     jsonb not null default '[]'::jsonb
        check (jsonb_typeof(fields) = 'array'),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
