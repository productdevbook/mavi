-- A cancelled pending outbox intent must never be claimed after the Hatchet
-- run-control request succeeds. This is forward-only so existing databases
-- can adopt cancellation without rewriting the original workflow migration.
alter table workflow_outbox
    drop constraint if exists workflow_outbox_status_check;

alter table workflow_outbox
    add constraint workflow_outbox_status_check
    check (status in ('pending', 'publishing', 'published', 'failed', 'cancelled'));
