-- Access that was sold for a year never ended.
--
-- An enrolment let somebody watch a course for as long as the site existed,
-- so a course sold for ninety days was sold for ever by accident. Null still
-- means for ever — it is what somebody choosing "no end" is asking for — but
-- now it is a choice rather than the only possibility.

alter table enrolments
    add column ends_at timestamptz;

comment on column enrolments.ends_at is
    'When this stops opening the course. Null is for ever.';

create index enrolments_ending_idx on enrolments (tenant_id, ends_at)
    where ends_at is not null;

-- When somebody was last actually here, for the screen that asks whether an
-- account is being used before it is taken away.
alter table students
    add column last_seen_at timestamptz;
