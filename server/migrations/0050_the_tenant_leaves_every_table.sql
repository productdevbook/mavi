-- The tenant leaves every table that carried one.
--
-- This is the change that cannot be undone. What follows deletes the only
-- record of which row belonged to which site; nothing after it can tell them
-- apart, and no tool in this repository will put them back. 0049 refuses to
-- run at all on a database holding more than one site, which is the only guard
-- there is and the reason it comes first.
--
-- What this file does, in order, and why the order is the whole of it:
--
--   1. drops the policy on all 65 tables that still had one
--   2. turns row-level security off on them
--   3. drops the column
--   4. puts back everything step 3 took away silently
--
-- Step 3 is the trap. `alter table ... drop column` also drops every index,
-- every unique constraint and every primary key that so much as mentions the
-- column, and says nothing about any of it. Everything below step 3 exists
-- because of that, and the counts are worth reading before trusting this file:
-- 65 columns, 65 policies, 4 keys, 2 singletons, 47 indexes, 28 uniques.
--
-- === the policies, and whether any was doing work beyond isolation ===
--
-- No. Asked of the catalogue rather than assumed: every one of the 65 policies
-- is named `tenant_isolation` and every expression is either
--
--     (tenant_id = current_tenant_id())
--
-- or that same test with one escape hatch OR'd onto it,
--
--     or (current_setting('app.worker', true) = 'on')
--
-- and there is not a third shape anywhere. Six tables carried the escape —
-- `domain_checks`, `jobs`, `reports`, `site_settings`, `tenant_domains` and
-- `transfers` — though only one of them carries the comment explaining it, so
-- reading the comments would have found one of six. `ledger` was a seventh and
-- went with 0049.
--
-- The escape said "and the machine's own work may read across every site". It
-- is not isolation, but it is not separable from it either: it exists only
-- because isolation exists. The session setting it read is set in one place in
-- the application and goes in the same change, so it does not outlive its
-- readers.
--
-- === what every per-site uniqueness became ===
--
-- Four keys were part tenant. Three of them keep the rest of themselves —
-- `counters (kind)`, `page_views (on_day, path)`, `visitor_marks (on_day,
-- mark)` — and `site_settings` had `tenant_id` as its *entire* primary key, so
-- dropping the column leaves a table with no key and nothing stopping a second
-- settings row. It gets the singleton idiom instead.
--
-- Two unique indexes were singletons rather than scopes: their only column was
-- `tenant_id`, so they said "at most one row per site" rather than "unique
-- within a site". Dropping the column does not flatten those, it deletes the
-- invariant. `languages_one_default` and `publishes_one_at_a_time` are
-- recreated as `((true))` — at most one row, full stop. The second matters
-- beyond tidiness: the build claim reads it, and without it two builds run at
-- once. A third, `tenant_domains_one_primary`, goes with its table in 0051.
--
-- Twenty-eight uniques were genuinely scoped and flatten by losing the column.
-- Two are worth naming. `orders_number_is_a_site_s_own` was scoped so that how
-- many orders the machine had taken altogether stayed private between sites;
-- with one site there is nobody to keep it from, so it flattens and loses its
-- name, which had stopped being true. `recovery_codes (tenant_id, code_hash)`
-- becomes `(code_hash)`, which is stricter than it was and is what it should
-- always have been.
--
-- Four uniques were global on purpose and are not touched by any of this: the
-- 256-bit token hashes on `sessions`, `student_sessions`, `tickets` and
-- `subscribers`. They never carried the tenant and a sweep that "flattened"
-- them would have been a sweep that broke them.
--
-- === the indexes ===
--
-- 47 led with the site and are recreated reading without it. Four of those led
-- with a *non-tenant* column and are the only cover their foreign key has —
-- `users_role_idx`, `sessions_user_idx`, `tickets_user_idx` and
-- `student_sessions_student_idx`. Dropping the column without putting those
-- back turns four foreign keys into sequential scans, and the schema test that
-- asks every foreign key what reads it goes red on exactly those four.
--
-- Twenty-eight more had `tenant_id` as their only column: "this site's rows".
-- Those are not recreated, because with one site that is every row and the
-- index is the table.
--
-- Twenty-four of the recreated ones were named for the thing they no longer
-- lead with. They are renamed here rather than left saying `tenant`: an index
-- called `posts_tenant_idx` that is not about a tenant is a name somebody will
-- trust later.
-- The policy on every one of them, and the setting it read.
drop policy tenant_isolation on audit_log;
drop policy tenant_isolation on board_stages;
drop policy tenant_isolation on boards;
drop policy tenant_isolation on campaigns;
drop policy tenant_isolation on card_notes;
drop policy tenant_isolation on cards;
drop policy tenant_isolation on content_types;
drop policy tenant_isolation on counters;
drop policy tenant_isolation on coupon_uses;
drop policy tenant_isolation on coupons;
drop policy tenant_isolation on courses;
drop policy tenant_isolation on domain_checks;
drop policy tenant_isolation on email_log;
drop policy tenant_isolation on enrolments;
drop policy tenant_isolation on flow_credentials;
drop policy tenant_isolation on flow_run_steps;
drop policy tenant_isolation on flow_runs;
drop policy tenant_isolation on flow_steps;
drop policy tenant_isolation on flows;
drop policy tenant_isolation on form_submissions;
drop policy tenant_isolation on forms;
drop policy tenant_isolation on jobs;
drop policy tenant_isolation on languages;
drop policy tenant_isolation on lesson_progress;
drop policy tenant_isolation on lessons;
drop policy tenant_isolation on letters;
drop policy tenant_isolation on mail_events;
drop policy tenant_isolation on mail_lists;
drop policy tenant_isolation on media;
drop policy tenant_isolation on modules;
drop policy tenant_isolation on oauth_attempts;
drop policy tenant_isolation on oauth_providers;
drop policy tenant_isolation on order_items;
drop policy tenant_isolation on orders;
drop policy tenant_isolation on outbox;
drop policy tenant_isolation on page_issues;
drop policy tenant_isolation on page_views;
drop policy tenant_isolation on payments;
drop policy tenant_isolation on plugins;
drop policy tenant_isolation on post_terms;
drop policy tenant_isolation on posts;
drop policy tenant_isolation on products;
drop policy tenant_isolation on publishes;
drop policy tenant_isolation on recovery_codes;
drop policy tenant_isolation on redirects;
drop policy tenant_isolation on reports;
drop policy tenant_isolation on roles;
drop policy tenant_isolation on second_factors;
drop policy tenant_isolation on sessions;
drop policy tenant_isolation on site_settings;
drop policy tenant_isolation on stock_holds;
drop policy tenant_isolation on student_sessions;
drop policy tenant_isolation on students;
drop policy tenant_isolation on subscriber_lists;
drop policy tenant_isolation on subscribers;
drop policy tenant_isolation on terms;
drop policy tenant_isolation on theme_files;
drop policy tenant_isolation on tickets;
drop policy tenant_isolation on transfers;
drop policy tenant_isolation on users;
drop policy tenant_isolation on videos;
drop policy tenant_isolation on visitor_marks;
drop policy tenant_isolation on vitals;
drop policy tenant_isolation on webhook_deliveries;
drop policy tenant_isolation on webhook_endpoints;

