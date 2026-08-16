-- Who can sign in, and what they may do.
--
-- One installation is one site, so there is one list of accounts and no
-- question of which site an account belongs to.

-- What a role is allowed to do, as the grants a check reads.
--
-- Grants are text — `content:write`, `shop:view:own` — rather than columns,
-- because a column per capability is a migration every time a site learns to
-- do something new. What the names may be is a list in the code, checked
-- where a grant is read rather than trusted because it is in a row.
create table roles (
    id           uuid primary key,
    name         text not null check (length(name) between 1 and 100),
    grants       text[] not null default '{}',
    -- The role that can do everything, including the things nothing else may.
    -- Exactly one exists.
    is_the_owner boolean not null default false,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now()
);

create unique index roles_name on roles (name);

-- One owner role. That there is always at least one *person* holding it is
-- the half a constraint cannot see, and is refused where somebody is removed.
create unique index roles_one_owner on roles ((true)) where is_the_owner;

create table people (
    id         uuid primary key,
    -- Folded before it is written, so one address is not two accounts.
    email      text not null check (email = lower(email) and position('@' in email) > 1),
    name       text not null check (length(name) between 1 and 200),
    -- Null until they choose one. An account that has been invited and never
    -- taken up has no password rather than an empty one.
    password   text,
    role_id    uuid not null references roles (id) on delete restrict,
    standing   text not null default 'asked'
        check (standing in ('asked', 'here', 'stopped')),
    -- When the address was proved. Proving it is not the same as choosing a
    -- password, and the two are kept apart because conflating them was a way
    -- into somebody else's account.
    proved_at  timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

create unique index people_address on people (email) where deleted_at is null;

create index people_recent
    on people (created_at desc, id desc)
    where deleted_at is null;

create index people_of_a_role on people (role_id) where deleted_at is null;

-- A session is a token somebody holds. What is stored is its hash: a stolen
-- database is not a stolen set of sessions.
create table sessions (
    id         uuid primary key,
    person_id  uuid not null references people (id) on delete cascade,
    token      bytea not null unique,
    -- Set when it is signed out or when something signs every session out —
    -- a password changing, an account being stopped. Kept rather than deleted
    -- so "when did this stop working" has an answer.
    ended_at   timestamptz,
    expires_at timestamptz not null,
    last_seen_at timestamptz,
    created_at timestamptz not null default now(),

    constraint a_session_ends_after_it_starts check (expires_at > created_at)
);

-- What every request asks: is this token one of ours, and is it still good.
create index sessions_live
    on sessions (token)
    where ended_at is null;

create index sessions_of_a_person on sessions (person_id);

-- A link somebody was sent, and the one thing it is good for.
create table tickets (
    id         uuid primary key,
    person_id  uuid not null references people (id) on delete cascade,
    token      bytea not null unique,
    -- What it is for. Redeeming asks for this in the `where`, so a ticket of
    -- the wrong purpose is not found rather than found and then checked.
    what_for   text not null
        check (what_for in ('invitation', 'forgotten_password', 'address_to_prove')),
    -- What the address will be, for a ticket that proves one. Null otherwise:
    -- an invitation does not change anybody's address.
    becomes    text,
    used_at    timestamptz,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create index tickets_live
    on tickets (token)
    where used_at is null;

create index tickets_of_a_person on tickets (person_id);
