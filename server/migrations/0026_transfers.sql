-- Moving a site, as something that survives the machine being killed.
--
-- A site being seeded from a bundle, made into one, or given one somebody
-- carried in. Spawning that off a request means a pod replaced halfway through
-- leaves a site half-written and nobody able to say so.
create type transfer_kind as enum ('seed', 'import', 'export');

create type transfer_state as enum (
    'waiting', 'working', 'done', 'failed', 'cancelled'
);

create table transfers (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    kind        transfer_kind not null,
    state       transfer_state not null default 'waiting',
    -- What it is doing now, in a word a screen can show.
    stage       text not null default 'waiting',
    -- What is being written in, or what was made. A bundle is what portable
    -- reads and writes, so a transfer holds one either way.
    bundle      jsonb,
    -- What it did, once it has done it.
    outcome     jsonb,
    -- Said as well as the sentence: the sentence changes when somebody
    -- rewrites it, and a dashboard keys on the word.
    error_code  text,
    error       text,
    -- Asked to stop. Looked at between stages rather than acted on at once: a
    -- transfer stopped mid-write is worse than one that finishes.
    cancelling  boolean not null default false,
    started_by  uuid references users (id) on delete set null,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    finished_at timestamptz
);

create trigger transfers_touch before update on transfers
    for each row execute function touch_updated_at();

create index transfers_tenant_idx on transfers (tenant_id, created_at desc);
create index transfers_started_by_idx on transfers (started_by);
create index transfers_unfinished_idx on transfers (state) where state in ('waiting', 'working');

alter table transfers enable row level security;
alter table transfers force row level security;

-- A site sees its own. The machine's own screens see every site's, because
-- "did that site arrive" is a question about the machine rather than about one
-- site — the same escape the queue uses, and named in the policy rather than
-- left to whoever opened the connection.
create policy tenant_isolation on transfers
    using (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    )
    with check (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    );