alter table audit_log no force row level security;
alter table audit_log disable row level security;
alter table board_stages no force row level security;
alter table board_stages disable row level security;
alter table boards no force row level security;
alter table boards disable row level security;
alter table campaigns no force row level security;
alter table campaigns disable row level security;
alter table card_notes no force row level security;
alter table card_notes disable row level security;
alter table cards no force row level security;
alter table cards disable row level security;
alter table content_types no force row level security;
alter table content_types disable row level security;
alter table counters no force row level security;
alter table counters disable row level security;
alter table coupon_uses no force row level security;
alter table coupon_uses disable row level security;
alter table coupons no force row level security;
alter table coupons disable row level security;
alter table courses no force row level security;
alter table courses disable row level security;
alter table domain_checks no force row level security;
alter table domain_checks disable row level security;
alter table email_log no force row level security;
alter table email_log disable row level security;
alter table enrolments no force row level security;
alter table enrolments disable row level security;
alter table flow_credentials no force row level security;
alter table flow_credentials disable row level security;
alter table flow_run_steps no force row level security;
alter table flow_run_steps disable row level security;
alter table flow_runs no force row level security;
alter table flow_runs disable row level security;
alter table flow_steps no force row level security;
alter table flow_steps disable row level security;
alter table flows no force row level security;
alter table flows disable row level security;
alter table form_submissions no force row level security;
alter table form_submissions disable row level security;
alter table forms no force row level security;
alter table forms disable row level security;
alter table jobs no force row level security;
alter table jobs disable row level security;
alter table languages no force row level security;
alter table languages disable row level security;
alter table lesson_progress no force row level security;
alter table lesson_progress disable row level security;
alter table lessons no force row level security;
alter table lessons disable row level security;
alter table letters no force row level security;
alter table letters disable row level security;
alter table mail_events no force row level security;
alter table mail_events disable row level security;
alter table mail_lists no force row level security;
alter table mail_lists disable row level security;
alter table media no force row level security;
alter table media disable row level security;
alter table modules no force row level security;
alter table modules disable row level security;
alter table oauth_attempts no force row level security;
alter table oauth_attempts disable row level security;
alter table oauth_providers no force row level security;
alter table oauth_providers disable row level security;
alter table order_items no force row level security;
alter table order_items disable row level security;
alter table orders no force row level security;
alter table orders disable row level security;
alter table outbox no force row level security;
alter table outbox disable row level security;
alter table page_issues no force row level security;
alter table page_issues disable row level security;
alter table page_views no force row level security;
alter table page_views disable row level security;
alter table payments no force row level security;
alter table payments disable row level security;
alter table plugins no force row level security;
alter table plugins disable row level security;
alter table post_terms no force row level security;
alter table post_terms disable row level security;
alter table posts no force row level security;
alter table posts disable row level security;
alter table products no force row level security;
alter table products disable row level security;
alter table publishes no force row level security;
alter table publishes disable row level security;
alter table recovery_codes no force row level security;
alter table recovery_codes disable row level security;
alter table redirects no force row level security;
alter table redirects disable row level security;
alter table reports no force row level security;
alter table reports disable row level security;
alter table roles no force row level security;
alter table roles disable row level security;
alter table second_factors no force row level security;
alter table second_factors disable row level security;
alter table sessions no force row level security;
alter table sessions disable row level security;
alter table site_settings no force row level security;
alter table site_settings disable row level security;
alter table stock_holds no force row level security;
alter table stock_holds disable row level security;
alter table student_sessions no force row level security;
alter table student_sessions disable row level security;
alter table students no force row level security;
alter table students disable row level security;
alter table subscriber_lists no force row level security;
alter table subscriber_lists disable row level security;
alter table subscribers no force row level security;
alter table subscribers disable row level security;
alter table terms no force row level security;
alter table terms disable row level security;
alter table theme_files no force row level security;
alter table theme_files disable row level security;
alter table tickets no force row level security;
alter table tickets disable row level security;
alter table transfers no force row level security;
alter table transfers disable row level security;
alter table users no force row level security;
alter table users disable row level security;
alter table videos no force row level security;
alter table videos disable row level security;
alter table visitor_marks no force row level security;
alter table visitor_marks disable row level security;
alter table vitals no force row level security;
alter table vitals disable row level security;
alter table webhook_deliveries no force row level security;
alter table webhook_deliveries disable row level security;
alter table webhook_endpoints no force row level security;
alter table webhook_endpoints disable row level security;

