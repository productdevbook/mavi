create table jobs (
    site_id          uuid not null references site_catalog(site_id),
    id               uuid not null,
    kind             text not null check (char_length(kind) between 1 and 120),
    payload          jsonb not null default '{}'::jsonb
                     check (jsonb_typeof(payload) = 'object'),
    state            text not null default 'ready'
                     check (state in ('ready', 'running', 'done', 'dead')),
    run_at           timestamptz not null default now(),
    claimed_until    timestamptz,
    claimed_by       text,
    attempts         integer not null default 0 check (attempts >= 0),
    last_error       text,
    idempotency_key  text check (idempotency_key is null or char_length(idempotency_key) between 1 and 160),
    created_at       timestamptz not null default now(),
    finished_at      timestamptz,
    primary key (site_id, id),
    constraint jobs_running_has_lease check (
        (state = 'running') = (claimed_until is not null and claimed_by is not null)
    ),
    constraint jobs_finished_have_timestamp check (
        (state in ('done', 'dead')) = (finished_at is not null)
    )
);

create unique index jobs_site_kind_idempotency
    on jobs (site_id, kind, idempotency_key)
    where idempotency_key is not null;

create index jobs_site_ready
    on jobs (site_id, run_at asc, id asc)
    where state in ('ready', 'running');

create index jobs_site_kind_state
    on jobs (site_id, kind, state, run_at asc, id asc);

create index jobs_site_recent
    on jobs (site_id, created_at desc, id desc);

do $$
declare
    table_name text;
begin
    foreach table_name in array array['jobs']
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
