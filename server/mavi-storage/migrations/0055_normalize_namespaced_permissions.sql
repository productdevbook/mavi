-- 0053 introduced the canonical permission column. This migration makes the
-- canonical storage key
-- value idempotently canonical for databases that passed through an earlier
-- preview, including the preview that wrote a second `governance.` prefix.
-- The old capability/action columns remain only as a compatibility projection.

alter table role_grants no force row level security;
alter table api_key_grants no force row level security;
alter table role_grants disable trigger role_grants_system_role_protected;

-- Drop the canonical primary keys while normalizing. A few historical
-- capability/action pairs collapse to one business permission, so the
-- duplicate cleanup below must run before the keys are restored.
alter table role_grants drop constraint if exists role_grants_pkey;
alter table api_key_grants drop constraint if exists api_key_grants_pkey;

update role_grants
   set permission = case capability
       when 'audit' then case
           when permission like 'governance.governance.%' then substring(permission from char_length('governance.') + 1)
           when permission like 'governance.%' then permission
           else 'governance.' || permission
       end
       when 'analytics' then case
           when permission like 'analytics.analytics.%' then substring(permission from char_length('analytics.') + 1)
           when permission like 'analytics.%' then permission
           else 'analytics.' || permission
       end
       when 'automation' then case
           when permission like 'automation.automation.%' then substring(permission from char_length('automation.') + 1)
           when permission like 'automation.%' then permission
           else 'automation.' || permission
       end
       when 'boards' then case
           when permission like 'boards.boards.%' then substring(permission from char_length('boards.') + 1)
           when permission like 'boards.%' then permission
           else 'boards.' || permission
       end
       when 'courses' then case when permission like 'learning.%' then permission else 'learning.' || permission end
       when 'forms' then case
           when permission like 'forms.forms.%' then substring(permission from char_length('forms.') + 1)
           when permission like 'forms.%' then permission
           else 'forms.' || permission
       end
       when 'mail' then case
           when permission like 'messaging.messaging.%' then substring(permission from char_length('messaging.') + 1)
           when permission like 'messaging.%' then permission
           else 'messaging.' || permission
       end
       when 'portable' then case
           when permission like 'governance.governance.%' then substring(permission from char_length('governance.') + 1)
           when permission like 'governance.%' then permission
           else 'governance.' || permission
       end
       when 'shop' then case when permission like 'commerce.%' then permission else 'commerce.' || permission end
       when 'trash' then case
           when permission like 'governance.governance.%' then substring(permission from char_length('governance.') + 1)
           when permission like 'governance.%' then permission
           else 'governance.' || permission
       end
       else permission
   end
 where capability in ('audit', 'analytics', 'automation', 'boards', 'courses', 'forms', 'mail', 'portable', 'shop', 'trash');

update api_key_grants
   set permission = case capability
       when 'audit' then case
           when permission like 'governance.governance.%' then substring(permission from char_length('governance.') + 1)
           when permission like 'governance.%' then permission
           else 'governance.' || permission
       end
       when 'analytics' then case
           when permission like 'analytics.analytics.%' then substring(permission from char_length('analytics.') + 1)
           when permission like 'analytics.%' then permission
           else 'analytics.' || permission
       end
       when 'automation' then case
           when permission like 'automation.automation.%' then substring(permission from char_length('automation.') + 1)
           when permission like 'automation.%' then permission
           else 'automation.' || permission
       end
       when 'boards' then case
           when permission like 'boards.boards.%' then substring(permission from char_length('boards.') + 1)
           when permission like 'boards.%' then permission
           else 'boards.' || permission
       end
       when 'courses' then case when permission like 'learning.%' then permission else 'learning.' || permission end
       when 'forms' then case
           when permission like 'forms.forms.%' then substring(permission from char_length('forms.') + 1)
           when permission like 'forms.%' then permission
           else 'forms.' || permission
       end
       when 'mail' then case
           when permission like 'messaging.messaging.%' then substring(permission from char_length('messaging.') + 1)
           when permission like 'messaging.%' then permission
           else 'messaging.' || permission
       end
       when 'portable' then case
           when permission like 'governance.governance.%' then substring(permission from char_length('governance.') + 1)
           when permission like 'governance.%' then permission
           else 'governance.' || permission
       end
       when 'shop' then case when permission like 'commerce.%' then permission else 'commerce.' || permission end
       when 'trash' then case
           when permission like 'governance.governance.%' then substring(permission from char_length('governance.') + 1)
           when permission like 'governance.%' then permission
           else 'governance.' || permission
       end
       else permission
   end
 where capability in ('audit', 'analytics', 'automation', 'boards', 'courses', 'forms', 'mail', 'portable', 'shop', 'trash');

