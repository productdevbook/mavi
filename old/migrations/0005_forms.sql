create table forms (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    slug        text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$'),
    name        text not null check (length(name) between 1 and 200),
    -- What the form asks for, as a list of fields. jsonb rather than text, so
    -- that a question about a field is a query rather than a scan.
    fields      jsonb not null default '[]'::jsonb check (jsonb_typeof(fields) = 'array'),
    active      boolean not null default true,
    -- How long what people send is kept. Every table holding somebody's own
    -- words has one of these and a job that enforces it.
    retention_days integer not null default 365 check (retention_days between 1 and 3650),
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz,
    unique (tenant_id, slug)
);

create trigger forms_touch before update on forms
    for each row execute function touch_updated_at();

create index forms_tenant_idx on forms (tenant_id, created_at desc);

create table form_submissions (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    form_id     uuid not null references forms (id) on delete cascade,
    answers     jsonb not null check (jsonb_typeof(answers) = 'object'),
    -- Kept to answer "is this the same person filling it in fifty times", and
    -- swept with the rest of the submission.
    from_ip     inet,
    user_agent  text,
    seen_at     timestamptz,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz
);

create trigger form_submissions_touch before update on form_submissions
    for each row execute function touch_updated_at();

create index form_submissions_form_idx
    on form_submissions (form_id, created_at desc)
    where deleted_at is null;

create index form_submissions_tenant_idx on form_submissions (tenant_id, created_at desc);

alter table forms            enable row level security;
alter table form_submissions enable row level security;
alter table forms            force row level security;
alter table form_submissions force row level security;

create policy tenant_isolation on forms
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());

create policy tenant_isolation on form_submissions
    using (tenant_id = current_tenant_id())
    with check (tenant_id = current_tenant_id());
