-- Canonical permissions may be site-wide or restricted to a resource type.
-- Keep the old capability/action columns as a compatibility projection, but
-- make the namespaced permission and its scope the actual identity of a row.
alter table role_grants
    add column if not exists resource_type text not null default '*';
alter table api_key_grants
    add column if not exists resource_type text not null default '*';

alter table role_grants
    drop constraint if exists role_grants_resource_type_check;
alter table role_grants
    add constraint role_grants_resource_type_check
    check (resource_type = '*' or resource_type ~ '^[A-Za-z][A-Za-z0-9_-]{0,119}$');

alter table api_key_grants
    drop constraint if exists api_key_grants_resource_type_check;
alter table api_key_grants
    add constraint api_key_grants_resource_type_check
    check (resource_type = '*' or resource_type ~ '^[A-Za-z][A-Za-z0-9_-]{0,119}$');

-- Several historical capability pairs intentionally collapsed into the same
-- business action (for example content:view and publish:view). Remove only
-- those duplicate projections before the canonical key is installed; no
-- distinct namespaced permission is lost.
-- The owner role has a protection trigger because normal application writes
-- must never remove its grants. This is a one-time canonicalization of
-- duplicate projections, so suspend only that trigger for the migration
-- transaction and restore it immediately afterwards.
alter table role_grants no force row level security;
alter table api_key_grants no force row level security;
alter table role_grants disable trigger role_grants_system_role_protected;

delete from role_grants older
 using role_grants newer
 where older.ctid < newer.ctid
   and older.site_id = newer.site_id
   and older.role_id = newer.role_id
   and older.permission = newer.permission
   and older.resource_type = newer.resource_type;

alter table role_grants enable trigger role_grants_system_role_protected;
alter table role_grants force row level security;

delete from api_key_grants older
 using api_key_grants newer
 where older.ctid < newer.ctid
   and older.site_id = newer.site_id
   and older.key_id = newer.key_id
   and older.permission = newer.permission
   and older.resource_type = newer.resource_type;

alter table api_key_grants force row level security;

alter table role_grants drop constraint if exists role_grants_pkey;
alter table role_grants
    add primary key (site_id, role_id, permission, resource_type);

alter table api_key_grants drop constraint if exists api_key_grants_pkey;
alter table api_key_grants
    add primary key (site_id, key_id, permission, resource_type);

create index role_grants_site_role_resource_permission
    on role_grants (site_id, role_id, resource_type, permission);
create index api_key_grants_site_key_resource_permission
    on api_key_grants (site_id, key_id, resource_type, permission);
