alter table role_grants drop constraint role_grants_capability_check;
alter table role_grants add constraint role_grants_capability_check check (
    capability in (
        'audit', 'analytics', 'automation', 'boards', 'content', 'courses',
        'design', 'forms', 'mail', 'media', 'people', 'portable', 'publish',
        'settings', 'shop', 'taxonomy', 'trash'
    )
);

alter table api_key_grants drop constraint api_key_grants_capability_check;
alter table api_key_grants add constraint api_key_grants_capability_check check (
    capability in (
        'audit', 'analytics', 'automation', 'boards', 'content', 'courses',
        'design', 'forms', 'mail', 'media', 'people', 'portable', 'publish',
        'settings', 'shop', 'taxonomy', 'trash'
    )
);
