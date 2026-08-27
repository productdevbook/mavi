-- Canonical permissions are the identity of a grant. Some business actions
-- have no safe capability/action equivalent (for example sessions.revoke and
-- writing.site.publish), so the old pair becomes an optional compatibility
-- projection instead of preventing those permissions from being assigned.

alter table role_grants alter column capability drop not null;
alter table role_grants alter column action drop not null;
alter table api_key_grants alter column capability drop not null;
alter table api_key_grants alter column action drop not null;

alter table role_grants drop constraint if exists role_grants_capability_check;
alter table role_grants add constraint role_grants_capability_check check (
    capability is null or capability in (
        'audit', 'analytics', 'automation', 'boards', 'content', 'courses',
        'credentials', 'design', 'feedback', 'forms', 'mail', 'media', 'people',
        'portable', 'publish', 'settings', 'shop', 'taxonomy', 'trash'
    )
);
alter table api_key_grants drop constraint if exists api_key_grants_capability_check;
alter table api_key_grants add constraint api_key_grants_capability_check check (
    capability is null or capability in (
        'audit', 'analytics', 'automation', 'boards', 'content', 'courses',
        'credentials', 'design', 'feedback', 'forms', 'mail', 'media', 'people',
        'portable', 'publish', 'settings', 'shop', 'taxonomy', 'trash'
    )
);

alter table role_grants drop constraint if exists role_grants_action_check;
alter table role_grants add constraint role_grants_action_check check (
    action is null or action in ('view', 'write', 'delete')
);
alter table api_key_grants drop constraint if exists api_key_grants_action_check;
alter table api_key_grants add constraint api_key_grants_action_check check (
    action is null or action in ('view', 'write', 'delete')
);

alter table role_grants add constraint role_grants_legacy_projection_check check (
    (capability is null) = (action is null)
);
alter table api_key_grants add constraint api_key_grants_legacy_projection_check check (
    (capability is null) = (action is null)
);
