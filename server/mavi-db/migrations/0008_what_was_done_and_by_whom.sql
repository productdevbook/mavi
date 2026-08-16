-- What was done, and by whom.
--
-- One row per change, written in the same transaction as the change itself.
-- Afterwards would be a row a crash between the two loses — and what it loses
-- is the record of the thing that did happen.

create table receipts (
    id         uuid primary key,
    who        text not null check (who in ('an_account', 'a_student', 'the_machine')),
    -- Their id, where there is one. Text rather than a reference: the account
    -- that did this may be gone, and a receipt that disappears with whoever it
    -- was about is not a record of anything.
    who_id     text,
    -- The endpoint's own name — `writings.publish` — rather than a verb chosen
    -- at the call site. Two names for one action is two answers to "what
    -- happened to this".
    did        text not null,
    about      text not null,
    about_id   text,
    -- Whatever somebody reading this in a year needs in order to understand it
    -- without the row it describes, which may since have been deleted and
    -- often has been.
    what       jsonb not null default '{}'::jsonb,
    -- What ties one request's rows together, and ties them to whatever the
    -- logs say about the same moment.
    request    text not null,
    created_at timestamptz not null default now(),

    -- The machine has no id and everybody else has one. Said here because a
    -- receipt attributed to nobody in particular is one nobody can follow up.
    constraint somebody_or_the_machine
        check ((who = 'the_machine') = (who_id is null))
);

-- What the panel reads, and the keyset it is ordered by, column for column.
create index receipts_recent on receipts (created_at desc, id desc);

-- "What has happened to this thing" — the question somebody actually asks,
-- and a scan of everything ever done without this.
create index receipts_about on receipts (about, about_id, created_at desc);

-- A record somebody can write into is not a record.
--
-- The rule is here rather than in the code because the code is not the only
-- thing that reaches this table: a migration, a console, a script somebody
-- writes at two in the morning. What may happen to a receipt is that it is
-- written once and read afterwards, and Postgres is where that can be said to
-- all of them at once.
create function a_receipt_is_written_once() returns trigger
language plpgsql as $$
begin
    raise exception 'a receipt is written once and not %', lower(tg_op);
end;
$$;

create trigger receipts_are_not_rewritten
    before update or delete on receipts
    for each row execute function a_receipt_is_written_once();
