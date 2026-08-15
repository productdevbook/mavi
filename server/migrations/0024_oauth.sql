-- Signing in with somebody else's account.
--
-- Which providers a site trusts is the site's own business, so the addresses
-- are configured rather than compiled in: what this machine knows is how the
-- exchange goes, not who is on the other end of it.
create table oauth_providers (
    id            uuid primary key default gen_random_uuid(),
    tenant_id     uuid not null references tenants (id) on delete cascade,
    key           text not null check (key ~ '^[a-z][a-z0-9_-]{0,30}$'),
    label         text not null,
    client_id     text not null,
    -- Sealed with the machine's keyring. It never comes back out of the API.
    sealed_secret text not null,
    authorize_url text not null,
    token_url     text not null,
    profile_url   text not null,
    scope         text not null default 'openid email profile',
    enabled       boolean not null default true,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    unique (tenant_id, key)
);

create trigger oauth_providers_touch before update on oauth_providers
    for each row execute function touch_updated_at();

-- One row per attempt, made before anybody leaves and looked for when they come
-- back. This is what makes the answer that arrives an answer to a question this
-- machine actually asked.
create table oauth_attempts (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    provider_id uuid not null references oauth_providers (id) on delete cascade,
    state_hash  bytea not null,
    -- The PKCE verifier, sealed: the proof that whoever comes back with the
    -- code is whoever was sent for it.
    sealed      text not null,
    -- Where in the panel to land afterwards. A path, never a whole address.
    redirect    text not null,
    expires_at  timestamptz not null,
    used_at     timestamptz,
    created_at  timestamptz not null default now(),
    unique (tenant_id, state_hash)
);

create index oauth_attempts_provider_idx on oauth_attempts (provider_id);

create index oauth_attempts_expiry_idx on oauth_attempts (expires_at) where used_at is null;

alter table oauth_providers enable row level security;
alter table oauth_providers force row level security;
alter table oauth_attempts enable row level security;
alter table oauth_attempts force row level security;

create policy tenant_isolation on oauth_providers
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());

create policy tenant_isolation on oauth_attempts
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
