-- Sender identity is site configuration, while the queued delivery keeps a
-- snapshot so changing settings never mutates an already accepted message.
alter table site_settings
    add column mail_sender_address text,
    add column mail_sender_name text;

alter table site_settings
    add constraint site_settings_mail_sender_address_check
    check (
        mail_sender_address is null
        or (
            mail_sender_address = lower(mail_sender_address)
            and char_length(mail_sender_address) between 6 and 320
            and position('@' in mail_sender_address) > 1
        )
    ),
    add constraint site_settings_mail_sender_name_check
    check (mail_sender_name is null or char_length(mail_sender_name) between 1 and 200);

alter table mail_deliveries
    add column sender_address text,
    add column sender_name text;

alter table mail_deliveries
    add constraint mail_deliveries_sender_address_check
    check (
        sender_address is null
        or (
            sender_address = lower(sender_address)
            and char_length(sender_address) between 6 and 320
            and position('@' in sender_address) > 1
        )
    ),
    add constraint mail_deliveries_sender_name_check
    check (sender_name is null or char_length(sender_name) between 1 and 200);
