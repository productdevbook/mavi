-- A name belongs to what is there, not to what was thrown away.
--
-- Uniqueness that counts deleted rows means a post in the trash holds its
-- address forever: nothing else can answer on it, and nobody can see why. The
-- constraints become partial indexes over what has not been deleted, and
-- putting something back whose name has since been taken is the conflict the
-- restore already answers.

alter table posts drop constraint posts_tenant_id_language_slug_key;
create unique index posts_address_idx on posts (tenant_id, language, slug)
    where deleted_at is null;

alter table forms drop constraint forms_tenant_id_slug_key;
create unique index forms_slug_idx on forms (tenant_id, slug)
    where deleted_at is null;

alter table products drop constraint products_tenant_id_slug_key;
create unique index products_slug_idx on products (tenant_id, slug)
    where deleted_at is null;

alter table courses drop constraint courses_tenant_id_slug_key;
create unique index courses_slug_idx on courses (tenant_id, slug)
    where deleted_at is null;

alter table theme_files drop constraint theme_files_tenant_id_branch_path_key;
create unique index theme_files_path_idx on theme_files (tenant_id, branch, path)
    where deleted_at is null;
