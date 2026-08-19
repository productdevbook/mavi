create table forms (
    site_id    uuid not null references site_catalog(site_id),
    id         uuid not null,
    slug       text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,158}[a-z0-9])?$'),
    name       text not null check (char_length(btrim(name)) between 1 and 200),
    fields     jsonb not null default '[]'::jsonb check (jsonb_typeof(fields) = 'array'),
    open       boolean not null default true,
    kept_days  integer not null default 365 check (kept_days between 1 and 3650),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,
    primary key (site_id, id)
);

create unique index forms_site_slug_active
    on forms (site_id, slug)
    where deleted_at is null;

create index forms_site_recent
    on forms (site_id, created_at desc, id desc)
    where deleted_at is null;

create table form_submissions (
    site_id    uuid not null,
    id         uuid not null,
    form_id    uuid not null,
    answers    jsonb not null check (jsonb_typeof(answers) = 'object'),
    seen_at    timestamptz,
    created_at timestamptz not null default now(),
    deleted_at timestamptz,
    primary key (site_id, id),
    foreign key (site_id, form_id) references forms(site_id, id)
        on delete cascade
);

create index form_submissions_site_form_recent
    on form_submissions (site_id, form_id, created_at desc, id desc)
    where deleted_at is null;

create index form_submissions_site_form_unread
    on form_submissions (site_id, form_id, created_at desc, id desc)
    where deleted_at is null and seen_at is null;

do $$
declare
    table_name text;
begin
    foreach table_name in array array['forms', 'form_submissions']
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
