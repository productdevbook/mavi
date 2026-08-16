-- A report was a paragraph, and everything that makes one actionable was lost.
--
-- Which kind of thing it is decides who looks at it and when: something broken
-- is not the same as something missing. What browser it happened in and what
-- the panel had already logged is the half of the story the person reporting
-- it cannot see. And a picture is the single most useful thing anybody sends —
-- which is why it is a file this site already holds rather than a data URI in
-- a text column.

create type report_kind as enum ('broken', 'missing', 'wanted');

alter table reports
    add column kind report_kind not null default 'broken',
    add column environment jsonb not null default '{}'::jsonb
        check (jsonb_typeof(environment) = 'object'),
    add column media_id uuid references media (id) on delete set null;

comment on column reports.environment is
    'What was gathered rather than asked for: the browser, the size of the
     window, and whatever had already gone wrong in the panel.';

create index reports_media_idx on reports (media_id) where media_id is not null;
