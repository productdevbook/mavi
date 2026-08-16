-- How much of the machine one site may take.
--
-- There was a limit on a single file and none on the total, so a site could
-- fill the disk one legal upload at a time — and a full disk on this machine is
-- the kubelet evicting Postgres, which is every site rather than one.
alter table site_settings
    add column storage_limit_bytes bigint
        check (storage_limit_bytes is null or storage_limit_bytes > 0);

comment on column site_settings.storage_limit_bytes is
    'Null means the machine''s own default. Set per site by the operator, for a
     site that has been sold more.';
