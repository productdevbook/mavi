-- Hatchet is the durable execution authority from this migration onward.
-- Preserve completed/dead compatibility history, but do not silently upgrade
-- a database while the old lease queue still has work that could be claimed by
-- both systems. The operator can export/replay those rows explicitly before
-- retrying the Mavi upgrade.
do $$
begin
    if to_regclass('public.jobs') is not null
       and exists (select 1 from jobs where state in ('ready', 'running')) then
        raise exception using
            message = 'mavi_legacy_jobs_active: ready/running jobs exist; drain or export them before upgrading to Hatchet-backed workflows';
    end if;
end;
$$;

comment on table jobs is
    'Legacy compatibility history only; new durable execution is owned by workflow_outbox and Hatchet.';