-- The one foreign key that was part tenant, and is not a reference to
-- `tenants`. A post's type is a type this site declared, said as
-- `(tenant_id, type_key) -> content_types (tenant_id, key)`. It is dropped
-- here and put back at the bottom, once `content_types` has the flattened
-- unique for it to point at. It cannot simply be left: a column cannot be
-- dropped while a foreign key names it, and this one refuses out loud rather
-- than going quietly the way the indexes do.
alter table posts drop constraint posts_type_is_one_the_site_declared;

-- The column itself. Every index and every constraint that mentions it
-- goes with it, silently, which is what the rest of this file is about.
alter table audit_log drop column tenant_id;
alter table board_stages drop column tenant_id;
alter table boards drop column tenant_id;
alter table campaigns drop column tenant_id;
alter table card_notes drop column tenant_id;
alter table cards drop column tenant_id;
alter table content_types drop column tenant_id;
alter table counters drop column tenant_id;
alter table coupon_uses drop column tenant_id;
alter table coupons drop column tenant_id;
alter table courses drop column tenant_id;
alter table domain_checks drop column tenant_id;
alter table email_log drop column tenant_id;
alter table enrolments drop column tenant_id;
alter table flow_credentials drop column tenant_id;
alter table flow_run_steps drop column tenant_id;
alter table flow_runs drop column tenant_id;
alter table flow_steps drop column tenant_id;
alter table flows drop column tenant_id;
alter table form_submissions drop column tenant_id;
alter table forms drop column tenant_id;
alter table jobs drop column tenant_id;
alter table languages drop column tenant_id;
alter table lesson_progress drop column tenant_id;
alter table lessons drop column tenant_id;
alter table letters drop column tenant_id;
alter table mail_events drop column tenant_id;
alter table mail_lists drop column tenant_id;
alter table media drop column tenant_id;
alter table modules drop column tenant_id;
alter table oauth_attempts drop column tenant_id;
alter table oauth_providers drop column tenant_id;
alter table order_items drop column tenant_id;
alter table orders drop column tenant_id;
alter table outbox drop column tenant_id;
alter table page_issues drop column tenant_id;
alter table page_views drop column tenant_id;
alter table payments drop column tenant_id;
alter table plugins drop column tenant_id;
alter table post_terms drop column tenant_id;
alter table posts drop column tenant_id;
alter table products drop column tenant_id;
alter table publishes drop column tenant_id;
alter table recovery_codes drop column tenant_id;
alter table redirects drop column tenant_id;
alter table reports drop column tenant_id;
alter table roles drop column tenant_id;
alter table second_factors drop column tenant_id;
alter table sessions drop column tenant_id;
alter table site_settings drop column tenant_id;
alter table stock_holds drop column tenant_id;
alter table student_sessions drop column tenant_id;
alter table students drop column tenant_id;
alter table subscriber_lists drop column tenant_id;
alter table subscribers drop column tenant_id;
alter table terms drop column tenant_id;
alter table theme_files drop column tenant_id;
alter table tickets drop column tenant_id;
alter table transfers drop column tenant_id;
alter table users drop column tenant_id;
alter table videos drop column tenant_id;
alter table visitor_marks drop column tenant_id;
alter table vitals drop column tenant_id;
alter table webhook_deliveries drop column tenant_id;
alter table webhook_endpoints drop column tenant_id;

