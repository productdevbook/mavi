-- Security-sensitive system messages (password reset and email verification)
-- must not persist their one-time token in the ordinary mail outbox body.
alter table mail_deliveries
    add column body_protected boolean not null default false;

alter table mail_deliveries
    add constraint mail_deliveries_body_protection_check
    check ((not body_protected) or body = '[protected]') not valid;

create table mail_delivery_secrets (
    site_id      uuid not null,
    delivery_id  uuid not null,
    ciphertext   bytea not null check (octet_length(ciphertext) between 1 and 1000000),
    primary key (site_id, delivery_id),
    foreign key (site_id, delivery_id)
        references mail_deliveries(site_id, id) on delete cascade
);

alter table mail_delivery_secrets enable row level security;
alter table mail_delivery_secrets force row level security;
create policy mail_delivery_secrets_scope on mail_delivery_secrets
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
