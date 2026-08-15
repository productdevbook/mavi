-- What somebody uploaded.
--
-- Two columns here say the same thing in two different senses, and keeping
-- them apart is the point of the table:
--
--   `name`     what somebody called it. Shown back to them. Used for nothing.
--   `kept_at`  where the bytes are. Made from the id, never from the name.
--
-- The moment one stands in for the other, a name somebody typed becomes a path
-- on the disk, and a name can be `../../etc/passwd`.
--
-- `mime` is what the bytes turned out to be, decided by reading them. It is
-- not what the upload claimed: a `holiday.png` full of script is not an image,
-- and a site that serves it as one is serving somebody else's script from its
-- own address.

create table files (
    id         uuid primary key,
    kind       text not null check (kind in ('image', 'video', 'audio', 'document')),
    mime       text not null check (mime ~ '^[a-z]+/[a-z0-9.+-]+$'),
    name       text not null check (length(name) between 1 and 255),
    -- Made from the id and the extension the bytes earned. The check is what
    -- keeps that true from the database's side, where nothing can forget it:
    -- two hex characters, a slash, the rest of the id, a dot, an extension.
    kept_at    text not null check (kept_at ~ '^[0-9a-f]{2}/[0-9a-f]{30}\.[a-z0-9]{2,5}$'),
    bytes      bigint not null check (bytes > 0),
    created_at timestamptz not null default now(),
    deleted_at timestamptz,

    -- Nothing is ever kept twice in one place. Not partial, unlike the
    -- writings' address: a deleted row still names bytes on the disk until
    -- something goes and removes them, and reusing the path in the meantime
    -- hands the old file's bytes to whoever asks for the new one.
    constraint files_are_kept_somewhere_of_their_own unique (kept_at)
);

-- What the panel lists, and the keyset it is ordered by, column for column.
create index files_recent
    on files (created_at desc, id desc)
    where deleted_at is null;

-- The same list narrowed to one sort of thing, which is how the picker opens:
-- an editor choosing a picture is not shown every PDF the site holds.
create index files_recent_of_a_kind
    on files (kind, created_at desc, id desc)
    where deleted_at is null;
