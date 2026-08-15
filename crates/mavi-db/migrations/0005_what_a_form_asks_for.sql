-- What a form asks for, and what people sent it.
--
-- The second table is the only one in this schema that anybody at all can
-- write to. Everything about it is decided from there: how much of it there
-- can be, how long it is kept, and what it is allowed to contain.

create table forms (
    id         uuid primary key,
    slug       text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,126}[a-z0-9])?$'),
    name       text not null check (length(name) between 1 and 200),
    -- What it asks for. `jsonb` rather than text so that a question about a
    -- field is a query rather than a scan, and an array rather than an object
    -- because the order the questions are asked in is part of the form.
    fields     jsonb not null default '[]'::jsonb check (jsonb_typeof(fields) = 'array'),
    -- Whether it takes anything. A closed form answers the same as one that
    -- was never made, so this is not a way to ask what the site has.
    open       boolean not null default true,
    -- How long what people send is kept. Every table holding somebody's own
    -- words has one of these; a default of "forever" is a decision nobody
    -- made.
    kept_days  integer not null default 365 check (kept_days between 1 and 3650),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

-- The address. Partial, so a slug freed by a deletion can be used again.
create unique index forms_address on forms (slug) where deleted_at is null;

create index forms_recent
    on forms (created_at desc, id desc)
    where deleted_at is null;

-- What people sent.
create table filled (
    id         uuid primary key,
    form_id    uuid not null references forms (id) on delete cascade,
    -- An object, one key per field the form declared. Nothing else is written
    -- here: a key the form never asked for is refused before the insert, so
    -- what is in this column is bounded by what somebody signed in declared.
    answers    jsonb not null check (jsonb_typeof(answers) = 'object'),
    -- Kept to answer "is this one person sending it fifty times", and swept
    -- with the rest of the submission rather than outliving it.
    from_where inet,
    seen_at    timestamptz,
    created_at timestamptz not null default now(),
    deleted_at timestamptz
);

-- What the panel lists: one form's submissions, newest first, and the keyset
-- matches it column for column.
create index filled_recent
    on filled (form_id, created_at desc, id desc)
    where deleted_at is null;

-- The unread ones, which is the screen anybody actually opens. Partial on
-- `seen_at` as well, because "unread" is a small slice of a table that only
-- ever grows.
create index filled_unseen
    on filled (form_id, created_at desc, id desc)
    where deleted_at is null and seen_at is null;

-- What the sweeper reads: everything past its form's own retention. Nothing
-- runs it yet — the queue is not written — and the column is here rather than
-- added later because a retention added afterwards is a default chosen on
-- behalf of rows nobody remembers agreeing to keep.
create index filled_by_age on filled (created_at) where deleted_at is null;
