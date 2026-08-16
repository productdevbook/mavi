-- A video is not a picture with a longer name.
--
-- It is uploaded once and then worked on for minutes by something that is not
-- this process, it has a state somebody has to be able to read while that
-- happens, and what plays in the end is not the file that went in. A row in
-- the media library cannot say any of that.
create type video_state as enum ('waiting', 'working', 'ready', 'failed');

create table videos (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    -- What it was made from. Kept even after it has been transcoded, because
    -- "make it again" is a thing that happens and the source is what it needs.
    media_id    uuid references media (id) on delete set null,
    title       text not null check (length(title) between 1 and 300),
    state       video_state not null default 'waiting',
    -- What the transcoder calls this piece of work, so that what comes back
    -- later can be matched to what went out.
    reference   text,
    seconds     integer check (seconds is null or seconds >= 0),
    -- Where it plays, and in what sizes. The shape of this belongs to whoever
    -- transcoded it; nothing here reads into it.
    plays       jsonb not null default '{}'::jsonb check (jsonb_typeof(plays) = 'object'),
    -- Why it did not work, for the one who has to fix it.
    note        text,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz
);

create trigger videos_touch before update on videos
    for each row execute function touch_updated_at();

create index videos_tenant_idx on videos (tenant_id, created_at desc) where deleted_at is null;
create index videos_media_idx on videos (media_id);
create unique index videos_reference_idx on videos (tenant_id, reference)
    where reference is not null;

alter table videos enable row level security;
alter table videos force row level security;

create policy tenant_isolation on videos
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
