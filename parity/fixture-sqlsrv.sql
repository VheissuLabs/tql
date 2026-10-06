drop table if exists taggings, tickets, wallets, shipments, orders, tags, customers, products, audit_log, empty_things, regions;
create table customers (id int constraint pk_customers primary key, name nvarchar(100), email nvarchar(100), notes nvarchar(max), profile nvarchar(max));
create table orders (id int constraint pk_orders primary key, customer_id int, total decimal(8, 2), placed_at datetime);
create table products (sku varchar(10) constraint pk_products primary key, title varchar(100), price decimal(8, 2));
create table audit_log (happened_at date, message varchar(100));
create table empty_things (id int constraint pk_empty_things primary key, label varchar(100));
create table regions (code varchar(5), region varchar(10), label varchar(50), constraint pk_regions primary key (code, region));
create table shipments (id int constraint pk_shipments primary key, order_id int, customer_id int, carrier varchar(20) not null constraint shipments_carrier_default default 'post', shipped_at datetime2(0), foreign key (order_id) references orders(id), foreign key (customer_id) references customers(id));
create table tags (id int constraint pk_tags primary key, name varchar(30) not null constraint tags_name_unique unique);
create table taggings (customer_id int, tag_id int, created_at date, foreign key (customer_id) references customers(id), foreign key (tag_id) references tags(id));
create table tickets (id int identity constraint pk_tickets primary key, customer_id int not null, subject varchar(200), status varchar(10) constraint tickets_status_default default 'open', foreign key (customer_id) references customers(id));
create table wallets (id int constraint pk_wallets primary key, customer_id int constraint wallets_customer_id_unique unique, balance decimal(8, 2), foreign key (customer_id) references customers(id));
with n(i) as (select 1 union all select i + 1 from n where i < 250)
insert into customers
select i,
       concat(N'Customer ', right(concat('00', i), 3)),
       concat('c', i, '@example.com'),
       case when i % 2 = 0 then null else 'vip' end,
       case when i % 5 = 0 then '{"tier": "gold", "tags": ["a", "b"]}' else null end
from n option (maxrecursion 300);
update customers set notes = 'a much longer note that goes on and on past the comfortable width of a column' where id = 3;
update customers set name = N'Zoë Ünïcödé' where id = 4;
insert into orders values (1, 7, 19.50, '2026-01-02 10:00:00'), (2, 3, 4.25, '2026-02-03 11:30:00'), (3, 7, 100, null);
insert into products values ('B-2', 'Bolt', 1.15), ('A-1', 'Anvil', 99.99), ('C-3', 'Cog', 0.15);
insert into audit_log values ('2026-03-01', 'created'), ('2026-03-02', 'updated');
insert into regions values ('NZ', 'north', 'Northland'), ('NZ', 'south', 'Southland');
insert into shipments values (1, 1, 7, 'courier', '2026-01-03 09:00:00'), (2, 2, 3, 'post', null), (3, 3, 7, 'courier', '2026-03-04 12:00:00');
insert into tags values (1, 'loyal'), (2, 'wholesale'), (3, 'late payer');
insert into taggings values (7, 1, '2026-01-01'), (7, 2, '2026-01-02'), (3, 3, '2026-02-01');
with n(i) as (select 1 union all select i + 1 from n where i < 12)
insert into tickets (customer_id, subject, status)
select 7, concat('Ticket number ', i, ' about an order that went somewhere unexpected'), case when i % 3 = 0 then 'closed' else 'open' end
from n;
insert into tickets (customer_id, subject) values (3, 'Wrong colour');
insert into wallets values (1, 7, 42.5), (2, 3, 5);
