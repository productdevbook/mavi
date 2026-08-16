-- The project a site's pages are built from. Only what is under src/ and
-- public/ can be written, and what decides how it is built is refused on
-- purpose — a build config a customer can edit is a build this machine runs.
create table theme_files (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    -- The branch this version of the file belongs to. `live` is what is
    -- served; anything else is somebody working.
    branch     text not null default 'live' check (branch ~ '^[a-z0-9][a-z0-9-]{0,60}$'),
    path       text not null check (path ~ '^(src|public)/'),
    body       text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,
    unique (tenant_id, branch, path)
);

create trigger theme_files_touch before update on theme_files
    for each row execute function touch_updated_at();

create index theme_files_branch_idx on theme_files (tenant_id, branch)
    where deleted_at is null;

create type publish_state as enum ('queued', 'building', 'live', 'failed');

create table publishes (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    branch      text not null,
    state       publish_state not null default 'queued',
    -- What it cost, for the bill and for the question "why is this slow".
    seconds     integer check (seconds >= 0),
    log         text,
    started_at  timestamptz,
    finished_at timestamptz,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger publishes_touch before update on publishes
    for each row execute function touch_updated_at();

create index publishes_tenant_idx on publishes (tenant_id, created_at desc);
-- One at a time per site: the claim reads this.
create unique index publishes_one_at_a_time on publishes (tenant_id)
    where state in ('queued', 'building');

alter table theme_files enable row level security;
alter table publishes   enable row level security;
alter table theme_files force row level security;
alter table publishes   force row level security;

create policy tenant_isolation on theme_files
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on publishes
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
