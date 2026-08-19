-- Provider gateways normalize bounce/complaint callbacks before they cross
-- the Mavi boundary. The event key is the gateway's durable idempotency key;
-- the payload is intentionally metadata-only and never contains message body.
create table mail_provider_events (
    site_id            uuid not null references site_catalog(site_id) on delete cascade,
    id                 uuid not null,
    provider           text not null check (char_length(btrim(provider)) between 1 and 64),
    event_id           text not null check (char_length(btrim(event_id)) between 1 and 256),
    delivery_id        uuid,
    recipient          text not null check (recipient = lower(recipient) and position('@' in recipient) > 1),
    kind               text not null check (kind in ('delivered', 'bounced', 'complained')),
    bounce_class       text,
    provider_reference text,
    reason             text,
    occurred_at        timestamptz not null,
    created_at         timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, provider, event_id),
    foreign key (site_id, delivery_id)
        references mail_deliveries(site_id, id) on delete cascade,
    check (
        (kind = 'bounced' and bounce_class in ('transient', 'permanent'))
        or (kind in ('delivered', 'complained') and bounce_class is null)
    ),
    check (provider_reference is null or char_length(provider_reference) between 1 and 1024),
    check (reason is null or char_length(reason) between 1 and 2000)
);

create index mail_provider_events_site_delivery
    on mail_provider_events (site_id, delivery_id, occurred_at desc)
    where delivery_id is not null;

create index mail_provider_events_site_recipient
    on mail_provider_events (site_id, recipient, occurred_at desc);

alter table mail_provider_events enable row level security;
alter table mail_provider_events force row level security;
create policy mail_provider_events_scope on mail_provider_events
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
