create function reject_audit_mutation() returns trigger
language plpgsql
as $$
begin
    raise exception 'audit_events is append-only' using errcode = '42501';
end;
$$;

create trigger audit_events_append_only
    before update or delete on audit_events
    for each row execute function reject_audit_mutation();

revoke update, delete on audit_events from public;
