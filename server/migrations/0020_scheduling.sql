-- When a scheduled post should stop being scheduled. Separate from
-- `published_at`, which is when it actually happened: a post scheduled for
-- Tuesday and published on Wednesday because nothing ran is a thing somebody
-- has to be able to see.
alter table posts add column publish_at timestamptz;

create index posts_due_idx on posts (publish_at)
    where state = 'scheduled' and deleted_at is null;

-- Scheduled means there is a moment it is waiting for.
alter table posts add constraint posts_scheduled_has_a_moment
    check (state <> 'scheduled' or publish_at is not null);
