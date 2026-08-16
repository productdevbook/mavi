-- What somebody is learning.
--
-- A course is modules in an order, a module is lessons in an order, and the
-- order is what a teacher spends their afternoon on.

create table courses (
    id         uuid primary key,
    slug       text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,126}[a-z0-9])?$'),
    title      text not null check (length(title) between 1 and 300),
    about      text,
    state      text not null default 'draft' check (state in ('draft', 'open', 'closed')),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

create unique index courses_address on courses (slug) where deleted_at is null;

create index courses_recent
    on courses (created_at desc, id desc)
    where deleted_at is null;

create table modules (
    id         uuid primary key,
    course_id  uuid not null references courses (id) on delete cascade,
    title      text not null check (length(title) between 1 and 300),
    -- Where it sits. A number rather than a linked list, because reordering is
    -- what a person does once in a while and reading in order is what
    -- everything else does all the time.
    place      integer not null check (place >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),

    -- Deferred on purpose. Swapping two modules means writing one into a place
    -- the other is still in; checked per row that is refused half way through,
    -- so every reorder becomes a dance of temporary numbers that a crash
    -- leaves half done. Checked at commit, a reorder is one statement saying
    -- what the new order is — and a duplicate is still refused, at the moment
    -- the answer is actually known.
    constraint one_module_to_a_place unique (course_id, place) deferrable initially deferred
);

create index modules_in_order on modules (course_id, place);

create table lessons (
    id         uuid primary key,
    module_id  uuid not null references modules (id) on delete cascade,
    title      text not null check (length(title) between 1 and 300),
    body       text not null default '',
    place      integer not null check (place >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),

    constraint one_lesson_to_a_place unique (module_id, place) deferrable initially deferred
);

create index lessons_in_order on lessons (module_id, place);

-- Somebody learning here. Not an account: they sign in at the site's own
-- front, hold no grants, and reach nothing in the panel.
create table students (
    id         uuid primary key,
    email      text not null check (email = lower(email) and position('@' in email) > 1),
    name       text not null check (length(name) between 1 and 200),
    -- Null until they take up the invitation and choose one.
    password   text,
    standing   text not null default 'asked' check (standing in ('asked', 'learning', 'stopped')),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

create unique index students_address on students (email) where deleted_at is null;

create index students_recent
    on students (created_at desc, id desc)
    where deleted_at is null;

create table enrolments (
    id         uuid primary key,
    student_id uuid not null references students (id) on delete cascade,
    course_id  uuid not null references courses (id) on delete cascade,
    started_at timestamptz not null default now(),
    finished_at timestamptz,
    created_at timestamptz not null default now(),

    -- Putting somebody on a course twice is putting them on it once, which is
    -- what pressing the button twice means.
    unique (student_id, course_id)
);

create index enrolments_of_a_course on enrolments (course_id, created_at desc, id desc);
create index enrolments_of_a_student on enrolments (student_id);

-- What somebody has worked through. The pair is the key, so saying a lesson is
-- done twice says it once.
create table done (
    student_id uuid not null references students (id) on delete cascade,
    lesson_id  uuid not null references lessons (id) on delete cascade,
    at         timestamptz not null default now(),

    primary key (student_id, lesson_id)
);

create index done_of_a_lesson on done (lesson_id);
