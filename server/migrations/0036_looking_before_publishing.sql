-- A design could be published or not published, and nothing in between.
--
-- Somebody has to be able to look at what a change did before it is on the
-- site everybody sees. A preview is a build like any other and is billed like
-- any other; what it does not do is put what it made live.

alter type publish_state add value if not exists 'previewed';

alter table publishes
    add column preview boolean not null default false;

comment on column publishes.preview is
    'A build to look at rather than to serve. What it made is reachable under
     /_preview/{id}/ on the site''s own address and nowhere else.';

create index publishes_preview_idx on publishes (tenant_id, created_at desc)
    where preview;
