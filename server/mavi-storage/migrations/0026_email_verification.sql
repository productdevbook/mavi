-- Existing accounts receive the migration-time default so an upgrade cannot
-- lock out a site owner; newly created people are explicitly unverified in the
-- domain command by inserting NULL.
alter table people add column email_verified_at timestamptz default now();

create table email_verification_tokens (
    site_id     uuid not null,
    id          uuid not null,
    person_id   uuid not null,
    token_hash  bytea not null check (octet_length(token_hash) = 32),
    expires_at  timestamptz not null,
    used_at     timestamptz,
    revoked_at  timestamptz,
    created_at  timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, token_hash),
    foreign key (site_id, person_id) references people(site_id, id)
);

create index email_verification_tokens_site_person_active
    on email_verification_tokens (site_id, person_id, expires_at)
    where used_at is null and revoked_at is null;

create index email_verification_tokens_site_expiry
    on email_verification_tokens (site_id, expires_at)
    where used_at is null and revoked_at is null;

-- Recovery and verification are subject-window throttles. IP/device limits
-- belong at the edge, where those signals exist; this table prevents an
-- attacker from repeatedly mailing one account from any runtime.
create table auth_request_throttles (
    site_id          uuid not null references site_catalog(site_id),
    action           text not null check (action in ('password_reset', 'email_verification')),
    subject_hash     bytea not null check (octet_length(subject_hash) = 32),
    window_started_at timestamptz not null,
    request_count    integer not null check (request_count between 1 and 1000),
    updated_at       timestamptz not null default now(),
    primary key (site_id, action, subject_hash)
);

create index auth_request_throttles_updated_at
    on auth_request_throttles (site_id, updated_at);

do $$
declare
    table_name text;
begin
    foreach table_name in array array['email_verification_tokens', 'auth_request_throttles']
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
