-- A fixed window per caller per thing, counted in the database rather than in
-- one process's memory: two replicas that each keep their own count let twice
-- as much through, and the limits that matter here are on sign-in and on
-- anything a visitor can post to.

create table rate_limits (
    bucket       text not null,
    window_start timestamptz not null,
    count        integer not null default 0 check (count >= 0),
    primary key (bucket, window_start)
);

-- Old windows are swept; nothing reads one once its window has passed.
create index rate_limits_window_idx on rate_limits (window_start);
