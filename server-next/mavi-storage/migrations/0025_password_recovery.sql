create table password_reset_tokens (
    site_id     uuid not null references site_catalog(site_id),
    id          uuid not null,
    person_id   uuid not null,
    token_hash  bytea not null check (octet_length(token_hash) = 32),
    expires_at  timestamptz not null,
    used_at     timestamptz,
    revoked_at  timestamptz,
    created_at  timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, token_hash),
    foreign key (site_id, person_id) references people(site_id, id),
    check (used_at is null or revoked_at is null)
);

create index password_reset_tokens_site_person_active
    on password_reset_tokens (site_id, person_id, expires_at)
    where used_at is null and revoked_at is null;

create index password_reset_tokens_site_expiry
    on password_reset_tokens (site_id, expires_at)
    where used_at is null and revoked_at is null;

alter table password_reset_tokens enable row level security;
alter table password_reset_tokens force row level security;
create policy password_reset_tokens_scope on password_reset_tokens
    using (site_id = current_setting('app.site_id', true)::uuid)
    with check (site_id = current_setting('app.site_id', true)::uuid);
