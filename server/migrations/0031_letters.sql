-- What a site's own letters say.
--
-- The invitation, the password link, the receipt: written by this machine
-- until a site says otherwise, in its own words and its own language when it
-- does. A site whose receipt says "Mavi CMS" in English to a Turkish customer
-- is a site that looks like somebody else's.
create table letters (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    -- Which letter, from the list in the code. A name nothing sends is refused
    -- rather than kept for ever against a letter that no longer exists.
    kind       text not null,
    -- Which language this is the wording for. One row per language, so a site
    -- writing Turkish does not lose its English.
    language   text not null,
    subject    text not null check (length(subject) between 1 and 300),
    body       text not null check (length(body) between 1 and 20000),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, kind, language)
);

create trigger letters_touch before update on letters
    for each row execute function touch_updated_at();

alter table letters enable row level security;
alter table letters force row level security;

create policy tenant_isolation on letters
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
