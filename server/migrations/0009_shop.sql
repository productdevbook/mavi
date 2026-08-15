-- Money is minor units and a currency, never a float, and never a number on
-- its own: a column holding 1250 says nothing until something says what of.
create type currency as enum ('TRY', 'EUR', 'USD', 'GBP');

create table products (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    slug         text not null check (slug ~ '^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$'),
    name         text not null check (length(name) between 1 and 300),
    description  text,
    price_minor  bigint not null check (price_minor >= 0),
    currency     currency not null,
    -- What is on the shelf, before anything held for a checkout in flight.
    stock        integer not null default 0 check (stock >= 0),
    low_stock_at integer check (low_stock_at >= 0),
    active       boolean not null default true,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    deleted_at   timestamptz,
    unique (tenant_id, slug)
);

create trigger products_touch before update on products
    for each row execute function touch_updated_at();

create index products_tenant_idx on products (tenant_id, created_at desc)
    where deleted_at is null;

create type cart_state as enum ('open', 'ordered', 'abandoned');

create table carts (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    state      cart_state not null default 'open',
    currency   currency not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger carts_touch before update on carts
    for each row execute function touch_updated_at();

create index carts_tenant_idx on carts (tenant_id, created_at desc);

create table cart_items (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    cart_id    uuid not null references carts (id) on delete cascade,
    product_id uuid not null references products (id) on delete restrict,
    quantity   integer not null check (quantity > 0 and quantity <= 1000),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    -- One line per product: asking for three and then two more is a line
    -- saying five, not two lines to add up later. Scoped to the site like
    -- every other uniqueness here, so one site's cart cannot collide with
    -- another's.
    unique (tenant_id, cart_id, product_id)
);

create trigger cart_items_touch before update on cart_items
    for each row execute function touch_updated_at();

create index cart_items_cart_idx on cart_items (cart_id);
create index cart_items_product_idx on cart_items (product_id);
create index cart_items_tenant_idx on cart_items (tenant_id);

create type order_state as enum ('pending', 'paid', 'fulfilled', 'cancelled', 'refunded');

create table orders (
    id            uuid primary key default gen_random_uuid(),
    tenant_id     uuid not null references tenants (id) on delete cascade,
    cart_id       uuid references carts (id) on delete set null,
    state         order_state not null default 'pending',
    email         text not null check (email = lower(email) and position('@' in email) > 1),
    total_minor   bigint not null check (total_minor >= 0),
    currency      currency not null,
    -- Who took the money, and what they call this. No card number, no
    -- fragment of one, ever: what is not held cannot leak.
    provider      text,
    provider_ref  text,
    -- The same request twice is one order. The caller chooses this and it is
    -- the caller's to repeat.
    idempotency_key text not null,
    paid_at       timestamptz,
    fulfilled_at  timestamptz,
    cancelled_at  timestamptz,
    refunded_at   timestamptz,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now(),
    unique (tenant_id, idempotency_key),
    check ((state = 'paid') = (paid_at is not null) or state in ('fulfilled', 'refunded')),
    check ((state = 'fulfilled') = (fulfilled_at is not null) or state = 'refunded')
);

create trigger orders_touch before update on orders
    for each row execute function touch_updated_at();

create index orders_tenant_idx on orders (tenant_id, created_at desc);
create index orders_cart_idx on orders (cart_id);
create index orders_pending_idx on orders (created_at) where state = 'pending';

create table order_items (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenants (id) on delete cascade,
    order_id    uuid not null references orders (id) on delete cascade,
    product_id  uuid references products (id) on delete set null,
    -- What it was called and what it cost at the time. A price that changes
    -- later does not change what somebody was charged.
    name        text not null,
    unit_minor  bigint not null check (unit_minor >= 0),
    quantity    integer not null check (quantity > 0),
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
);

create trigger order_items_touch before update on order_items
    for each row execute function touch_updated_at();

create index order_items_order_idx on order_items (order_id);
create index order_items_product_idx on order_items (product_id);
create index order_items_tenant_idx on order_items (tenant_id);

-- Stock taken out of the shelf for a checkout that has not been paid for yet.
-- It goes back when the hold runs out, which is what stops an abandoned
-- checkout from keeping the last one of something forever.
create table stock_holds (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenants (id) on delete cascade,
    order_id   uuid not null references orders (id) on delete cascade,
    product_id uuid not null references products (id) on delete cascade,
    quantity   integer not null check (quantity > 0),
    expires_at timestamptz not null,
    released_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger stock_holds_touch before update on stock_holds
    for each row execute function touch_updated_at();

create index stock_holds_expiry_idx on stock_holds (expires_at) where released_at is null;
create index stock_holds_order_idx on stock_holds (order_id);
create index stock_holds_product_idx on stock_holds (product_id);
create index stock_holds_tenant_idx on stock_holds (tenant_id);

create type coupon_kind as enum ('percent', 'amount');

create table coupons (
    id           uuid primary key default gen_random_uuid(),
    tenant_id    uuid not null references tenants (id) on delete cascade,
    code         text not null check (code = upper(code) and length(code) between 3 and 40),
    kind         coupon_kind not null,
    -- A percentage, or minor units off. One column, and the kind says which.
    value        bigint not null check (value > 0),
    uses_allowed integer check (uses_allowed > 0),
    expires_at   timestamptz,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now(),
    unique (tenant_id, code),
    check (kind <> 'percent' or value <= 100)
);

create trigger coupons_touch before update on coupons
    for each row execute function touch_updated_at();

create index coupons_tenant_idx on coupons (tenant_id);

-- One row per use, so that "used twice" is something the database refuses
-- rather than something a counter is asked about.
create table coupon_uses (
    coupon_id  uuid not null references coupons (id) on delete cascade,
    order_id   uuid not null references orders (id) on delete cascade,
    tenant_id  uuid not null references tenants (id) on delete cascade,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (coupon_id, order_id)
);

create trigger coupon_uses_touch before update on coupon_uses
    for each row execute function touch_updated_at();

create index coupon_uses_order_idx on coupon_uses (order_id);
create index coupon_uses_tenant_idx on coupon_uses (tenant_id);

alter table products    enable row level security;
alter table carts       enable row level security;
alter table cart_items  enable row level security;
alter table orders      enable row level security;
alter table order_items enable row level security;
alter table stock_holds enable row level security;
alter table coupons     enable row level security;
alter table coupon_uses enable row level security;

alter table products    force row level security;
alter table carts       force row level security;
alter table cart_items  force row level security;
alter table orders      force row level security;
alter table order_items force row level security;
alter table stock_holds force row level security;
alter table coupons     force row level security;
alter table coupon_uses force row level security;

create policy tenant_isolation on products
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on carts
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on cart_items
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on orders
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on order_items
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on stock_holds
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on coupons
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
create policy tenant_isolation on coupon_uses
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());
