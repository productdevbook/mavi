-- A post had no way to say how it should appear elsewhere.
--
-- The title a search engine shows, the sentence under it, the picture a chat
-- app puts beside a link, and the address that counts as the original when the
-- same writing is in two places. A theme can be made to guess at the first two
-- from the title and the excerpt — which is what a site with nothing else does
-- — but a site that wants to say something different had nowhere to say it.

alter table posts
    add column cover_media_id uuid references media (id) on delete set null,
    add column seo_title text check (seo_title is null or length(seo_title) <= 200),
    add column seo_description text
        check (seo_description is null or length(seo_description) <= 400),
    add column canonical text check (canonical is null or length(canonical) <= 500);

comment on column posts.canonical is
    'Where this was published first, when it was published somewhere else
     first. Empty for everything a site wrote itself.';

create index posts_cover_idx on posts (cover_media_id) where cover_media_id is not null;
