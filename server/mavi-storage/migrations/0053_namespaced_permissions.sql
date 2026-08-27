-- The capability/action pair remains a compatibility projection for old
-- clients, but Cedar and new application code need one namespaced permission
-- value that cannot be interpreted differently by each transport.
alter table role_grants add column if not exists permission text;
alter table api_key_grants add column if not exists permission text;

update role_grants
   set permission = case
       when capability = 'audit' and action = 'view' then 'governance.audit.view'
       when capability = 'audit' then 'governance.audit.manage'
       when capability = 'analytics' and action = 'view' then 'analytics.view'
       when capability = 'analytics' then 'analytics.manage'
       when capability = 'automation' and action = 'view' then 'automation.flow.view'
       when capability = 'automation' and action = 'write' then 'automation.flow.manage'
       when capability = 'automation' then 'automation.flow.delete'
       when capability = 'boards' and action = 'view' then 'boards.board.view'
       when capability = 'boards' and action = 'write' then 'boards.board.manage'
       when capability = 'boards' then 'boards.board.delete'
       when capability = 'content' and action = 'view' then 'writing.content.entry.list'
       when capability = 'content' and action = 'write' then 'writing.content.entry.update'
       when capability = 'content' then 'writing.content.entry.delete'
       when capability = 'courses' and action = 'view' then 'learning.courses.course.view'
       when capability = 'courses' and action = 'write' then 'learning.courses.course.manage'
       when capability = 'courses' then 'learning.courses.course.delete'
       when capability = 'credentials' and action = 'view' then 'core.credentials.list'
       when capability = 'credentials' and action = 'write' then 'core.credentials.manage'
       when capability = 'credentials' then 'core.credentials.revoke'
       when capability = 'design' and action = 'view' then 'writing.design.change.view'
       when capability = 'design' and action = 'write' then 'writing.design.change.manage'
       when capability = 'design' then 'writing.design.change.delete'
       when capability = 'feedback' and action = 'view' then 'core.feedback.view'
       when capability = 'feedback' and action = 'write' then 'core.feedback.submit'
       when capability = 'feedback' then 'core.feedback.delete'
       when capability = 'forms' and action = 'view' then 'forms.submission.view'
       when capability = 'forms' and action = 'write' then 'forms.form.manage'
       when capability = 'forms' then 'forms.form.delete'
       when capability = 'mail' and action = 'view' then 'messaging.delivery.view'
       when capability = 'mail' and action = 'write' then 'messaging.delivery.manage'
       when capability = 'mail' then 'messaging.delivery.delete'
       when capability = 'media' and action = 'view' then 'writing.media.file.list'
       when capability = 'media' and action = 'write' then 'writing.media.file.manage'
       when capability = 'media' then 'writing.media.file.delete'
       when capability = 'people' and action = 'view' then 'core.people.list'
       when capability = 'people' and action = 'write' then 'core.people.update'
       when capability = 'people' then 'core.people.delete'
       when capability = 'portable' and action = 'view' then 'governance.portable.export'
       when capability = 'portable' and action = 'write' then 'governance.portable.import'
       when capability = 'portable' then 'governance.portable.delete'
       when capability = 'publish' and action = 'write' then 'writing.content.entry.publish'
       when capability = 'publish' and action = 'delete' then 'writing.content.entry.delete'
       when capability = 'publish' then 'writing.content.entry.list'
       when capability = 'settings' and action = 'view' then 'core.settings.view'
       when capability = 'settings' and action = 'write' then 'core.settings.manage'
       when capability = 'settings' then 'core.settings.delete'
       when capability = 'shop' and action = 'view' then 'commerce.shop.product.view'
       when capability = 'shop' and action = 'write' then 'commerce.shop.product.manage'
       when capability = 'shop' then 'commerce.shop.product.delete'
       when capability = 'taxonomy' and action = 'view' then 'writing.taxonomy.term.view'
       when capability = 'taxonomy' and action = 'write' then 'writing.taxonomy.term.manage'
       when capability = 'taxonomy' then 'writing.taxonomy.term.delete'
       when capability = 'trash' and action = 'view' then 'governance.trash.view'
       when capability = 'trash' and action = 'write' then 'governance.trash.manage'
       when capability = 'trash' then 'governance.trash.delete'
       else null
   end;

