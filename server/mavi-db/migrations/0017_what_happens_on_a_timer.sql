-- What happens on its own, and when it is next due.
--
-- One row per kind of work that runs on a timer. `next_at` is both the
-- question and the claim: the statement that asks "is it due" is the statement
-- that moves it forward, so two processes asking at the same moment is one of
-- them getting it and the other finding a time in the future.
--
-- How often is not here. It is written in the code, because it is a fact about
-- what the code does — this column is what that fact was the last time a
-- process started, kept so the claim can be one statement rather than two.

create table schedules (
    kind          text primary key,
    every_seconds bigint not null check (every_seconds > 0),
    -- When it may next be taken. `now()` for a schedule that has just been
    -- written down, so a new installation does its first sweep rather than
    -- waiting an interval for one.
    next_at       timestamptz not null default now(),
    -- When it was last taken. Not used to decide anything: it is what somebody
    -- reads when they are asking why something has not happened.
    last_at       timestamptz,
    created_at    timestamptz not null default now()
);
