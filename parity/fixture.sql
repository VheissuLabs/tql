create table customers (id integer primary key, name text, email text, notes text, profile text);
create table orders (id integer primary key, customer_id integer, total real, placed_at text);
create table products (sku text primary key, title text, price real);
create table audit_log (happened_at text, message text);
create table empty_things (id integer primary key, label text);
create table regions (code text, region text, label text, primary key (code, region));

with recursive n(i) as (select 1 union all select i + 1 from n where i < 250)
insert into customers
select i,
       'Customer ' || substr('000' || i, -3),
       'c' || i || '@example.com',
       case when i % 2 = 0 then null else 'vip' end,
       case when i % 5 = 0 then '{"tier": "gold", "tags": ["a", "b"]}' else null end
from n;

update customers set notes = 'a much longer note that goes on and on past the comfortable width of a column' where id = 3;
update customers set name = 'Zoë Ünïcödé' where id = 4;

insert into orders values (1, 7, 19.5, '2026-01-02 10:00:00'), (2, 3, 4.25, '2026-02-03 11:30:00'), (3, 7, 100, null);
insert into products values ('B-2', 'Bolt', 0.15), ('A-1', 'Anvil', 99.99);
insert into audit_log values ('2026-03-01', 'created'), ('2026-03-02', 'updated');
insert into regions values ('NZ', 'north', 'Northland'), ('NZ', 'south', 'Southland');
