-- A second thing somebody has, besides the password they know.
--
-- One per account: two authenticator apps for one way in is a way to be locked
-- out by whichever was set up second.
create table second_factors (
    person_id  uuid primary key references people (id) on delete cascade,
    -- Sealed, never hashed. The digits have to be computed from this on every
    -- sign-in, so it has to come back out — and kept plainly it would make a
    -- copy of this table a drawer of working authenticators. What seals it is
    -- the host's, through a port.
    sealed     bytea not null,
    -- Null until the six digits have been shown to work once. **An unconfirmed
    -- row stands between nobody and their account** — somebody who scanned a
    -- picture and closed the tab has not locked themselves out.
    confirmed_at timestamptz,
    -- The last step whose code was taken. A code that works twice is a code
    -- somebody read over a shoulder.
    last_step  bigint,
    created_at timestamptz not null default now()
);

-- What gets somebody back in when the phone is gone.
--
-- Hashed like a session token, and one row per code so that using one leaves
-- the others alone. A single column of codes would be a read, a rewrite, and
-- two people using two codes at once leaving one of them still valid.
create table ways_back_in (
    person_id  uuid not null references people (id) on delete cascade,
    code       bytea not null,
    used_at    timestamptz,
    created_at timestamptz not null default now(),

    primary key (person_id, code)
);
