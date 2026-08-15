-- A discount could be given away without conditions.
--
-- A code meant for a first order could be used ten times by the same person,
-- and a code meant to lift a small basket took the same amount off a basket of
-- one thing. Both were in the panel before this build and neither was here.

alter table coupons
    add column minimum_minor bigint not null default 0
        check (minimum_minor >= 0),
    add column per_shopper integer
        check (per_shopper is null or per_shopper > 0),
    -- What an amount off is an amount of. A percentage is a percentage of
    -- whatever the basket is in, so this is only read for the other kind.
    add column currency currency not null default 'TRY';

comment on column coupons.minimum_minor is
    'What the basket has to reach before this may be used. Zero is no minimum.';
comment on column coupons.per_shopper is
    'How many times one address may use it. Null is as often as they like.';
