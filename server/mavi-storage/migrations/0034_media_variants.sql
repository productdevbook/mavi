create table media_variants (
    site_id         uuid not null,
    id              uuid not null,
    source_file_id  uuid not null,
    preset          text not null check (preset in ('thumbnail', 'medium', 'large')),
    mime            text not null check (mime = 'image/jpeg'),
    storage_key     text not null check (storage_key ~ '^[0-9a-f]{2}/[0-9a-f]{30}\.jpg$'),
    width           integer not null check (width > 0),
    height          integer not null check (height > 0),
    bytes           bigint not null check (bytes > 0),
    sha256          text not null check (sha256 ~ '^[0-9a-f]{64}$'),
    created_at      timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id) references site_catalog(site_id),
    foreign key (site_id, source_file_id) references media_files(site_id, id),
    unique (site_id, source_file_id, preset),
    unique (site_id, storage_key)
);

create index media_variants_source
    on media_variants (site_id, source_file_id, created_at asc, id asc);

alter table media_variants enable row level security;
alter table media_variants force row level security;
create policy media_variants_scope on media_variants
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);

alter table media_cleanup_tasks
    add column storage_keys text[] not null default '{}';
