alter table audit_events
    drop constraint audit_events_actor_kind_check;

alter table audit_events
    add constraint audit_events_actor_kind_check
    check (actor_kind in ('public', 'account', 'student', 'assistant', 'system'));

alter table board_activity
    drop constraint board_activity_actor_kind_check;

alter table board_activity
    add constraint board_activity_actor_kind_check
    check (actor_kind in ('public', 'account', 'student', 'assistant', 'system'));
