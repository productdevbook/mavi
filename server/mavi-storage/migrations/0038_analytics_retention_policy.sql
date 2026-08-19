-- Retention is site policy. The worker reads it in the site transaction and
-- enqueues one idempotent UTC-day job; the raw and aggregate windows are kept
-- separately because aggregate data is deliberately longer-lived.
alter table site_settings
    add column analytics_raw_retention_days smallint not null default 365,
    add column analytics_aggregate_retention_days smallint not null default 3650;

alter table site_settings
    add constraint site_settings_analytics_raw_retention_check
    check (analytics_raw_retention_days between 1 and 3650),
    add constraint site_settings_analytics_aggregate_retention_check
    check (analytics_aggregate_retention_days between 1 and 3650);
