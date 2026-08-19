create table media_files (
    site_id     uuid not null,
    id          uuid not null,
    kind        text not null check (kind in ('image', 'video', 'audio', 'document')),
    mime        text not null check (mime ~ '^[a-z]+/[a-z0-9.+-]+$'),
    name        text not null check (char_length(name) between 1 and 255),
    storage_key text not null check (storage_key ~ '^[0-9a-f]{2}/[0-9a-f]{30}\.[a-z0-9]{2,5}$'),
    bytes       bigint not null check (bytes > 0),
    sha256      text not null check (sha256 ~ '^[0-9a-f]{64}$'),
    created_at  timestamptz not null default now(),
    deleted_at  timestamptz,
    primary key (site_id, id),
    foreign key (site_id) references site_catalog(site_id),
    unique (site_id, storage_key)
);

create index media_files_site_recent
    on media_files (site_id, created_at desc, id desc)
    where deleted_at is null;

create index media_files_site_kind_recent
    on media_files (site_id, kind, created_at desc, id desc)
    where deleted_at is null;

alter table media_files enable row level security;
alter table media_files force row level security;
create policy media_files_scope on media_files
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
