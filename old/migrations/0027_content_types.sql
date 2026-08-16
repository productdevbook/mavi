-- Which of a site's own kinds of thing a post is.
--
-- By name rather than by id: what a bundle carries, what a theme's template
-- says, and what somebody types into a filter is the name, and one that has to
-- be looked up before it can be used is one every caller has to look up.
alter table posts add column type_key text;

alter table posts add constraint posts_type_is_one_the_site_declared
    foreign key (tenant_id, type_key) references content_types (tenant_id, key)
    -- Only the name is forgotten. Setting both columns would mean setting
    -- `tenant_id` to null, which is the one thing every row must have.
    on delete set null (type_key);

create index posts_type_key_idx on posts (tenant_id, type_key) where deleted_at is null;
