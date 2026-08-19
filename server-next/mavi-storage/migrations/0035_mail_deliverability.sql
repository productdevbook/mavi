-- Campaign unsubscribe links are bearer secrets. Keep only their hashes in
-- the reader/token catalog and keep the rendered URL sealed in the outbox so
-- ordinary delivery inspection and portable exports cannot disclose it.
create table mail_unsubscribe_tokens (
    site_id      uuid not null references site_catalog(site_id) on delete cascade,
    id           uuid not null,
    delivery_id  uuid not null,
    reader_id    uuid not null,
    token_hash   bytea not null check (octet_length(token_hash) = 32),
    created_at   timestamptz not null default now(),
    used_at      timestamptz,
    primary key (site_id, id),
    constraint mail_unsubscribe_tokens_site_token_hash unique (site_id, token_hash),
    constraint mail_unsubscribe_tokens_site_delivery unique (site_id, delivery_id),
    foreign key (site_id, delivery_id)
        references mail_deliveries(site_id, id) on delete cascade,
    foreign key (site_id, reader_id)
        references mail_readers(site_id, id) on delete cascade
);

create index mail_unsubscribe_tokens_site_reader
    on mail_unsubscribe_tokens (site_id, reader_id, created_at desc);

create table mail_delivery_links (
    site_id      uuid not null,
    delivery_id  uuid not null,
    ciphertext   bytea not null check (octet_length(ciphertext) between 1 and 8192),
    primary key (site_id, delivery_id),
    foreign key (site_id, delivery_id)
        references mail_deliveries(site_id, id) on delete cascade
);

create index mail_delivery_links_site_delivery
    on mail_delivery_links (site_id, delivery_id);

alter table mail_unsubscribe_tokens enable row level security;
alter table mail_unsubscribe_tokens force row level security;
create policy mail_unsubscribe_tokens_scope on mail_unsubscribe_tokens
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);

alter table mail_delivery_links enable row level security;
alter table mail_delivery_links force row level security;
create policy mail_delivery_links_scope on mail_delivery_links
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
