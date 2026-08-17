create table shop_products (
    site_id        uuid not null references site_catalog(site_id),
    id             uuid not null,
    slug           text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]{0,158}[a-z0-9])?$'),
    name           text not null check (char_length(btrim(name)) between 1 and 300),
    description    text check (description is null or char_length(description) <= 10000),
    price_minor    bigint not null check (price_minor >= 0),
    currency       text not null check (currency ~ '^[A-Z]{3}$'),
    stock_on_hand  integer not null default 0 check (stock_on_hand between 0 and 1000000000),
    on_sale        boolean not null default true,
    created_at     timestamptz not null default now(),
    updated_at     timestamptz not null default now(),
    deleted_at     timestamptz,
    primary key (site_id, id)
);

create unique index shop_products_site_slug_active
    on shop_products (site_id, slug)
    where deleted_at is null;

create index shop_products_site_recent
    on shop_products (site_id, created_at desc, id desc)
    where deleted_at is null;

create table shop_coupons (
    site_id       uuid not null references site_catalog(site_id),
    id            uuid not null,
    code          text not null check (code = upper(code) and code ~ '^[A-Z0-9-]{3,40}$'),
    kind          text not null check (kind in ('percent', 'amount')),
    percent       integer,
    amount_minor  bigint,
    currency      text,
    max_uses      bigint check (max_uses is null or max_uses between 1 and 1000000000),
    expires_at    timestamptz,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    deleted_at    timestamptz,
    primary key (site_id, id),
    constraint shop_coupons_rule check (
        (kind = 'percent' and percent between 1 and 100 and amount_minor is null and currency is null)
        or
        (kind = 'amount' and percent is null and amount_minor > 0 and currency ~ '^[A-Z]{3}$')
    )
);

create unique index shop_coupons_site_code_active
    on shop_coupons (site_id, code)
    where deleted_at is null;

create index shop_coupons_site_recent
    on shop_coupons (site_id, created_at desc, id desc)
    where deleted_at is null;

create table shop_order_counters (
    site_id     uuid primary key references site_catalog(site_id),
    next_number bigint not null check (next_number > 0)
);

create table shop_orders (
    site_id            uuid not null references site_catalog(site_id),
    id                 uuid not null,
    number             bigint not null check (number > 0),
    state              text not null default 'waiting'
                       check (state in ('waiting', 'paid', 'sent', 'called_off', 'given_back')),
    email              text not null check (email = lower(email) and position('@' in email) > 1),
    total_minor        bigint not null check (total_minor >= 0),
    currency           text not null check (currency ~ '^[A-Z]{3}$'),
    idempotency_key    text not null check (char_length(idempotency_key) between 1 and 128),
    payment_provider   text,
    payment_reference  text,
    paid_at            timestamptz,
    sent_at            timestamptz,
    called_off_at      timestamptz,
    given_back_at      timestamptz,
    created_at         timestamptz not null default now(),
    updated_at         timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, number),
    constraint shop_orders_paid_time check (
        (state in ('paid', 'sent', 'given_back')) <= (paid_at is not null)
    ),
    constraint shop_orders_sent_time check ((state = 'sent') <= (sent_at is not null)),
    constraint shop_orders_called_off_time check ((state = 'called_off') = (called_off_at is not null)),
    constraint shop_orders_given_back_time check ((state = 'given_back') = (given_back_at is not null))
);

create unique index shop_orders_site_email_idempotency
    on shop_orders (site_id, email, idempotency_key);

create index shop_orders_site_recent
    on shop_orders (site_id, created_at desc, id desc);

create index shop_orders_site_state_recent
    on shop_orders (site_id, state, created_at desc, id desc);

create table shop_order_lines (
    site_id       uuid not null references site_catalog(site_id),
    id            uuid not null,
    order_id      uuid not null,
    product_id    uuid,
    name          text not null check (char_length(name) between 1 and 300),
    each_minor    bigint not null check (each_minor >= 0),
    quantity      integer not null check (quantity between 1 and 1000),
    created_at    timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id, order_id) references shop_orders(site_id, id) on delete cascade,
    foreign key (site_id, product_id) references shop_products(site_id, id),
    unique (site_id, order_id, product_id)
);

create index shop_order_lines_site_order
    on shop_order_lines (site_id, order_id, created_at asc, id asc);

create table shop_stock_holds (
    site_id      uuid not null references site_catalog(site_id),
    id           uuid not null,
    order_id     uuid not null,
    product_id   uuid not null,
    quantity     integer not null check (quantity between 1 and 1000),
    status       text not null default 'held'
                 check (status in ('held', 'consumed', 'released')),
    expires_at   timestamptz not null,
    settled_at   timestamptz,
    created_at   timestamptz not null default now(),
    primary key (site_id, id),
    foreign key (site_id, order_id) references shop_orders(site_id, id) on delete cascade,
    foreign key (site_id, product_id) references shop_products(site_id, id),
    check ((status = 'held') = (settled_at is null))
);

create index shop_stock_holds_site_expired
    on shop_stock_holds (site_id, expires_at, order_id)
    where status = 'held';

create index shop_stock_holds_site_order
    on shop_stock_holds (site_id, order_id, status);

create table shop_coupon_uses (
    site_id    uuid not null references site_catalog(site_id),
    id         uuid not null,
    coupon_id  uuid not null,
    order_id   uuid not null,
    used_at    timestamptz not null default now(),
    primary key (site_id, id),
    unique (site_id, coupon_id, order_id),
    foreign key (site_id, coupon_id) references shop_coupons(site_id, id),
    foreign key (site_id, order_id) references shop_orders(site_id, id) on delete cascade
);

create index shop_coupon_uses_site_coupon
    on shop_coupon_uses (site_id, coupon_id, used_at);

do $$
declare
    table_name text;
begin
    foreach table_name in array array[
        'shop_products',
        'shop_coupons',
        'shop_order_counters',
        'shop_orders',
        'shop_order_lines',
        'shop_stock_holds',
        'shop_coupon_uses'
    ]
    loop
        execute format('alter table %I enable row level security', table_name);
        execute format('alter table %I force row level security', table_name);
        execute format(
            'create policy %I_scope on %I using (site_id = current_setting(''app.site_id'', true)::uuid) with check (site_id = current_setting(''app.site_id'', true)::uuid)',
            table_name,
            table_name
        );
    end loop;
end $$;
