alter table site_settings
    add column canonical_url text;

alter table site_settings
    add constraint site_settings_canonical_url_length
    check (canonical_url is null or char_length(canonical_url) between 8 and 2048);