update api_key_grants
   set permission = case
       when capability = 'audit' and action = 'view' then 'governance.audit.view'
       when capability = 'audit' then 'governance.audit.manage'
       when capability = 'analytics' and action = 'view' then 'analytics.view'
       when capability = 'analytics' then 'analytics.manage'
       when capability = 'automation' and action = 'view' then 'automation.flow.view'
       when capability = 'automation' and action = 'write' then 'automation.flow.manage'
       when capability = 'automation' then 'automation.flow.delete'
       when capability = 'boards' and action = 'view' then 'boards.board.view'
       when capability = 'boards' and action = 'write' then 'boards.board.manage'
       when capability = 'boards' then 'boards.board.delete'
       when capability = 'content' and action = 'view' then 'writing.content.entry.list'
       when capability = 'content' and action = 'write' then 'writing.content.entry.update'
       when capability = 'content' then 'writing.content.entry.delete'
       when capability = 'courses' and action = 'view' then 'learning.courses.course.view'
       when capability = 'courses' and action = 'write' then 'learning.courses.course.manage'
       when capability = 'courses' then 'learning.courses.course.delete'
       when capability = 'credentials' and action = 'view' then 'core.credentials.list'
       when capability = 'credentials' and action = 'write' then 'core.credentials.manage'
       when capability = 'credentials' then 'core.credentials.revoke'
       when capability = 'design' and action = 'view' then 'writing.design.change.view'
       when capability = 'design' and action = 'write' then 'writing.design.change.manage'
       when capability = 'design' then 'writing.design.change.delete'
       when capability = 'feedback' and action = 'view' then 'core.feedback.view'
       when capability = 'feedback' and action = 'write' then 'core.feedback.submit'
       when capability = 'feedback' then 'core.feedback.delete'
       when capability = 'forms' and action = 'view' then 'forms.submission.view'
       when capability = 'forms' and action = 'write' then 'forms.form.manage'
       when capability = 'forms' then 'forms.form.delete'
       when capability = 'mail' and action = 'view' then 'messaging.delivery.view'
       when capability = 'mail' and action = 'write' then 'messaging.delivery.manage'
       when capability = 'mail' then 'messaging.delivery.delete'
       when capability = 'media' and action = 'view' then 'writing.media.file.list'
       when capability = 'media' and action = 'write' then 'writing.media.file.manage'
       when capability = 'media' then 'writing.media.file.delete'
       when capability = 'people' and action = 'view' then 'core.people.list'
       when capability = 'people' and action = 'write' then 'core.people.update'
       when capability = 'people' then 'core.people.delete'
       when capability = 'portable' and action = 'view' then 'governance.portable.export'
       when capability = 'portable' and action = 'write' then 'governance.portable.import'
       when capability = 'portable' then 'governance.portable.delete'
       when capability = 'publish' and action = 'write' then 'writing.content.entry.publish'
       when capability = 'publish' and action = 'delete' then 'writing.content.entry.delete'
       when capability = 'publish' then 'writing.content.entry.list'
       when capability = 'settings' and action = 'view' then 'core.settings.view'
       when capability = 'settings' and action = 'write' then 'core.settings.manage'
       when capability = 'settings' then 'core.settings.delete'
       when capability = 'shop' and action = 'view' then 'commerce.shop.product.view'
       when capability = 'shop' and action = 'write' then 'commerce.shop.product.manage'
       when capability = 'shop' then 'commerce.shop.product.delete'
       when capability = 'taxonomy' and action = 'view' then 'writing.taxonomy.term.view'
       when capability = 'taxonomy' and action = 'write' then 'writing.taxonomy.term.manage'
       when capability = 'taxonomy' then 'writing.taxonomy.term.delete'
       when capability = 'trash' and action = 'view' then 'governance.trash.view'
       when capability = 'trash' and action = 'write' then 'governance.trash.manage'
       when capability = 'trash' then 'governance.trash.delete'
       else null
   end;

do $$
begin
    if exists (select 1 from role_grants where permission is null)
       or exists (select 1 from api_key_grants where permission is null) then
        raise exception 'namespaced permission backfill failed: unknown capability/action';
    end if;
end $$;

alter table role_grants alter column permission set not null;
alter table api_key_grants alter column permission set not null;

alter table role_grants
    add constraint role_grants_permission_check
    check (char_length(permission) between 3 and 160 and permission ~ '^[a-z][a-z0-9_-]*(\.[a-z][a-z0-9_-]*)+$');
alter table api_key_grants
    add constraint api_key_grants_permission_check
    check (char_length(permission) between 3 and 160 and permission ~ '^[a-z][a-z0-9_-]*(\.[a-z][a-z0-9_-]*)+$');

create index role_grants_site_role_permission
    on role_grants (site_id, role_id, permission);
create index api_key_grants_site_key_permission
    on api_key_grants (site_id, key_id, permission);
