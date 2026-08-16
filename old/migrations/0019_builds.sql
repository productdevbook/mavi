alter table publishes add column cancelled_at timestamptz;

-- What was built, so that a site can be put back the way it was without
-- keeping every version of every file forever.
alter table publishes add column files integer check (files >= 0);

-- Cancelled is a fourth thing a publish can be.
alter type publish_state add value if not exists 'cancelled';
