-- What is being worked on.
--
-- A board is stages and cards. The only decision here that is not obvious is
-- what a card's place is: a number between its neighbours' numbers, so that
-- dragging one is one row changed rather than every row below it.

create table boards (
    id         uuid primary key,
    name       text not null check (length(name) between 1 and 200),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

create index boards_recent
    on boards (created_at desc, id desc)
    where deleted_at is null;

create table stages (
    id         uuid primary key,
    board_id   uuid not null references boards (id) on delete cascade,
    name       text not null check (length(name) between 1 and 100),
    place      integer not null check (place >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),

    -- Deferred, like every other order in this schema: rearranging columns
    -- means writing one into a place another is still in.
    constraint one_stage_to_a_place unique (board_id, place) deferrable initially deferred
);

create index stages_in_order on stages (board_id, place);

create table cards (
    id         uuid primary key,
    board_id   uuid not null references boards (id) on delete cascade,
    -- `restrict`, not `cascade`: deleting a column that still has cards in it
    -- would take somebody's work with it, and what they want is to be told to
    -- move them first.
    stage_id   uuid not null references stages (id) on delete restrict,
    title      text not null check (length(title) between 1 and 300),
    detail     text,
    -- Whose it is, and what it is worth. Both optional, because a board is a
    -- board before it is a pipeline. Text rather than a reference for the
    -- owner, for the reason a receipt's is: the account may be gone and the
    -- card is still somebody's work.
    owner      text,
    worth_minor bigint check (worth_minor >= 0),
    currency   text check (currency ~ '^[A-Z]{3}$'),
    -- Between its neighbours. A double, and what happens when two neighbours
    -- are as close as a double can hold is a refusal in the code rather than
    -- two cards in one place.
    -- `place = place` is false for NaN and true for every real number, which
    -- is the shortest way to say that a card's place is a number. A NaN in
    -- this column sorts wherever the planner likes and compares equal to
    -- nothing, including itself.
    place      double precision not null default 0 check (place = place),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,

    -- An amount is a number and a currency or it is neither. Half of it is a
    -- number nobody can add up.
    constraint worth_is_money check ((worth_minor is null) = (currency is null))
);

-- What a board draws: one stage's cards, in the order somebody dragged them
-- into, and the keyset matches it column for column.
create index cards_in_place
    on cards (stage_id, place, id)
    where deleted_at is null;

create index cards_of_a_board on cards (board_id) where deleted_at is null;
