create table design_changes (
    site_id           uuid not null references site_catalog(site_id),
    id                uuid not null,
    name              text not null check (char_length(btrim(name)) between 1 and 120),
    state             text not null default 'writing'
        check (state in ('writing', 'building', 'ready', 'failed', 'published')),
    last_error        text,
    ready_build_id    uuid,
    published_build_id uuid,
    published_at      timestamptz,
    created_at        timestamptz not null default now(),
    updated_at        timestamptz not null default now(),
    primary key (site_id, id)
);

create index design_changes_site_recent
    on design_changes (site_id, created_at desc, id desc);

create unique index design_changes_one_published
    on design_changes (site_id)
    where state = 'published';

create table design_files (
    site_id    uuid not null,
    change_id  uuid not null,
    path       text not null check (char_length(path) between 1 and 200),
    contents   bytea not null,
    bytes      bigint not null check (bytes > 0),
    sha256     text not null check (sha256 ~ '^[0-9a-f]{64}$'),
    removed    boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (site_id, change_id, path),
    foreign key (site_id, change_id) references design_changes(site_id, id)
        on delete cascade
);

create index design_files_site_change_path
    on design_files (site_id, change_id, path);

create table design_builds (
    site_id     uuid not null,
    id          uuid not null,
    change_id   uuid not null,
    state       text not null default 'queued'
        check (state in ('queued', 'ready', 'failed')),
    error       text,
    created_at  timestamptz not null default now(),
    completed_at timestamptz,
    primary key (site_id, id),
    foreign key (site_id, change_id) references design_changes(site_id, id)
        on delete cascade
);

create index design_builds_site_change_recent
    on design_builds (site_id, change_id, created_at desc, id desc);

create table design_build_artifacts (
    site_id     uuid not null,
    build_id    uuid not null,
    path        text not null check (char_length(path) between 1 and 200),
    storage_key text not null check (char_length(storage_key) between 1 and 512),
    mime        text not null check (mime ~ '^[a-z]+/[a-z0-9.+-]+$'),
    bytes       bigint not null check (bytes > 0),
    sha256      text not null check (sha256 ~ '^[0-9a-f]{64}$'),
    primary key (site_id, build_id, path),
    foreign key (site_id, build_id) references design_builds(site_id, id)
        on delete cascade
);

create index design_build_artifacts_lookup
    on design_build_artifacts (site_id, build_id, path);

alter table design_changes
    add constraint design_changes_ready_build_fk
    foreign key (site_id, ready_build_id) references design_builds(site_id, id);

alter table design_changes
    add constraint design_changes_published_build_fk
    foreign key (site_id, published_build_id) references design_builds(site_id, id);

do $$
declare
    table_name text;
begin
    foreach table_name in array array['design_changes', 'design_files', 'design_builds', 'design_build_artifacts']
    loop
        execute format('alter table %I enable row level security', table_name);
        execute format('alter table %I force row level security', table_name);
        execute format(
            'create policy %I_scope on %I using (site_id = current_setting(''app.site_id'', true)::uuid) with check (site_id = current_setting(''app.site_id'', true)::uuid)',
            table_name,
            table_name
        );
    end loop;
end $$;
