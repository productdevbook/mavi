-- Soft-deleted rows remain restorable for a bounded, site-configured period.
-- Permanent deletion is still performed by the durable worker so media bytes
-- can be removed through FileStore after the metadata transaction commits.
alter table site_settings
    add column trash_retention_days smallint not null default 30;

alter table site_settings
    add constraint site_settings_trash_retention_check
    check (trash_retention_days between 1 and 3650);
