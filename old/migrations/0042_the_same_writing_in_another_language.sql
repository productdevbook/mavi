-- The same piece of writing in two languages was two unrelated posts.
--
-- A site writing in English and Turkish had no way to say that these two are
-- the same page: a reader on one could not be offered the other, and an editor
-- had to remember which of forty drafts was the pair of which.
--
-- One of them is the original and the rest point at it. Which one is the
-- original carries no meaning beyond being written first — what matters is
-- that they are one group.

alter table posts
    add column translation_of uuid references posts (id) on delete set null;

create index posts_translation_idx on posts (translation_of)
    where translation_of is not null;

-- One post per language in a group: two Turkish translations of one page is a
-- reader being offered a choice nobody meant to give them.
create unique index posts_one_per_language
    on posts (tenant_id, coalesce(translation_of, id), language)
    where deleted_at is null;
