alter table media_files
    add column visibility text not null default 'private'
        check (visibility in ('private', 'public'));

create index media_files_site_visibility_recent
    on media_files (site_id, visibility, created_at desc, id desc)
    where deleted_at is null;
