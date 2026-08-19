create table feedback_reports (
    site_id       uuid not null references site_catalog(site_id) on delete cascade,
    id            uuid not null,
    reporter_kind text not null check (reporter_kind in ('account', 'assistant')),
    reporter_id   text not null check (char_length(reporter_id) between 1 and 120),
    kind          text not null check (kind in ('broken', 'missing', 'wanted')),
    title         text not null check (char_length(btrim(title)) between 1 and 300),
    body          text not null default '' check (char_length(body) <= 20000),
    context       jsonb not null default '{}'::jsonb check (jsonb_typeof(context) = 'object'),
    state         text not null default 'open' check (state in ('open', 'closed')),
    answer        text check (answer is null or char_length(answer) <= 20000),
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    primary key (site_id, id)
);

create index feedback_reports_site_recent
    on feedback_reports (site_id, created_at desc, id desc);

create index feedback_reports_site_reporter
    on feedback_reports (site_id, reporter_kind, reporter_id, created_at desc, id desc);

do $$
begin
    alter table feedback_reports enable row level security;
    alter table feedback_reports force row level security;
    create policy feedback_reports_scope on feedback_reports
        using (site_id = current_setting('app.site_id', true)::uuid)
        with check (site_id = current_setting('app.site_id', true)::uuid);
end $$;
