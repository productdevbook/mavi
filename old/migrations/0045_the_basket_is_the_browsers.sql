-- A cart here is the visitor's, held by the page they are on, and checkout
-- takes the lines in one request. These tables were written for a cart the
-- server keeps and nothing has ever read or written them: an empty table with
-- a row-level policy on it reads as a feature that exists.

alter table orders drop column cart_id;

drop table cart_items;
drop table carts;

drop type cart_state;
