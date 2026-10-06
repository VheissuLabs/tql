create database tql_rust_scratch;
use tql_rust_scratch;
create table customers (id int primary key, name varchar(100), email varchar(100), notes text, profile text);
create table orders (id int primary key, customer_id int, total decimal(8, 2), placed_at datetime);
create table products (sku varchar(10) primary key, title varchar(100), price decimal(8, 2));
create table audit_log (happened_at date, message varchar(100));
create table empty_things (id int primary key, label varchar(100));
create table regions (code varchar(5), region varchar(10), label varchar(50), primary key (code, region));
create view big_spenders as select customer_id, sum(total) as spent from orders group by customer_id;

insert into customers
with recursive n(i) as (select 1 union all select i + 1 from n where i < 250)
select i,
       concat('Customer ', lpad(i, 3, '0')),
       concat('c', i, '@example.com'),
       case when i % 2 = 0 then null else 'vip' end,
       case when i % 5 = 0 then '{"tier": "gold", "tags": ["a", "b"]}' else null end
from n;

update customers set notes = 'a much longer note that goes on and on past the comfortable width of a column' where id = 3;
update customers set name = 'Zoë Ünïcödé' where id = 4;
insert into orders values (1, 7, 19.50, '2026-01-02 10:00:00'), (2, 3, 4.25, '2026-02-03 11:30:00'), (3, 7, 100, null);
insert into products values ('B-2', 'Bolt', 0.15), ('A-1', 'Anvil', 99.99);
insert into audit_log values ('2026-03-01', 'created'), ('2026-03-02', 'updated');
insert into regions values ('NZ', 'north', 'Northland'), ('NZ', 'south', 'Southland');
