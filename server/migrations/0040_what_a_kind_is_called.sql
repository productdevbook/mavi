-- A kind of thing had one name, in whichever language it was made in.
--
-- A site writing in two languages had every screen labelled in one of them: a
-- Turkish editor reading "Book", or an English one reading "Kitap". And every
-- list of them said "Book" above a list of several, because there was nowhere
-- to put the plural.

alter table content_types
    add column plural text check (plural is null or length(plural) between 1 and 100),
    add column names jsonb not null default '{}'::jsonb
        check (jsonb_typeof(names) = 'object');

comment on column content_types.names is
    'What this kind is called in each language: {"tr": {"name": "Kitap",
     "plural": "Kitaplar"}}. What is missing falls back to name and plural,
     which is what a site with one language always uses.';
