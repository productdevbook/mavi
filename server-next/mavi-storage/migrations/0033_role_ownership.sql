-- The owner role is created by setup, not by a normal role mutation. Keep its
-- identity and grants protected in the database as a last line of defense for
-- repositories and maintenance SQL that bypass the application service.
alter table roles
    add column system_role boolean not null default false;

update roles
   set system_role = true
 where name = 'owner';

alter table roles
    add constraint roles_system_name_check
    check (not system_role or name = 'owner');

create function protect_system_role() returns trigger
language plpgsql
as $$
begin
    if tg_op = 'DELETE' then
        if old.system_role then
            raise exception 'system role is protected'
                using errcode = 'restrict_violation',
                      constraint = 'roles_system_role_protected';
        end if;
        return old;
    end if;

    if old.system_role and (
        new.name is distinct from old.name
        or new.system_role is distinct from old.system_role
    ) then
        raise exception 'system role is protected'
            using errcode = 'restrict_violation',
                  constraint = 'roles_system_role_protected';
    end if;
    return new;
end;
$$;

create trigger roles_system_role_protected
before update or delete on roles
for each row execute function protect_system_role();

create function protect_system_role_grants() returns trigger
language plpgsql
as $$
begin
    if exists (
        select 1
          from roles
         where site_id = old.site_id
           and id = old.role_id
           and system_role
    ) then
        raise exception 'system role grants are protected'
            using errcode = 'restrict_violation',
                  constraint = 'role_grants_system_role_protected';
    end if;
    if tg_op = 'DELETE' then
        return old;
    end if;
    return new;
end;
$$;

create trigger role_grants_system_role_protected
before update or delete on role_grants
for each row execute function protect_system_role_grants();
