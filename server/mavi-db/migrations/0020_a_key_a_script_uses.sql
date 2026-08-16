-- A key an assistant or a script signs in with.
--
-- A session is for a person at a screen: it expires, because somebody walking
-- away from a machine should stop being signed in. A script has nobody to walk
-- away, so a session is the wrong shape for one — it stops working in the
-- night and nothing says why.
--
-- The description has claimed since it was written that a bearer token is "a
-- key made in the panel". Nothing made one. This is that.
create table keys (
    id         uuid primary key,
    -- Whose it is. What it may do is never more than what they may do, worked
    -- out when it is used rather than copied here — see `store::whoever_holds`.
    person_id  uuid not null references people (id) on delete cascade,
    -- What somebody calls it, so the one to revoke is the one they meant.
    -- "the deploy script" and "my laptop" are the difference between revoking
    -- confidently and revoking everything.
    name       text not null check (length(name) between 1 and 100),
    -- The hash, never the key. A copy of this table is not a drawer of working
    -- keys, which is the same rule sessions and tickets already follow.
    token      bytea not null unique,
    -- What it may do, as a narrowing of what its account may do. Empty means
    -- everything the account can — a key made without thinking about it is a
    -- key that is exactly its account, which is what somebody expects.
    grants     text[] not null default '{}',
    -- Kept rather than deleted, so "when did this stop working" has an answer.
    ended_at   timestamptz,
    -- What tells somebody which key nobody uses any more. Null until it is
    -- used once.
    last_seen_at timestamptz,
    created_at timestamptz not null default now()
);

create index keys_of_a_person on keys (person_id) where ended_at is null;
