-- What a site writes to people.
--
-- Three tables: what its own letters say, who it writes to, and which lists
-- they are on. What actually puts a letter on the wire is not here — that
-- belongs with the queue, because a letter that fails has to be tried again.

-- What a site says instead of what this machine says.
--
-- One row per kind and language, so a site that writes its Turkish does not
-- lose its English. A kind is checked in the code against a closed list rather
-- than here: the list of letters this machine sends is a fact about the code,
-- and a check constraint naming them would be a second copy of it that a
-- migration has to keep up with.
create table letters (
    kind       text not null,
    language   text not null check (language ~ '^[a-z]{2}(-[A-Za-z0-9]{2,8})*$'),
    subject    text not null check (length(subject) between 1 and 300),
    body       text not null check (length(body) between 1 and 20000),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),

    primary key (kind, language)
);

-- Somebody a site writes to.
create table readers (
    id         uuid primary key,
    -- Folded before it is written, so one address is not two readers. Checked
    -- here as well because the fold is what the unique index below means.
    email      text not null check (email = lower(email) and position('@' in email) > 1),
    name       text check (length(name) between 1 and 200),
    standing   text not null default 'subscribed'
        check (standing in ('subscribed', 'unsubscribed', 'bounced', 'complained')),
    -- What the link at the bottom of a letter carries, hashed. A link sitting
    -- in somebody's inbox is a link in whatever else reads that inbox, and a
    -- token kept as it was is one a copy of the database hands over.
    way_out    bytea not null unique,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    left_at    timestamptz
);

create unique index readers_address on readers (email);

create index readers_recent on readers (created_at desc, id desc);

-- A site's lists.
create table mail_lists (
    id         uuid primary key,
    name       text not null check (length(name) between 1 and 200),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

-- Who is on what. The pair is the key, so adding somebody twice adds them
-- once — which is what pressing a button twice means.
create table on_a_list (
    reader_id  uuid not null references readers (id) on delete cascade,
    list_id    uuid not null references mail_lists (id) on delete cascade,
    created_at timestamptz not null default now(),

    primary key (reader_id, list_id)
);

-- One list's readers, newest first, which is what the panel asks for. The
-- primary key answers the other direction.
create index on_a_list_recent on on_a_list (list_id, created_at desc, reader_id desc);