delete from role_grants older
 using role_grants newer
 where older.ctid < newer.ctid
   and older.site_id = newer.site_id
   and older.role_id = newer.role_id
   and older.permission = newer.permission
   and older.resource_type = newer.resource_type;

delete from api_key_grants older
 using api_key_grants newer
 where older.ctid < newer.ctid
   and older.site_id = newer.site_id
   and older.key_id = newer.key_id
   and older.permission = newer.permission
   and older.resource_type = newer.resource_type;

alter table role_grants
    add primary key (site_id, role_id, permission, resource_type);
alter table api_key_grants
    add primary key (site_id, key_id, permission, resource_type);

alter table role_grants enable trigger role_grants_system_role_protected;
alter table role_grants force row level security;
alter table api_key_grants force row level security;

do $$
begin
    if exists (
        select 1
          from (
              select permission from role_grants
              union all
              select permission from api_key_grants
          ) as grants
         where permission is null
            or not (permission = any (array[
                'core.plugins.list', 'core.plugins.activate', 'core.plugins.deactivate',
                'core.people.list', 'core.people.view', 'core.people.create',
                'core.people.update', 'core.people.delete', 'core.roles.list',
                'core.roles.manage', 'core.sessions.revoke', 'core.credentials.list',
                'core.credentials.manage', 'core.credentials.revoke', 'core.settings.view',
                'core.settings.manage', 'core.settings.delete', 'core.feedback.view',
                'core.feedback.submit', 'core.feedback.delete',
                'writing.content.entry.list', 'writing.content.entry.create',
                'writing.content.entry.update', 'writing.content.entry.delete',
                'writing.content.entry.publish', 'writing.content.type.manage',
                'writing.taxonomy.term.manage', 'writing.taxonomy.term.view',
                'writing.taxonomy.term.delete', 'writing.media.file.list',
                'writing.media.file.manage', 'writing.media.file.delete',
                'writing.design.change.view', 'writing.design.change.manage',
                'writing.design.change.delete', 'writing.design.build',
                'writing.design.publish', 'writing.site.view', 'writing.site.publish',
                'commerce.shop.product.view', 'commerce.shop.product.manage',
                'commerce.shop.product.delete', 'commerce.shop.order.view',
                'commerce.shop.order.manage', 'commerce.shop.order.fulfill',
                'commerce.shop.coupon.manage', 'learning.courses.course.view',
                'learning.courses.course.manage', 'learning.courses.course.delete',
                'learning.courses.module.manage', 'learning.courses.lesson.view',
                'learning.courses.lesson.manage', 'learning.courses.student.view',
                'learning.courses.student.manage', 'learning.courses.enrollment.manage',
                'learning.courses.progress.view', 'forms.form.manage',
                'forms.form.delete', 'forms.submission.view',
                'forms.submission.manage', 'messaging.template.manage',
                'messaging.list.manage', 'messaging.delivery.view',
                'messaging.delivery.manage', 'messaging.delivery.delete',
                'automation.flow.view', 'automation.flow.manage',
                'automation.flow.delete', 'automation.flow.start',
                'automation.flow.run.view', 'automation.workflow.view',
                'automation.workflow.control', 'boards.board.view',
                'boards.board.manage', 'boards.board.delete',
                'boards.card.manage', 'analytics.view',
                'analytics.manage', 'governance.audit.view',
                'governance.audit.manage', 'governance.trash.view',
                'governance.trash.manage', 'governance.trash.delete',
                'governance.portable.export', 'governance.portable.import',
                'governance.portable.delete'
            ]::text[]))
    ) then
        raise exception 'namespaced permission normalization failed';
    end if;
end $$;
