truncate table customers, orders, products, audit_log, empty_things, regions, shipments, tags, taggings, tickets, wallets restart identity cascade;
insert into customers
select i,
       'Customer ' || lpad(i::text, 3, '0'),
       'c' || i || '@example.com',
       case when i % 2 = 0 then null else 'vip' end,
       case when i % 5 = 0 then '{"tier": "gold", "tags": ["a", "b"]}' else null end
from generate_series(1, 250) as n(i);
update customers set notes = 'a much longer note that goes on and on past the comfortable width of a column' where id = 3;
update customers set name = 'Zoë Ünïcödé' where id = 4;
insert into orders values (1, 7, 19.50, '2026-01-02 10:00:00'), (2, 3, 4.25, '2026-02-03 11:30:00'), (3, 7, 100, null);
insert into products values ('B-2', 'Bolt', 0.15), ('A-1', 'Anvil', 99.99);
insert into audit_log values ('2026-03-01', 'created'), ('2026-03-02', 'updated');
insert into regions values ('NZ', 'north', 'Northland'), ('NZ', 'south', 'Southland');
insert into shipments values (1, 1, 7, 'courier', '2026-01-03 09:00:00'), (2, 2, 3, 'post', null), (3, 3, 7, 'courier', '2026-03-04 12:00:00');
insert into tags values (1, 'loyal'), (2, 'wholesale'), (3, 'late payer');
insert into taggings values (7, 1, '2026-01-01'), (7, 2, '2026-01-02'), (3, 3, '2026-02-01');
insert into tickets (customer_id, subject, status)
select 7, 'Ticket number ' || i || ' about an order that went somewhere unexpected', case when i % 3 = 0 then 'closed' else 'open' end
from generate_series(1, 12) as n(i);
insert into tickets (customer_id, subject) values (3, 'Wrong colour');
insert into wallets values (1, 7, 42.5, true), (2, 3, 0, false);
