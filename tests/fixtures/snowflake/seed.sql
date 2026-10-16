create or replace database TQL_TEST;
use database TQL_TEST;
create schema if not exists PUBLIC;
create or replace schema STAGING;
use schema PUBLIC;

create or replace table CUSTOMERS (
    ID number(38,0) not null autoincrement start 1 increment 1,
    NAME varchar(100) not null,
    EMAIL varchar(255),
    ACTIVE boolean default true,
    CREATED_AT timestamp_ntz(9) default current_timestamp(),
    constraint CUSTOMERS_PK primary key (ID),
    constraint CUSTOMERS_EMAIL_UNIQUE unique (EMAIL)
) comment = 'People who place orders';

insert into CUSTOMERS (NAME, EMAIL, ACTIVE, CREATED_AT) values
    ('Ada Lovelace', 'ada@example.com', true, '2026-01-05 09:30:00.123456789'),
    ('Grace Hopper', 'grace@example.com', true, '2026-02-11 14:00:00'),
    ('O''Brien', 'obrien@example.com', false, '2026-03-20 08:15:30.5'),
    ('Back\\slash', null, true, '1969-12-31 23:59:58.5');

create or replace table ORDERS (
    ID number(38,0) not null,
    CUSTOMER_ID number(38,0) not null,
    TOTAL number(10,2),
    RATE float,
    PLACED_ON date,
    PLACED_AT timestamp_tz(3),
    SEEN_AT timestamp_ltz(0),
    PICKUP time(0),
    NOTE varchar,
    constraint ORDERS_PK primary key (ID),
    constraint ORDERS_CUSTOMER_FK foreign key (CUSTOMER_ID) references CUSTOMERS (ID)
) cluster by (PLACED_ON);

insert into ORDERS values
    (1, 1, 12.50, 0.075, '2026-01-06', '2026-01-06 10:00:00.250 +01:00', '2026-01-06 09:00:00', '17:30:00', 'first'),
    (2, 1, 99.99, 0.2, '2026-02-01', '2026-02-01 23:59:59.999 -05:00', '2026-02-02 04:59:59', '08:00:00', null),
    (3, 2, 0.00, 0, '2026-02-14', '2026-02-14 12:00:00 +00:00', '2026-02-14 12:00:00', '12:00:00', 'free, with a comma'),
    (4, 3, -5.25, -1.5, '1969-12-31', '1969-12-31 23:59:59 +00:00', '1969-12-31 23:59:59', '00:00:00', 'refund
on two lines');

create or replace table EVENTS (
    ID number(38,0),
    PAYLOAD variant,
    TAGS array,
    ATTRIBUTES object,
    CHECKSUM binary,
    HAPPENED_AT timestamp_ntz(3)
);

insert into EVENTS
    select 1, parse_json('{"kind": "signup", "plan": {"name": "pro", "seats": 3}}'),
           array_construct('new', 'web'), object_construct('source', 'ad', 'cost', 1.25),
           to_binary('cafe', 'HEX'), '2026-01-05 09:30:00.123'
    union all
    select 2, parse_json('[1, 2, null]'), array_construct(), object_construct(),
           to_binary('', 'HEX'), '2026-01-06 00:00:00'
    union all
    select 3, null, null, null, null, null;

create or replace table READINGS (
    ID number(38,0) not null,
    SENSOR varchar(20),
    VALUE number(12,4),
    TAKEN_AT timestamp_ntz(0),
    constraint READINGS_PK primary key (ID)
);

insert into READINGS
    select seq4() + 1,
           'sensor-' || (seq4() % 25),
           uniform(0, 100000, random()) / 100,
           dateadd(minute, seq4(), '2026-01-01 00:00:00'::timestamp_ntz)
    from table(generator(rowcount => 60000));

create or replace view ACTIVE_CUSTOMERS as
    select ID, NAME, EMAIL from CUSTOMERS where ACTIVE;

create or replace table STAGING.RAW_EVENTS (
    LOADED_AT timestamp_ntz default current_timestamp(),
    LINE varchar
);

insert into STAGING.RAW_EVENTS (LINE) values ('{"raw": true}'), ('not json');

create or replace table STAGING."Mixed Case" ("Quoted Column" varchar, "lower" number);

insert into STAGING."Mixed Case" values ('kept as typed', 1);