-- The keys that were part tenant.
alter table counters add primary key (kind);

-- And the two functions that counted per site. Postgres does not check a
-- PL/pgSQL body against the tables it names until something runs it, so
-- dropping `counters.tenant_id` above left `number_an_order` reading a field
-- that is not there and every order failing on insert — a break no compiler,
-- and no preparing of this crate's own statements, can see.
create function next_number(of_what text) returns bigint
language sql as $$
    insert into counters (kind, next) values (of_what, 2)
    on conflict (kind) do update set next = counters.next + 1
    returning case when counters.next is null then 1 else counters.next - 1 end;
$$;

create or replace function number_an_order() returns trigger language plpgsql as $$
begin
    if new.number is null then
        new.number := next_number('order');
    end if;

    return new;
end;
$$;

drop function next_number(uuid, text);
alter table page_views add primary key (on_day, path);
create unique index site_settings_is_one_row on site_settings ((true));
alter table visitor_marks add primary key (on_day, mark);

-- At most one row, where the scope used to say at most one per site.
create unique index languages_one_default on languages ((true)) where is_default;
create unique index publishes_one_at_a_time on publishes ((true)) where (state = any (array['queued'::publish_state, 'building'::publish_state]));

-- Every index that led with the site, reading without it.
create index audit_log_subject_idx on audit_log (subject, subject_id);
create index audit_log_newest_idx on audit_log (created_at DESC);
create index boards_newest_idx on boards (created_at DESC);
create index campaigns_newest_idx on campaigns (created_at DESC);
create index cards_newest_idx on cards (created_at DESC);
create unique index courses_slug_idx on courses (slug) where (deleted_at is null);
create index courses_newest_idx on courses (created_at DESC);
create index email_log_purpose_idx on email_log (purpose, created_at DESC);
create index email_log_newest_idx on email_log (created_at DESC);
create index enrolments_ending_idx on enrolments (ends_at) where (ends_at is not null);
create index flow_runs_newest_idx on flow_runs (started_at DESC);
create index flows_trigger_idx on flows (trigger) where (active and (deleted_at is null));
create index form_submissions_newest_idx on form_submissions (created_at DESC);
create unique index forms_slug_idx on forms (slug) where (deleted_at is null);
create index forms_newest_idx on forms (created_at DESC);
create index jobs_newest_idx on jobs (created_at DESC);
create index mail_events_newest_idx on mail_events (created_at DESC);
create index media_checksum_idx on media (checksum);
create index media_newest_idx on media (created_at DESC) where (deleted_at is null);
create index orders_newest_idx on orders (created_at DESC);
create index outbox_pending_idx on outbox (created_at) where (state = 'pending'::outbox_state);
create index page_issues_weight_idx on page_issues (weight);
create index page_views_day_idx on page_views (on_day DESC);
create index payments_newest_idx on payments (created_at DESC);
create unique index posts_address_idx on posts (language, slug) where (deleted_at is null);
create index posts_feed_idx on posts (language, published_at DESC) where ((state = 'published'::post_state) and (deleted_at is null));
create unique index posts_one_per_language on posts (COALESCE(translation_of, id), language) where (deleted_at is null);
create index posts_newest_idx on posts (created_at DESC);
create index posts_type_key_idx on posts (type_key) where (deleted_at is null);
create unique index products_slug_idx on products (slug) where (deleted_at is null);
create index products_newest_idx on products (created_at DESC) where (deleted_at is null);
create index publishes_preview_idx on publishes (created_at DESC) where preview;
create index publishes_newest_idx on publishes (created_at DESC);
create index reports_newest_idx on reports (created_at DESC);
create index sessions_user_idx on sessions (user_id);
create index student_sessions_student_idx on student_sessions (student_id);
create index subscribers_state_idx on subscribers (state);
create index terms_kind_idx on terms (kind, language);
create index theme_files_branch_idx on theme_files (branch) where (deleted_at is null);
create unique index theme_files_path_idx on theme_files (branch, path) where (deleted_at is null);
create index tickets_user_idx on tickets (user_id, purpose);
create index transfers_newest_idx on transfers (created_at DESC);
create index users_role_idx on users (role_id);
create unique index videos_reference_idx on videos (reference) where (reference is not null);
create index videos_newest_idx on videos (created_at DESC) where (deleted_at is null);
create index vitals_day_idx on vitals (on_day DESC, kind);
create index webhook_deliveries_newest_idx on webhook_deliveries (sent_at DESC);

