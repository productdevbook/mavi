-- A relocation fence is separate from the lifecycle state. It blocks request
-- writes while allowing reads and keeps a token so a stale worker cannot
-- release a newer operation's fence.
create table site_write_fences (
    site_id     uuid primary key references site_catalog(site_id) on delete restrict,
    fence_token uuid not null,
    reason      text not null check (length(reason) between 1 and 120),
    fenced_at   timestamptz not null default now()
);

create index site_write_fences_fenced_at on site_write_fences (fenced_at);
