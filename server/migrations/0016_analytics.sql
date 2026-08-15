-- What a site was asked for, by the day and the path. Counted rather than
-- logged: a row per visit is a table nobody can afford, and nothing here needs
-- to know which visit was which.
create table page_views (
    tenant_id  uuid not null references tenants (id) on delete cascade,
    on_day     date not null,
    path       text not null check (length(path) <= 500),
    views      bigint not null default 0 check (views >= 0),
    -- Roughly how many people, from a count of what the day's salt hashed
    -- their address to. The salt is thrown away with the day, so nothing here
    -- can be turned back into who somebody was.
    visitors   bigint not null default 0 check (visitors >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (tenant_id, on_day, path)
);

create trigger page_views_touch before update on page_views
    for each row execute function touch_updated_at();

create index page_views_day_idx on page_views (tenant_id, on_day desc);

-- One row per address per day, and the address is not in it: only what today's
-- salt hashed it to. Swept with the day.
create table visitor_marks (
    tenant_id  uuid not null references tenants (id) on delete cascade,
    on_day     date not null,
    mark       bytea not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (tenant_id, on_day, mark)
);

create trigger visitor_marks_touch before update on visitor_marks
    for each row execute function touch_updated_at();

create type vital_kind as enum ('lcp', 'inp', 'cls', 'ttfb');

create table vitals (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    on_day     date not null,
    path       text not null check (length(path) <= 500),
    kind       vital_kind not null,
    -- Milliseconds, or a hundredth for the ones that are a ratio. An integer,
    -- because a measurement is not money and not a float either.
    value      integer not null check (value >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger vitals_touch before update on vitals
    for each row execute function touch_updated_at();

create index vitals_day_idx on vitals (tenant_id, on_day desc, kind);

alter table page_views    enable row level security;
alter table visitor_marks enable row level security;
alter table vitals        enable row level security;
alter table page_views    force row level security;
alter table visitor_marks force row level security;
alter table vitals        force row level security;

create policy tenant_isolation on page_views
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on visitor_marks
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on vitals
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
