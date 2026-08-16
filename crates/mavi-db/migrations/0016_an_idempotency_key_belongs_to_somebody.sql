-- What "the same request twice" is scoped to.
--
-- `said_once` was unique on its own. It is chosen by whoever is placing the
-- order, and placing an order is open to anybody — so a string that somebody
-- else happened to pick was a way to read their order back: the address they
-- typed, what they bought, and what they paid.
--
-- It is scoped to the address the order is for instead. Repeating a request
-- still answers with the same order, which is what it is for; guessing a
-- stranger's key no longer answers with anything, because the address has to
-- match as well.
--
-- Two people picking the same key are now two orders rather than one refusal.

alter table orders drop constraint orders_said_once_key;

create unique index orders_said_once on orders (said_once, email);
