-- A fourth thing a ticket can be for.
--
-- Signing in with a password, when the account has a second step, answers with
-- one of these instead of a session. Whoever holds it has given a right
-- password and nothing more: it is not a way in, and it stops being anything
-- at all within minutes.
--
-- The same table as every other link, so it is minted, redeemed and expired by
-- the same code — a second mechanism for a short-lived token is a second place
-- for "has this been used" to be got wrong.
alter table tickets drop constraint tickets_what_for_check;

alter table tickets add constraint tickets_what_for_check
    check (what_for in (
        'invitation',
        'forgotten_password',
        'address_to_prove',
        'a_moment_to_finish'
    ));
