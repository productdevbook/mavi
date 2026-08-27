-- Hatchet is the only durable execution authority. Once the migration guard
-- has proven that no legacy lease can still be claimed, remove the old queue
-- rather than leaving a second table that future code could accidentally use.
do $$
begin
    if to_regclass('public.jobs') is not null then
        if exists (
            select 1 from jobs where state in ('ready', 'running')
        ) then
            raise exception using
                message = 'mavi_legacy_jobs_active: ready/running jobs exist; drain or export them before decommissioning the legacy jobs table';
        end if;
        drop table jobs;
    end if;
end;
$$;
