-- An order could only be spoken about by its uuid.
--
-- Somebody writing to a shop says "order 41", not a uuid, and the shop's own
-- screen has to be able to find it. Counted per site: how many orders this
-- machine has taken altogether is nobody's business but the operator's, and a
-- shared sequence tells every customer.

create table counters (
    tenant_id uuid not null references tenants (id) on delete cascade,
    kind      text not null,
    next      bigint not null default 1,
    primary key (tenant_id, kind)
);

alter table counters enable row level security;
alter table counters force row level security;

create policy tenant_isolation on counters
    using (tenant_id = current_tenant_id()) with check (tenant_id = current_tenant_id());

alter table orders
    add column number bigint;

-- What is already here, in the order it happened.
with numbered as (
    select id, row_number() over (partition by tenant_id order by created_at, id) as n
      from orders
)
update orders o set number = numbered.n from numbered where numbered.id = o.id;

insert into counters (tenant_id, kind, next)
select tenant_id, 'order', max(number) + 1 from orders group by tenant_id
on conflict do nothing;

alter table orders
    alter column number set not null,
    add constraint orders_number_is_a_site_s_own unique (tenant_id, number);

-- The next one, and the counter moved on, in a single statement: two orders
-- placed at the same moment are two rows in a queue rather than two orders
-- called 41.
create function next_number(site uuid, of_what text) returns bigint
language sql as $$
    insert into counters (tenant_id, kind, next) values (site, of_what, 2)
    on conflict (tenant_id, kind) do update set next = counters.next + 1
    returning case when counters.next is null then 1 else counters.next - 1 end;
$$;

-- Filled by the database rather than by whoever is inserting: an order written
-- by the mover, by a test or by a future screen is still an order somebody
-- will ask about by number.
create function number_an_order() returns trigger language plpgsql as $$
begin
    if new.number is null then
        new.number := next_number(new.tenant_id, 'order');
    end if;

    return new;
end;
$$;

create trigger orders_are_numbered before insert on orders
    for each row execute function number_an_order();
