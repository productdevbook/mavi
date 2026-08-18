-- Validate the protected-body constraint separately so the existing outbox
-- table is not held under the validation lock while migration 0027 runs.
alter table mail_deliveries
    validate constraint mail_deliveries_body_protection_check;
