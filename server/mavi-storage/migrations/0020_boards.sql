create table boards (
    site_id      uuid not null references site_catalog(site_id),
    id           uuid not null,
    name         text not null check (char_length(btrim(name)) between 1 and 200),
    description  text check (description is null or char_length(description) <= 10000),
    archived     boolean not null default false,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    deleted_at   timestamptz,
    primary key (site_id, id)
);

create unique index boards_site_name_active
    on boards (site_id, lower(name))
    where deleted_at is null;

create index boards_site_recent
    on boards (site_id, created_at desc, id desc)
    where deleted_at is null;

create table board_lists (
    site_id     uuid not null references site_catalog(site_id),
    id          uuid not null,
    board_id    uuid not null,
    name        text not null check (char_length(btrim(name)) between 1 and 120),
    position    integer not null check (position >= 0),
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    deleted_at  timestamptz,
    primary key (site_id, id),
    foreign key (site_id, board_id) references boards(site_id, id) on delete cascade,
    unique (site_id, id, board_id),
    constraint board_lists_site_position unique (site_id, board_id, position)
        deferrable initially deferred
);

create index board_lists_site_order
    on board_lists (site_id, board_id, position, id)
    where deleted_at is null;

create table board_cards (
    site_id      uuid not null references site_catalog(site_id),
    id           uuid not null,
    board_id     uuid not null,
    list_id      uuid not null,
    title        text not null check (char_length(btrim(title)) between 1 and 300),
    description  text check (description is null or char_length(description) <= 20000),
    assignee_id  uuid,
    position     integer not null check (position >= 0),
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    archived_at  timestamptz,
    primary key (site_id, id),
    foreign key (site_id, board_id) references boards(site_id, id) on delete cascade,
    foreign key (site_id, list_id, board_id) references board_lists(site_id, id, board_id) on delete restrict,
    foreign key (site_id, assignee_id) references people(site_id, id),
    unique (site_id, id, board_id),
    constraint board_cards_site_position unique (site_id, list_id, position)
        deferrable initially deferred
);

create index board_cards_site_list_order
    on board_cards (site_id, list_id, position, id)
    where archived_at is null;

create index board_cards_site_assignee
    on board_cards (site_id, assignee_id, updated_at desc, id desc)
    where archived_at is null and assignee_id is not null;

create table board_comments (
    site_id     uuid not null references site_catalog(site_id),
    id          uuid not null,
    board_id    uuid not null,
    card_id     uuid not null,
    author_id   uuid,
    body        text not null check (char_length(btrim(body)) between 1 and 10000),
    edited_at   timestamptz,
    created_at  timestamptz not null default now(),
    deleted_at  timestamptz,
    primary key (site_id, id),
    foreign key (site_id, board_id) references boards(site_id, id) on delete cascade,
    foreign key (site_id, card_id, board_id) references board_cards(site_id, id, board_id) on delete cascade,
    foreign key (site_id, author_id) references people(site_id, id)
);

create index board_comments_site_card_recent
    on board_comments (site_id, card_id, created_at asc, id asc)
    where deleted_at is null;

create table board_activity (
    site_id     uuid not null references site_catalog(site_id),
    id          uuid not null,
    board_id    uuid not null,
    card_id     uuid,
    kind        text not null check (char_length(kind) between 1 and 120),
    actor_kind  text not null check (actor_kind in ('public', 'account', 'assistant', 'student')),
    actor_id    text,
    detail      jsonb not null default '{}'::jsonb
                check (jsonb_typeof(detail) = 'object'),
    created_at  timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id, board_id) references boards(site_id, id) on delete cascade,
    foreign key (site_id, card_id, board_id) references board_cards(site_id, id, board_id) on delete cascade
);

create index board_activity_site_board_recent
    on board_activity (site_id, board_id, created_at desc, id desc);

create index board_activity_site_card_recent
    on board_activity (site_id, card_id, created_at desc, id desc)
    where card_id is not null;

create function reject_board_activity_mutation() returns trigger
language plpgsql as $$
begin
    raise exception 'board_activity_is_immutable';
end;
$$;

create trigger board_activity_immutable
before update or delete on board_activity
for each row execute function reject_board_activity_mutation();

do $$
declare
    table_name text;
begin
    foreach table_name in array array[
        'boards', 'board_lists', 'board_cards', 'board_comments', 'board_activity'
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
