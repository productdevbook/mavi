create table analytics_events (
    site_id      uuid not null references site_catalog(site_id),
    id           uuid not null,
    event_name   text not null check (char_length(btrim(event_name)) between 1 and 120),
    path         text not null check (char_length(btrim(path)) between 1 and 500),
    value        bigint not null default 0 check (value >= 0),
    occurred_at  timestamptz not null default now(),
    created_at   timestamptz not null default now(),
    primary key (site_id, id)
);

create index analytics_events_site_recent
    on analytics_events (site_id, occurred_at desc, id desc);

create index analytics_events_site_filter
    on analytics_events (site_id, event_name, path, occurred_at desc, id desc);

create table analytics_daily (
    site_id      uuid not null references site_catalog(site_id),
    day          date not null,
    event_name   text not null check (char_length(btrim(event_name)) between 1 and 120),
    path         text not null check (char_length(btrim(path)) between 1 and 500),
    event_count  bigint not null check (event_count >= 0),
    value_sum    bigint not null check (value_sum >= 0),
    value_min    bigint not null check (value_min >= 0),
    value_max    bigint not null check (value_max >= 0),
    primary key (site_id, day, event_name, path)
);

create index analytics_daily_site_recent
    on analytics_daily (site_id, day desc, event_name asc, path asc);

create index analytics_daily_site_filter
    on analytics_daily (site_id, event_name, path, day desc);

do $$
declare
    table_name text;
begin
    foreach table_name in array array['analytics_events', 'analytics_daily']
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
