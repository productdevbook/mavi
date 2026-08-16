-- What a message is for, so that the one somebody asked for by acting and the
-- one a list sent them are told apart: a receipt has no unsubscribe link on it
-- and a campaign always does.
create type mail_purpose as enum ('transactional', 'campaign');

alter table email_log add column purpose mail_purpose not null default 'transactional';
alter table email_log add column body text not null default '';
alter table email_log add column attempts integer not null default 0 check (attempts >= 0);

create index email_log_purpose_idx on email_log (tenant_id, purpose, created_at desc);

-- What a provider said about an address afterwards. One row per event, so a
-- bounce arriving twice is one bounce.
create table mail_events (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    email_log_id uuid references email_log (id) on delete set null,
    kind         text not null check (kind in ('delivered', 'bounced', 'complained')),
    -- What the provider called this event. Two of the same are one.
    provider_ref text not null,
    detail       text,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    unique (tenant_id, provider_ref)
);

create trigger mail_events_touch before update on mail_events
    for each row execute function touch_updated_at();

create index mail_events_log_idx on mail_events (email_log_id);
create index mail_events_tenant_idx on mail_events (tenant_id, created_at desc);

alter table mail_events enable row level security;
alter table mail_events force row level security;

create policy tenant_isolation on mail_events
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
