-- Board trash keeps the user's existing archive state separate from the
-- reversible root-board tombstone. Flow trash does the same for enablement.
alter table boards
    add column trash_archived boolean not null default false;

alter table board_cards
    add column deleted_at timestamptz;

alter table automation_flows
    add column trash_enabled boolean not null default false;

-- Activity is immutable to callers, but a parent board purge must be able to
-- cascade its historical activity rows.
drop trigger if exists board_activity_immutable on board_activity;
create trigger board_activity_immutable
before update on board_activity
for each row execute function reject_board_activity_mutation();

-- Tombstoned lists/cards must not reserve active positions. Partial indexes
-- retain the original constraint names while allowing a restored/new item to
-- reuse a position after the old row has entered site trash.
alter table board_lists drop constraint if exists board_lists_site_position;
create unique index board_lists_site_position
    on board_lists (site_id, board_id, position)
    where deleted_at is null;

alter table board_cards drop constraint if exists board_cards_site_position;
create unique index board_cards_site_position
    on board_cards (site_id, list_id, position)
    where archived_at is null and deleted_at is null;

drop index if exists board_cards_site_list_order;
create index board_cards_site_list_order
    on board_cards (site_id, list_id, position, id)
    where archived_at is null and deleted_at is null;

drop index if exists board_cards_site_assignee;
create index board_cards_site_assignee
    on board_cards (site_id, assignee_id, updated_at desc, id desc)
    where archived_at is null and deleted_at is null and assignee_id is not null;

