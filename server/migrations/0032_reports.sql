-- Somebody saying a screen is broken, and the answer coming back.
--
-- Without this a customer's only route is the phone, and what they said is in
-- somebody's messages rather than beside the site it was about.
create type report_state as enum ('said', 'seen', 'answered', 'closed');

create table reports (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    -- Who said it. Null where the account has since gone, which does not make
    -- what they said less true.
    said_by    uuid references users (id) on delete set null,
    -- Whether a person said it or an assistant did. An assistant reporting the
    -- same gap in forty sites is one thing to fix, not forty.
    by_a_person boolean not null default true,
    -- What they were looking at. Kept as they typed it, and it is a path on
    -- their own site rather than anything this machine chooses.
    screen     text,
    body       text not null check (length(body) between 1 and 10000),
    state      report_state not null default 'said',
    -- What came back, and when. Written by whoever runs the machine.
    answer     text,
    answered_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger reports_touch before update on reports
    for each row execute function touch_updated_at();

create index reports_tenant_idx on reports (tenant_id, created_at desc);
create index reports_said_by_idx on reports (said_by);
create index reports_open_idx on reports (state) where state <> 'closed';

alter table reports enable row level security;
alter table reports force row level security;

-- A site sees its own; the machine's own screens see every site's, because a
-- gap reported by three sites is one gap.
create policy tenant_isolation on reports
    using (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    )
    with check (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    );
