-- How many times the site was read, and how it felt.
--
-- Counted rather than logged. A row per visit is a table nobody can afford,
-- and nothing here needs to know which visit was which.
--
-- **How many times, not how many people.** Telling those apart means knowing
-- where a request came from, and this software is not told that: a request
-- arrives having crossed whatever the host put in front of it, and every
-- arrangement for recovering the original address is a decision about which
-- proxy to believe. A count of people that is wrong — everybody behind one
-- proxy counted as one — is worse than no count of people at all, so there is
-- none, and nothing here has ever held an address to be careful with.

-- What was asked for, by the day and the path.
create table page_views (
    on_day     date not null,
    path       text not null check (length(path) between 1 and 500),
    views      bigint not null default 0 check (views >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),

    primary key (on_day, path)
);

-- The order a screen reads them in: a day at a time, newest first.
create index page_views_by_day on page_views (on_day desc);

-- What a browser measured, by the day and the path.
--
-- Kept as rows rather than averaged on the way in, because what somebody needs
-- from these is the middle and the bad end — and an average hides exactly the
-- readers who had a bad time.
create table vitals (
    id         uuid primary key,
    on_day     date not null,
    path       text not null check (length(path) between 1 and 500),
    -- What was measured. Text with a list rather than a type of its own: a
    -- browser that starts reporting a fifth of these is a migration either
    -- way, and this one does not need the table rewritten.
    kind       text not null check (kind in ('lcp', 'inp', 'cls', 'ttfb')),
    -- Milliseconds, or a hundredth where the measurement is a ratio. A whole
    -- number, because a measurement is not money and not a float either.
    value      integer not null check (value >= 0),
    created_at timestamptz not null default now()
);

create index vitals_by_day on vitals (on_day desc, kind);