-- Every uniqueness that was a site's own, flattened.
alter table board_stages add unique (board_id, "position");
alter table content_types add unique (key);
alter table coupons add unique (code);
alter table domain_checks add unique (host);
alter table enrolments add unique (student_id, course_id);
alter table flow_credentials add unique (name);
alter table flow_steps add unique (flow_id, "position");
alter table languages add unique (code);
alter table lessons add unique (module_id, "position");
alter table letters add unique (kind, language);
alter table mail_events add unique (provider_ref);
alter table media add unique (location);
alter table modules add unique (course_id, "position");
alter table oauth_attempts add unique (state_hash);
alter table oauth_providers add unique (key);
alter table orders add unique (number);
alter table orders add unique (idempotency_key);
alter table page_issues add unique (post_id, kind);
alter table payments add unique (provider, provider_ref);
alter table plugins add unique (key);
alter table recovery_codes add unique (code_hash);
alter table redirects add unique (language, was);
alter table roles add unique (key);
alter table second_factors add unique (user_id);
alter table students add unique (email);
alter table subscribers add unique (email);
alter table terms add unique (kind, language, slug);
alter table users add unique (email);

-- And the composite foreign key, now that there is a `content_types (key)` to
-- point at. Still `on delete set null`: taking a kind of thing away leaves
-- what was written under it, which is what the test beside it asks.
alter table posts
    add constraint posts_type_is_one_this_site_declared
    foreign key (type_key) references content_types (key) on delete set null;
