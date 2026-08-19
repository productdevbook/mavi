create table media_cleanup_tasks (
    site_id     uuid not null,
    file_id     uuid not null,
    storage_key text not null check (storage_key ~ '^[0-9a-f]{2}/[0-9a-f]{30}\.[a-z0-9]{2,5}$'),
    created_at  timestamptz not null default now(),
    attempts    integer not null default 0 check (attempts >= 0),
    completed_at timestamptz,
    primary key (site_id, file_id),
    foreign key (site_id) references site_catalog(site_id),
    unique (site_id, storage_key)
);

create index media_cleanup_tasks_pending
    on media_cleanup_tasks (site_id, created_at, file_id)
    where completed_at is null;

alter table media_cleanup_tasks enable row level security;
alter table media_cleanup_tasks force row level security;
create policy media_cleanup_tasks_scope on media_cleanup_tasks
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
