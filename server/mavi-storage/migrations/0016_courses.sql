create table courses (
    site_id     uuid not null references site_catalog(site_id),
    id          uuid not null,
    slug        text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,158}[a-z0-9])?$'),
    title       text not null check (char_length(btrim(title)) between 1 and 300),
    about       text check (about is null or char_length(about) <= 10000),
    state       text not null default 'draft' check (state in ('draft', 'open', 'closed')),
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz,
    primary key (site_id, id)
);

create unique index courses_site_slug_active
    on courses (site_id, slug)
    where deleted_at is null;

create index courses_site_recent
    on courses (site_id, created_at desc, id desc)
    where deleted_at is null;

create table course_modules (
    site_id     uuid not null references site_catalog(site_id),
    id          uuid not null,
    course_id   uuid not null,
    title       text not null check (char_length(btrim(title)) between 1 and 300),
    position    integer not null check (position >= 0),
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id, course_id) references courses(site_id, id) on delete cascade,
    constraint course_modules_site_position unique (site_id, course_id, position)
        deferrable initially deferred
);

create index course_modules_site_order
    on course_modules (site_id, course_id, position, id);

create table course_lessons (
    site_id       uuid not null references site_catalog(site_id),
    id            uuid not null,
    module_id     uuid not null,
    title         text not null check (char_length(btrim(title)) between 1 and 300),
    body          text not null default '' check (char_length(body) <= 100000),
    media_file_id uuid,
    position      integer not null check (position >= 0),
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id, module_id) references course_modules(site_id, id) on delete cascade,
    foreign key (site_id, media_file_id) references media_files(site_id, id),
    constraint course_lessons_site_position unique (site_id, module_id, position)
        deferrable initially deferred
);

create index course_lessons_site_order
    on course_lessons (site_id, module_id, position, id);

create table course_students (
    site_id                 uuid not null references site_catalog(site_id),
    id                      uuid not null,
    email                   text not null check (email = lower(email) and position('@' in email) > 1),
    name                    text not null check (char_length(btrim(name)) between 1 and 200),
    password_hash           text,
    standing                text not null default 'asked'
                            check (standing in ('asked', 'learning', 'stopped')),
    activation_token_hash   bytea,
    activation_expires_at   timestamptz,
    created_at              timestamptz not null default now(),
    updated_at              timestamptz not null default now(),
    deleted_at              timestamptz,
    primary key (site_id, id),
    constraint course_students_activation check (
        (standing = 'asked' and password_hash is null)
        or
        (standing in ('learning', 'stopped') and password_hash is not null)
    )
);

create unique index course_students_site_email_active
    on course_students (site_id, email)
    where deleted_at is null;

create index course_students_site_recent
    on course_students (site_id, created_at desc, id desc)
    where deleted_at is null;

create table course_student_sessions (
    site_id     uuid not null references site_catalog(site_id),
    id          uuid not null,
    student_id  uuid not null,
    token_hash  bytea not null,
    expires_at  timestamptz not null,
    revoked_at  timestamptz,
    created_at  timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id, student_id) references course_students(site_id, id) on delete cascade,
    unique (site_id, token_hash)
);

create index course_student_sessions_site_active
    on course_student_sessions (site_id, token_hash, expires_at)
    where revoked_at is null;

create table course_enrollments (
    site_id     uuid not null references site_catalog(site_id),
    id          uuid not null,
    course_id   uuid not null,
    student_id  uuid not null,
    started_at  timestamptz not null default now(),
    finished_at timestamptz,
    created_at  timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id, course_id) references courses(site_id, id) on delete cascade,
    foreign key (site_id, student_id) references course_students(site_id, id) on delete cascade,
    unique (site_id, student_id, course_id)
);

create index course_enrollments_site_course_recent
    on course_enrollments (site_id, course_id, created_at desc, id desc);

create index course_enrollments_site_student_recent
    on course_enrollments (site_id, student_id, created_at desc, id desc);

create table course_progress (
    site_id      uuid not null references site_catalog(site_id),
    student_id   uuid not null,
    lesson_id    uuid not null,
    completed_at timestamptz not null default now(),
    primary key (site_id, student_id, lesson_id),
    foreign key (site_id, student_id) references course_students(site_id, id) on delete cascade,
    foreign key (site_id, lesson_id) references course_lessons(site_id, id) on delete cascade
);

create index course_progress_site_student_recent
    on course_progress (site_id, student_id, completed_at desc, lesson_id desc);

do $$
declare
    table_name text;
begin
    foreach table_name in array array[
        'courses',
        'course_modules',
        'course_lessons',
        'course_students',
        'course_student_sessions',
        'course_enrollments',
        'course_progress'
    ]
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
