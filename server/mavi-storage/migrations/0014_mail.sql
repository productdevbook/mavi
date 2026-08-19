create table mail_templates (
    site_id       uuid not null references site_catalog(site_id),
    id            uuid not null,
    template_key  text not null check (template_key ~ '^[a-z][a-z0-9_]{0,63}$'),
    language      text not null check (language ~ '^[A-Za-z]{2,8}(-[A-Za-z0-9]{1,8})*$'),
    subject       text not null check (char_length(subject) between 1 and 300),
    body          text not null check (char_length(body) between 1 and 100000),
    content_type  text not null default 'plain' check (content_type in ('plain', 'html')),
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    deleted_at    timestamptz,
    primary key (site_id, id)
);

create unique index mail_templates_site_key_language_active
    on mail_templates (site_id, template_key, language)
    where deleted_at is null;

create index mail_templates_site_recent
    on mail_templates (site_id, created_at desc, id desc)
    where deleted_at is null;

create table mail_lists (
    site_id    uuid not null references site_catalog(site_id),
    id         uuid not null,
    slug       text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
    name       text not null check (char_length(btrim(name)) between 1 and 200),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,
    primary key (site_id, id)
);

create unique index mail_lists_site_slug_active
    on mail_lists (site_id, slug)
    where deleted_at is null;

create index mail_lists_site_recent
    on mail_lists (site_id, created_at desc, id desc)
    where deleted_at is null;

create table mail_readers (
    site_id                 uuid not null references site_catalog(site_id),
    id                      uuid not null,
    email                   text not null check (email = lower(email) and position('@' in email) > 1),
    name                    text,
    standing                text not null default 'subscribed'
                              check (standing in ('subscribed', 'unsubscribed', 'bounced', 'complained')),
    unsubscribe_token_hash  bytea not null,
    created_at              timestamptz not null default now(),
    updated_at              timestamptz not null default now(),
    deleted_at              timestamptz,
    primary key (site_id, id)
);

create unique index mail_readers_site_email_active
    on mail_readers (site_id, email)
    where deleted_at is null;

create unique index mail_readers_site_unsubscribe_token
    on mail_readers (site_id, unsubscribe_token_hash)
    where deleted_at is null;

create table mail_list_members (
    site_id    uuid not null references site_catalog(site_id),
    list_id    uuid not null,
    reader_id  uuid not null,
    created_at timestamptz not null default now(),
    primary key (site_id, list_id, reader_id),
    foreign key (site_id, list_id) references mail_lists(site_id, id) on delete cascade,
    foreign key (site_id, reader_id) references mail_readers(site_id, id) on delete cascade
);

create index mail_list_members_site_list_recent
    on mail_list_members (site_id, list_id, created_at desc, reader_id desc);

create table mail_deliveries (
    site_id             uuid not null references site_catalog(site_id),
    id                  uuid not null,
    template_id         uuid,
    list_id             uuid,
    recipient           text not null check (recipient = lower(recipient) and position('@' in recipient) > 1),
    subject             text not null check (char_length(subject) between 1 and 300),
    body                text not null check (char_length(body) between 1 and 100000),
    content_type        text not null check (content_type in ('plain', 'html')),
    purpose             text not null check (purpose in ('transactional', 'campaign')),
    status              text not null default 'queued'
                         check (status in ('queued', 'sending', 'retry', 'sent', 'dead', 'cancelled')),
    attempts            smallint not null default 0 check (attempts between 0 and 25),
    available_at        timestamptz not null default now(),
    lease_owner         text,
    lease_until         timestamptz,
    provider            text,
    provider_reference  text,
    last_error          text,
    idempotency_key     text,
    created_at          timestamptz not null default now(),
    updated_at          timestamptz not null default now(),
    sent_at             timestamptz,
    primary key (site_id, id),
    foreign key (site_id, template_id) references mail_templates(site_id, id),
    foreign key (site_id, list_id) references mail_lists(site_id, id)
);

create unique index mail_deliveries_site_idempotency
    on mail_deliveries (site_id, idempotency_key)
    where idempotency_key is not null;

create index mail_deliveries_site_queue
    on mail_deliveries (site_id, available_at, created_at, id)
    where status in ('queued', 'retry');

create index mail_deliveries_site_recent
    on mail_deliveries (site_id, created_at desc, id desc);

create table mail_delivery_attempts (
    site_id             uuid not null references site_catalog(site_id),
    id                  uuid not null,
    delivery_id         uuid not null,
    attempt_number      smallint not null check (attempt_number between 1 and 25),
    status              text not null check (status in ('sending', 'sent', 'retry', 'dead')),
    provider            text,
    provider_reference  text,
    error               text,
    started_at          timestamptz not null default now(),
    finished_at         timestamptz,
    primary key (site_id, id),
    unique (site_id, delivery_id, attempt_number),
    foreign key (site_id, delivery_id) references mail_deliveries(site_id, id) on delete cascade,
    check ((status = 'sending') = (finished_at is null))
);

create index mail_delivery_attempts_site_delivery
    on mail_delivery_attempts (site_id, delivery_id, attempt_number desc);

do $$
declare
    table_name text;
begin
    foreach table_name in array array[
        'mail_templates',
        'mail_lists',
        'mail_readers',
        'mail_list_members',
        'mail_deliveries',
        'mail_delivery_attempts'
    ]
    loop
        execute format('alter table %I enable row level security', table_name);
        execute format('alter table %I force row level security', table_name);
        execute format(
            'create policy %I_scope on %I using (site_id = current_setting(''app.site_id'', true)::uuid) with check (site_id = current_setting(''app.site_id'', true)::uuid)',
            table_name,
            table_name
        );
    end loop;
end $$;
