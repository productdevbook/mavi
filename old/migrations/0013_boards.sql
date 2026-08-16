create table boards (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    name       text not null check (length(name) between 1 and 200),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

create trigger boards_touch before update on boards
    for each row execute function touch_updated_at();

create index boards_tenant_idx on boards (tenant_id, created_at desc);

create table board_stages (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    board_id   uuid not null references boards (id) on delete cascade,
    name       text not null check (length(name) between 1 and 100),
    position   integer not null check (position >= 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, board_id, position)
);

create trigger board_stages_touch before update on board_stages
    for each row execute function touch_updated_at();

create index board_stages_board_idx on board_stages (board_id, position);
create index board_stages_tenant_idx on board_stages (tenant_id);

create table cards (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    board_id   uuid not null references boards (id) on delete cascade,
    stage_id   uuid not null references board_stages (id) on delete restrict,
    title      text not null check (length(title) between 1 and 300),
    detail     text,
    -- Whose it is, and what it is worth. Both optional: a board is a board
    -- before it is a pipeline.
    owner_id   uuid references users (id) on delete set null,
    value_minor bigint check (value_minor >= 0),
    currency   currency,
    -- Where it sits in its stage. A float so that moving a card between two
    -- others is one row changed rather than every row below it.
    position   double precision not null default 0,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,
    check ((value_minor is null) = (currency is null))
);

create trigger cards_touch before update on cards
    for each row execute function touch_updated_at();

create index cards_stage_idx on cards (stage_id, position) where deleted_at is null;
create index cards_board_idx on cards (board_id);
create index cards_owner_idx on cards (owner_id);
create index cards_tenant_idx on cards (tenant_id, created_at desc);

create table card_notes (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    card_id    uuid not null references cards (id) on delete cascade,
    author_id  uuid references users (id) on delete set null,
    body       text not null check (length(body) between 1 and 10000),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger card_notes_touch before update on card_notes
    for each row execute function touch_updated_at();

create index card_notes_card_idx on card_notes (card_id, created_at desc);
create index card_notes_author_idx on card_notes (author_id);
create index card_notes_tenant_idx on card_notes (tenant_id);

alter table boards       enable row level security;
alter table board_stages enable row level security;
alter table cards        enable row level security;
alter table card_notes   enable row level security;

alter table boards       force row level security;
alter table board_stages force row level security;
alter table cards        force row level security;
alter table card_notes   force row level security;

create policy tenant_isolation on boards
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on board_stages
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on cards
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on card_notes
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
