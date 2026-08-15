-- A table only an outside crate would know to ask for, proving that
-- something an outside crate carries in through `Outside::migrations` is
-- actually run rather than silently ignored.
create table outside_beacons (
    id uuid primary key default gen_random_uuid(),
    seen_at timestamptz not null default now()
);
