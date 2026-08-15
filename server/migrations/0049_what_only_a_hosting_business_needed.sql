-- The first of three that take the tenancy out of the database. This one takes
-- away the tables that were never a CMS's: what a site used, what it owed, what
-- it had been sold, and who to invoice about it.
--
-- Read the three together before running any of them. There is no way back
-- from them and no upgrade path across them: after 0050 the knowledge of which
-- row belonged to which site has left the database, so a database holding more
-- than one site has to be taken apart *before* this runs, not after. That is
-- what the first statement below is for.

-- A database this cannot honestly convert.
--
-- Everything after this point merges every site's rows into one site's, and
-- nothing afterwards can tell them apart again — the column that said so is
-- what leaves. On a machine that only ever had one site this is a no-op. On one
-- that had several it stops the migration dead, with the count, rather than
-- silently handing every site's posts to whoever signs in first.
--
-- Getting past it means exporting each site through `/api/portable/export` on
-- the version *before* this one and reading each bundle into an installation of
-- its own. There is no tool here that will do it afterwards.
do $$
declare sites bigint;
begin
    select count(*) into sites from tenants;

    if sites > 1 then
        raise exception
            'this database holds % sites and this version serves one; '
            'export each site before upgrading — after this migration '
            'nothing can tell their rows apart', sites;
    end if;
end $$;

-- What a site owed. `ledger` is the account behind the usage: it names an
-- operator as who adjusted a line and a charge as what a line settled, which is
-- one business's record of another. A CMS somebody installs has no such
-- relationship to itself.
drop table ledger;
drop type ledger_kind;

-- What a site was billed. One row per site per month, settled from the events
-- below it.
drop table charges;
drop type charge_state;

-- What a site used, written down as it happened so that a site removed halfway
-- through a month still had half a month of readings to bill for. Nobody bills
-- an installation for its own disk.
drop table usage_events;
drop type usage_kind;

-- Three columns on `site_settings` that were one machine's record of its
-- customer rather than anything the site is:
--
--   `storage_limit_bytes` — what a site had been *sold*, null meaning fall back
--     to the machine's own default. The ceiling itself is not going anywhere:
--     an installation with no limit on what it will store is a disk that fills
--     up one legal upload at a time, and that was the reason this column was
--     added. What goes is the per-site override, because selling one site more
--     room than another is a hosting business. The ceiling stays where it
--     already was for every site that had no override — one number in the
--     application — and the column that let somebody raise it for a paying
--     customer goes.
--   `contact` and `notes` — who to talk to about this site, held deliberately
--     where the site's own people could not read it. With one installation the
--     site's own people are the only people.
alter table site_settings
    drop column storage_limit_bytes,
    drop column contact,
    drop column notes;

-- `seed` is a site being made from a bundle by something provisioning it.
-- `import` and `export` stay: carrying content in and out is a CMS feature and
-- is reached from the panel. Rewritten rather than left with a value nothing
-- can produce, because an enum's values are the documentation of what the
-- column means.
alter table transfers alter column kind type text;
drop type transfer_kind;
create type transfer_kind as enum ('import', 'export');
alter table transfers alter column kind type transfer_kind using kind::transfer_kind;
