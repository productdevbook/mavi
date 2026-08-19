-- A course instructor assignment is a resource-scoped Cedar grant. It is
-- deliberately separate from site-wide role grants: removing an instructor
-- from one course must not change their permissions anywhere else.
create table course_instructors (
    site_id    uuid not null references site_catalog(site_id),
    course_id  uuid not null,
    person_id  uuid not null,
    grants     text[] not null default array['view']::text[],
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (site_id, course_id, person_id),
    foreign key (site_id, course_id) references courses(site_id, id) on delete cascade,
    foreign key (site_id, person_id) references people(site_id, id) on delete cascade,
    constraint course_instructors_grants_valid check (
        cardinality(grants) between 1 and 3
        and grants <@ array['view', 'write', 'delete']::text[]
    )
);

create index course_instructors_site_course_created
    on course_instructors (site_id, course_id, created_at asc, person_id asc);

do $$
begin
    alter table course_instructors enable row level security;
    alter table course_instructors force row level security;
    create policy course_instructors_scope on course_instructors
        using (site_id = current_setting('app.site_id', true)::uuid)
        with check (site_id = current_setting('app.site_id', true)::uuid);
end $$;
