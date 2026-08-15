-- What a shop sells, and what happened to somebody's money.
--
-- Money is minor units and a currency, in two columns, never a float and never
-- a number on its own: a column holding 1250 says nothing until another says
-- twelve fifty of what.

create table products (
    id           uuid primary key,
    slug         text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,126}[a-z0-9])?$'),
    name         text not null check (length(name) between 1 and 300),
    about        text,
    price_minor  bigint not null check (price_minor >= 0),
    currency     text not null check (currency ~ '^[A-Z]{3}$'),
    -- What is on the shelf, after anything held for a checkout in flight. The
    -- check is what makes a race a failed transaction rather than a shop that
    -- owes somebody something.
    on_the_shelf integer not null default 0 check (on_the_shelf >= 0),
    for_sale     boolean not null default true,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    deleted_at   timestamptz
);

create unique index products_address on products (slug) where deleted_at is null;

create index products_recent
    on products (created_at desc, id desc)
    where deleted_at is null;

create table orders (
    id           uuid primary key,
    -- Counted per site and never reused. What somebody says on the telephone,
    -- because nobody reads a uuid down a telephone.
    number       bigint not null unique generated always as identity,
    state        text not null default 'waiting'
        check (state in ('waiting', 'paid', 'sent', 'called_off', 'given_back')),
    email        text not null check (email = lower(email) and position('@' in email) > 1),
    total_minor  bigint not null check (total_minor >= 0),
    currency     text not null check (currency ~ '^[A-Z]{3}$'),
    -- Who took the money and what they call this. No card number, and no
    -- fragment of one, ever: what is not held cannot leak.
    took_it      text,
    their_ref    text,
    -- The same request twice is one order. The caller chooses this and it is
    -- the caller's to repeat.
    said_once    text not null unique,
    paid_at      timestamptz,
    sent_at      timestamptz,
    called_off_at timestamptz,
    given_back_at timestamptz,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),

    -- Each moment is set once the order has been through it, and stays set
    -- afterwards. Written as "the state implies the moment" rather than as an
    -- equality, because an order that is `sent` has been paid for and still
    -- has its paid_at — and an equality per state is how the version this
    -- replaces came to allow an order sent without ever being paid for.
    constraint paid_when_it_was_paid
        check ((state in ('paid', 'sent', 'given_back')) <= (paid_at is not null)),
    constraint sent_when_it_was_sent
        check ((state = 'sent') <= (sent_at is not null)),
    constraint called_off_when_it_was
        check ((state = 'called_off') = (called_off_at is not null)),
    constraint given_back_when_it_was
        check ((state = 'given_back') = (given_back_at is not null))
);

create index orders_recent on orders (created_at desc, id desc);
create index orders_waiting on orders (created_at) where state = 'waiting';

-- What was ordered, and what it was called and cost at the time. A price that
-- changes next week does not change what somebody was charged last week, which
-- is why the name and the price are copies rather than a look up.
create table order_lines (
    id          uuid primary key,
    order_id    uuid not null references orders (id) on delete cascade,
    -- Kept if the product is deleted: what somebody was sent is part of the
    -- order, and a line pointing at nothing is a receipt with a hole in it.
    product_id  uuid references products (id) on delete set null,
    name        text not null,
    each_minor  bigint not null check (each_minor >= 0),
    how_many    integer not null check (how_many > 0 and how_many <= 1000),
    created_at  timestamptz not null default now(),

    -- One line per product. Asking for three and then two more is a line
    -- saying five, not two lines to add up later.
    unique (order_id, product_id)
);

create index order_lines_of_an_order on order_lines (order_id);

-- Stock taken off the shelf for a checkout nobody has paid for yet. It goes
-- back when the hold runs out, which is what stops a basket abandoned at
-- lunchtime from holding the last one of something at closing.
create table holds (
    id          uuid primary key,
    order_id    uuid not null references orders (id) on delete cascade,
    product_id  uuid not null references products (id) on delete cascade,
    how_many    integer not null check (how_many > 0),
    until       timestamptz not null,
    -- Set when the stock went back, or when the hold became a sale. Either
    -- way there is nothing further to put back.
    settled_at  timestamptz,
    created_at  timestamptz not null default now()
);

-- What the sweeper reads: holds that have run out and have not been settled.
create index holds_to_put_back on holds (until) where settled_at is null;
create index holds_of_an_order on holds (order_id);

create table coupons (
    id           uuid primary key,
    -- Upper case, because a code read off a poster and typed in lower case is
    -- the same code.
    code         text not null check (code = upper(code) and length(code) between 3 and 40),
    kind         text not null check (kind in ('percent', 'amount')),
    percent      integer check (percent between 1 and 100),
    amount_minor bigint check (amount_minor > 0),
    currency     text check (currency ~ '^[A-Z]{3}$'),
    at_most_uses bigint check (at_most_uses > 0),
    expires_at   timestamptz,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),

    -- The kind says which column is filled in, and the other is empty. A row
    -- with both is a discount two readers work out differently.
    constraint a_percentage_or_an_amount check (
        (kind = 'percent' and percent is not null
            and amount_minor is null and currency is null)
        or
        (kind = 'amount' and percent is null
            and amount_minor is not null and currency is not null)
    )
);

create unique index coupons_code on coupons (code);

-- One row per use, so "used twice" is something the database refuses rather
-- than something a counter is asked about and then trusted.
create table coupon_uses (
    coupon_id  uuid not null references coupons (id) on delete cascade,
    order_id   uuid not null references orders (id) on delete cascade,
    created_at timestamptz not null default now(),

    primary key (coupon_id, order_id)
);

create index coupon_uses_of_an_order on coupon_uses (order_id);
