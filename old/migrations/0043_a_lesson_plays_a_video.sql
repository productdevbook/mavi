-- A lesson could hold words and not the video it is about.
--
-- Videos were uploaded, made ready and listed, and nothing tied one to the
-- lesson that plays it: a curriculum built through the API had text and no
-- way to say "watch this".

alter table lessons
    add column video_id uuid references videos (id) on delete set null;

create index lessons_video_idx on lessons (video_id) where video_id is not null;
