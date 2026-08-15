create table mail_templates (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    key        text not null check (key ~ '^[a-z][a-z0-9_.-]{0,60}$'),
    subject    text not null check (length(subject) between 1 and 300),
    body       text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, key)
);

create trigger mail_templates_touch before update on mail_templates
    for each row execute function touch_updated_at();

create index mail_templates_tenant_idx on mail_templates (tenant_id);

create table mail_lists (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    name       text not null check (length(name) between 1 and 200),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger mail_lists_touch before update on mail_lists
    for each row execute function touch_updated_at();

create index mail_lists_tenant_idx on mail_lists (tenant_id);

create type subscriber_state as enum ('subscribed', 'unsubscribed', 'bounced', 'complained');

create table subscribers (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    email       text not null check (email = lower(email) and position('@' in email) > 1),
    name        text,
    state       subscriber_state not null default 'subscribed',
    -- What an unsubscribe link carries. Hashed, because a link in somebody's
    -- inbox is a link in whatever reads their inbox.
    token_hash  bytea not null unique,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now(),
    unsubscribed_at timestamptz,
    unique (tenant_id, email)
);

create trigger subscribers_touch before update on subscribers
    for each row execute function touch_updated_at();

create index subscribers_tenant_idx on subscribers (tenant_id, state);

create table subscriber_lists (
    subscriber_id uuid not null references subscribers (id) on delete cascade,
    list_id       uuid not null references mail_lists (id) on delete cascade,
    tenant_id     uuid not null references tenants (id) on delete cascade,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    primary key (subscriber_id, list_id)
);

create trigger subscriber_lists_touch before update on subscriber_lists
    for each row execute function touch_updated_at();

create index subscriber_lists_list_idx on subscriber_lists (list_id);
create index subscriber_lists_tenant_idx on subscriber_lists (tenant_id);

create type campaign_state as enum ('draft', 'sending', 'sent', 'cancelled');

create table campaigns (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    list_id     uuid not null references mail_lists (id) on delete restrict,
    subject     text not null check (length(subject) between 1 and 300),
    body        text not null,
    state       campaign_state not null default 'draft',
    -- Where the last batch stopped. A campaign picks up from here rather than
    -- reading everything it has already sent to in order to find the next
    -- twenty-five.
    sent_through uuid,
    sent_count  integer not null default 0 check (sent_count >= 0),
    started_at  timestamptz,
    finished_at timestamptz,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger campaigns_touch before update on campaigns
    for each row execute function touch_updated_at();

create index campaigns_tenant_idx on campaigns (tenant_id, created_at desc);
create index campaigns_list_idx on campaigns (list_id);

create type mail_state as enum ('queued', 'sent', 'failed', 'bounced', 'complained');

-- Every message this machine sends on a site's behalf, campaign or not. What
-- is not written down here is not billed, and mail that is not a campaign was
-- exactly what went uncounted before.
create table email_log (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    campaign_id  uuid references campaigns (id) on delete set null,
    subscriber_id uuid references subscribers (id) on delete set null,
    to_email     text not null,
    subject      text not null,
    state        mail_state not null default 'queued',
    provider_ref text,
    failure      text,
    sent_at      timestamptz,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now()
);

create trigger email_log_touch before update on email_log
    for each row execute function touch_updated_at();

create index email_log_tenant_idx on email_log (tenant_id, created_at desc);
create index email_log_campaign_idx on email_log (campaign_id);
create index email_log_subscriber_idx on email_log (subscriber_id);
create index email_log_queued_idx on email_log (created_at) where state = 'queued';

alter table mail_templates   enable row level security;
alter table mail_lists       enable row level security;
alter table subscribers      enable row level security;
alter table subscriber_lists enable row level security;
alter table campaigns        enable row level security;
alter table email_log        enable row level security;

alter table mail_templates   force row level security;
alter table mail_lists       force row level security;
alter table subscribers      force row level security;
alter table subscriber_lists force row level security;
alter table campaigns        force row level security;
alter table email_log        force row level security;

create policy tenant_isolation on mail_templates
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on mail_lists
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on subscribers
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on subscriber_lists
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on campaigns
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on email_log
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
