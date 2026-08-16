-- Which kind of thing a post is has been its name since 0027, and the `restrict`
-- on this one would have refused to let a site take a kind away — for a column
-- that has never held anything.
alter table posts drop column content_type_id;

-- What a page says about itself is columns of its own since 0041. This was
-- where it would have gone.
alter table posts drop column seo;

-- Nothing measures an image on the way in, so these were always null and a
-- panel reading them would have shown nothing rather than a size.
alter table media drop column width;
alter table media drop column height;
