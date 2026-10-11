use super::*;

#[test]
fn encrypts_the_way_the_same_ssl_mode_does_for_postgres() {
    let expectations = [
        (None, true, true),
        (Some("disable"), false, true),
        (Some("prefer"), true, true),
        (Some("require"), true, true),
        (Some("verify-ca"), true, false),
        (Some("verify-full"), true, false),
        (Some(" verify-full "), true, false),
    ];
    for (mode, encrypt, trust_server_certificate) in expectations {
        assert_eq!(
            encryption(mode),
            Encryption {
                encrypt,
                trust_server_certificate
            },
            "{mode:?}"
        );
    }
}

#[test]
fn pages_with_offset_and_fetch_giving_it_the_order_it_insists_on() {
    assert_eq!(
        page("select * from [fruit]", 50, 0),
        "select * from [fruit] order by (select null) offset 0 rows fetch next 50 rows only"
    );
    assert_eq!(
        page("select * from [fruit] order by [name] desc", 50, 100),
        "select * from [fruit] order by [name] desc offset 100 rows fetch next 50 rows only"
    );
}

#[test]
fn quotes_with_brackets_doubling_a_closing_one() {
    assert_eq!(quote_bracket("fruit"), "[fruit]");
    assert_eq!(quote_bracket("odd]name"), "[odd]]name]");
}

#[test]
fn numbers_placeholders_outside_quotes() {
    assert_eq!(
        numbered_parameters("select * from [a?] where x = ? and y = '?' and z in (?, ?)"),
        "select * from [a?] where x = @P1 and y = '?' and z in (@P2, @P3)"
    );
}

#[test]
fn writes_decimals_with_a_leading_zero_and_their_scale() {
    assert_eq!(decimal(Numeric::new_with_scale(50, 2)), "0.50");
    assert_eq!(decimal(Numeric::new_with_scale(-5, 2)), "-0.05");
    assert_eq!(decimal(Numeric::new_with_scale(1250, 2)), "12.50");
    assert_eq!(decimal(Numeric::new_with_scale(42, 0)), "42");
}

#[test]
fn writes_dates_and_times_the_way_sql_server_spells_them() {
    let datetime = ColumnData::DateTime(Some(tiberius::time::DateTime::new(45291, 3_349_620)));
    assert_eq!(
        text(datetime, ColumnType::Datetimen).as_deref(),
        Some("2024-01-02 03:06:05.400")
    );
    let small = ColumnData::SmallDateTime(Some(tiberius::time::SmallDateTime::new(45291, 184)));
    assert_eq!(
        text(small, ColumnType::Datetime4).as_deref(),
        Some("2024-01-02 03:04:00")
    );
    let date = tiberius::time::Date::new(738_886);
    assert_eq!(
        text(ColumnData::Date(Some(date)), ColumnType::Daten).as_deref(),
        Some("2024-01-02")
    );
    let time = Time::new(11_045_000_000 + 1, 7);
    assert_eq!(
        text(ColumnData::Time(Some(time)), ColumnType::Timen).as_deref(),
        Some("00:18:24.5000001")
    );
    let offset = tiberius::time::DateTimeOffset::new(
        DateTime2::new(date, Time::new(3600 * 23 * 10, 1)),
        120,
    );
    assert_eq!(
        text(
            ColumnData::DateTimeOffset(Some(offset)),
            ColumnType::DatetimeOffsetn
        )
        .as_deref(),
        Some("2024-01-03 01:00:00.0 +02:00")
    );
}

#[test]
fn writes_money_to_four_places_bits_as_digits_and_bytes_as_hex() {
    assert_eq!(
        text(ColumnData::F64(Some(12.5)), ColumnType::Money).as_deref(),
        Some("12.5000")
    );
    assert_eq!(
        text(ColumnData::F64(Some(1.5)), ColumnType::Float8).as_deref(),
        Some("1.5")
    );
    assert_eq!(
        text(ColumnData::Bit(Some(true)), ColumnType::Bitn).as_deref(),
        Some("1")
    );
    assert_eq!(
        text(
            ColumnData::Binary(Some(vec![1, 171].into())),
            ColumnType::BigVarBin
        )
        .as_deref(),
        Some("0x01AB")
    );
    assert_eq!(text(ColumnData::I32(None), ColumnType::Intn), None);
}

#[test]
fn exports_numbers_bare_and_bytes_as_bytes() {
    assert_eq!(
        scalar(ColumnData::I32(Some(7)), ColumnType::Intn),
        Scalar::Number("7".into())
    );
    assert_eq!(
        scalar(
            ColumnData::Binary(Some(vec![1].into())),
            ColumnType::BigVarBin
        ),
        Scalar::Bytes(vec![1])
    );
    assert_eq!(
        scalar(ColumnData::String(None), ColumnType::NVarchar),
        Scalar::Null
    );
}
