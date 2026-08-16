create table media (
    id            uuid primary key default gen_random_uuid(),
    tenant_id     uuid not null references tenants (id) on delete cascade,
    uploaded_by   uuid references users (id) on delete set null,
    -- Where the bytes are, under whatever is storing them. Never what the
    -- person called the file: a name they chose is not a path this machine
    -- opens.
    location      text not null,
    -- What they called it, kept only to give back when it is downloaded.
    original_name text not null check (length(original_name) between 1 and 300),
    -- What the bytes say it is, not what the name claimed.
    mime          text not null,
    bytes         bigint not null check (bytes > 0),
    width         integer check (width > 0),
    height        integer check (height > 0),
    checksum      bytea not null,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    deleted_at    timestamptz,
    unique (tenant_id, location)
);

create trigger media_touch before update on media
    for each row execute function touch_updated_at();

create index media_tenant_idx on media (tenant_id, created_at desc)
    where deleted_at is null;
create index media_uploader_idx on media (uploaded_by);
-- The same file uploaded twice is one set of bytes and two rows, and this is
-- what finds the first.
create index media_checksum_idx on media (tenant_id, checksum);

alter table media enable row level security;
alter table media force row level security;

create policy tenant_isolation on media
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());
