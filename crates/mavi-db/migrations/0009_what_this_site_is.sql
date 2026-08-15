-- What this site is.
--
-- One installation is one site, and this is where that stops being a sentence
-- in a README. The settings are one row — not one row by convention, one row
-- because a second cannot be inserted.

create table settings (
    -- Always true, unique. The whole of the single-row rule: `insert` a second
    -- one and the unique index refuses it, whoever is inserting and whatever
    -- they think they are doing.
    only_one   boolean primary key default true check (only_one),
    name       text not null check (length(name) between 1 and 200),
    about      text,
    -- When "tomorrow at nine" is, and what day a report covers. Stored rather
    -- than taken from the machine: a machine is moved and a site is not.
    time_zone  text not null default 'UTC' check (time_zone ~ '^[A-Za-z0-9_+-]+(/[A-Za-z0-9_+-]+)*$'),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

-- What the site writes in.
create table languages (
    tag        text primary key check (tag ~ '^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$'),
    -- In itself: `Türkçe` rather than `Turkish`. Whoever is choosing reads
    -- that one.
    name       text not null check (length(name) between 1 and 100),
    is_the_sites_own boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

-- At most one is the site's own. The other half — that there is at least one,
-- and at least one language at all — is not something a constraint on a table
-- can see, and is refused where a language is taken away.
create unique index languages_one_is_the_sites_own
    on languages ((true))
    where is_the_sites_own;
