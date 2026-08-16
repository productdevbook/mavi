create type course_state as enum ('draft', 'open', 'closed');

create table courses (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    slug        text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$'),
    title       text not null check (length(title) between 1 and 300),
    summary     text,
    state       course_state not null default 'draft',
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz,
    unique (tenant_id, slug)
);

create trigger courses_touch before update on courses
    for each row execute function touch_updated_at();

create index courses_tenant_idx on courses (tenant_id, created_at desc);

create table modules (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    course_id  uuid not null references courses (id) on delete cascade,
    title      text not null check (length(title) between 1 and 300),
    -- Where it sits in the course. An integer rather than a linked list,
    -- because reordering is what a person does and reading in order is what
    -- everything else does.
    position   integer not null check (position >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, course_id, position)
);

create trigger modules_touch before update on modules
    for each row execute function touch_updated_at();

create index modules_course_idx on modules (course_id, position);
create index modules_tenant_idx on modules (tenant_id);

create table lessons (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    module_id  uuid not null references modules (id) on delete cascade,
    title      text not null check (length(title) between 1 and 300),
    body       text not null default '',
    position   integer not null check (position >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, module_id, position)
);

create trigger lessons_touch before update on lessons
    for each row execute function touch_updated_at();

create index lessons_module_idx on lessons (module_id, position);
create index lessons_tenant_idx on lessons (tenant_id);

-- Somebody learning on a site. Not a `users` row: a student signs in at the
-- site's own front, has no grants, and reaches nothing in the panel.
create type student_state as enum ('invited', 'active', 'suspended');

create table students (
    id            uuid primary key default gen_random_uuid(),
    tenant_id     uuid not null references tenants (id) on delete cascade,
    email         text not null check (email = lower(email) and position('@' in email) > 1),
    name          text not null check (length(name) between 1 and 200),
    password_hash text,
    state         student_state not null default 'invited',
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    deleted_at    timestamptz,
    unique (tenant_id, email)
);

create trigger students_touch before update on students
    for each row execute function touch_updated_at();

create index students_tenant_idx on students (tenant_id);

create table student_sessions (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    student_id   uuid not null references students (id) on delete cascade,
    token_hash   bytea not null unique,
    issued_at    timestamptz not null default now(),
    expires_at   timestamptz not null,
    last_seen_at timestamptz,
    revoked_at   timestamptz,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    check (expires_at > issued_at)
);

create trigger student_sessions_touch before update on student_sessions
    for each row execute function touch_updated_at();

create index student_sessions_student_idx on student_sessions (student_id, tenant_id);
create index student_sessions_tenant_idx on student_sessions (tenant_id);
create index student_sessions_expiry_idx on student_sessions (expires_at)
    where revoked_at is null;

create table enrolments (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    student_id uuid not null references students (id) on delete cascade,
    course_id  uuid not null references courses (id) on delete cascade,
    started_at timestamptz not null default now(),
    finished_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, student_id, course_id)
);

create trigger enrolments_touch before update on enrolments
    for each row execute function touch_updated_at();

create index enrolments_course_idx on enrolments (course_id);
create index enrolments_student_idx on enrolments (student_id);
create index enrolments_tenant_idx on enrolments (tenant_id);

create table lesson_progress (
    student_id uuid not null references students (id) on delete cascade,
    lesson_id  uuid not null references lessons (id) on delete cascade,
    tenant_id  uuid not null references tenants (id) on delete cascade,
    done_at    timestamptz not null default now(),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (student_id, lesson_id)
);

create trigger lesson_progress_touch before update on lesson_progress
    for each row execute function touch_updated_at();

create index lesson_progress_lesson_idx on lesson_progress (lesson_id);
create index lesson_progress_tenant_idx on lesson_progress (tenant_id);

alter table courses          enable row level security;
alter table modules          enable row level security;
alter table lessons          enable row level security;
alter table students         enable row level security;
alter table student_sessions enable row level security;
alter table enrolments       enable row level security;
alter table lesson_progress  enable row level security;

alter table courses          force row level security;
alter table modules          force row level security;
alter table lessons          force row level security;
alter table students         force row level security;
alter table student_sessions force row level security;
alter table enrolments       force row level security;
alter table lesson_progress  force row level security;

create policy tenant_isolation on courses
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on modules
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on lessons
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on students
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on student_sessions
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on enrolments
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on lesson_progress
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
