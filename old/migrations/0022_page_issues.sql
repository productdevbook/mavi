-- What is wrong with a page, worked out when it changes rather than when
-- somebody asks: a screen that computes this on every visit is a screen that
-- reads every post to draw a badge.
create type issue_weight as enum ('note', 'warning');

create table page_issues (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    post_id    uuid not null references posts (id) on delete cascade,
    -- What kind of thing is wrong, from a fixed list in the code. Not a
    -- sentence: the sentence belongs to whatever is showing it, in whatever
    -- language that is being shown in.
    kind       text not null,
    weight     issue_weight not null,
    -- What it was measured to be, where a number is what makes it wrong.
    detail     jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (tenant_id, post_id, kind)
);

create trigger page_issues_touch before update on page_issues
    for each row execute function touch_updated_at();

create index page_issues_post_idx on page_issues (post_id);
create index page_issues_tenant_idx on page_issues (tenant_id, weight);

alter table page_issues enable row level security;
alter table page_issues force row level security;

create policy tenant_isolation on page_issues
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
