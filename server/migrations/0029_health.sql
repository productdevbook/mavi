-- Whether a site's addresses actually work, asked rather than assumed.
--
-- An address that was attached and never pointed here looks exactly like one
-- that works, until somebody visits it. So it is checked on a schedule and
-- what was found is written down — the answer to "why is my site not loading"
-- should be on a screen rather than in somebody's terminal.
create table domain_checks (
    id             uuid primary key default gen_random_uuid(),
    tenant_id      uuid not null references tenants (id) on delete cascade,
    host           text not null,
    -- Whether the name resolves at all, and whether this machine answers on it.
    resolves       boolean not null default false,
    -- Named for what it does rather than "answers": a column of that name is
    -- what a form submission keeps, and this holds nobody's words.
    answered       boolean not null default false,
    -- What came back, for the one who has to fix it.
    note           text,
    checked_at     timestamptz not null default now(),
    unique (tenant_id, host)
);

create index domain_checks_tenant_idx on domain_checks (tenant_id);

alter table domain_checks enable row level security;
alter table domain_checks force row level security;

-- A site sees its own; the machine's own screens see every site's, the same
-- escape the queue uses.
create policy tenant_isolation on domain_checks
    using (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    )
    with check (
        tenant_id = current_tenant_id()
        or current_setting('app.worker', true) = 'on'
    );
