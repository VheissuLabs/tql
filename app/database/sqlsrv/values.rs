use super::*;

pub(super) fn decimal(numeric: Numeric) -> String {
    let scale = numeric.scale() as usize;
    let digits = numeric.value().unsigned_abs().to_string();
    let sign = if numeric.value() < 0 { "-" } else { "" };
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let padded = format!("{digits:0>width$}", width = scale + 1);
    let (whole, fraction) = padded.split_at(padded.len() - scale);
    format!("{sign}{whole}.{fraction}")
}

pub(super) fn day(days_since_year_one: i64) -> Date {
    Date::constant(1, 1, 1)
        .checked_add(days_since_year_one.days())
        .unwrap_or(Date::MAX)
}

pub(super) fn day_since_1900(days: i64) -> Date {
    Date::constant(1900, 1, 1)
        .checked_add(days.days())
        .unwrap_or(Date::MAX)
}

pub(super) fn clock(time: Time) -> String {
    let scale = time.scale() as u32;
    let per_second = 10u64.pow(scale);
    let seconds = time.increments() / per_second;
    let fraction = time.increments() % per_second;
    let whole = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    );
    if scale == 0 {
        return whole;
    }
    format!("{whole}.{fraction:0>width$}", width = scale as usize)
}

pub(super) fn moment(datetime: DateTime2) -> String {
    format!(
        "{} {}",
        day(i64::from(datetime.date().days())),
        clock(datetime.time())
    )
}

pub(super) fn offset_moment(datetime: DateTime2, offset_minutes: i16) -> String {
    let scale = datetime.time().scale() as u32;
    let per_second = 10u64.pow(scale) as i64;
    let increments = datetime.time().increments() as i64;
    let utc = DateTime::from(day(i64::from(datetime.date().days())))
        .checked_add(Span::new().seconds(increments / per_second))
        .unwrap_or(DateTime::MAX);
    let local = utc
        .checked_add(Span::new().minutes(i64::from(offset_minutes)))
        .unwrap_or(DateTime::MAX);
    let seconds_into_day =
        i64::from(local.hour()) * 3600 + i64::from(local.minute()) * 60 + i64::from(local.second());
    let shown_time = clock(Time::new(
        (seconds_into_day * per_second + increments % per_second) as u64,
        scale as u8,
    ));
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let magnitude = offset_minutes.unsigned_abs();
    format!(
        "{} {shown_time} {sign}{:02}:{:02}",
        local.date(),
        magnitude / 60,
        magnitude % 60
    )
}

pub(super) fn hexadecimal(bytes: &[u8]) -> String {
    let digits: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("0x{digits}")
}

pub(super) fn text(value: ColumnData<'static>, column_type: ColumnType) -> Option<String> {
    Some(match value {
        ColumnData::U8(number) => number?.to_string(),
        ColumnData::I16(number) => number?.to_string(),
        ColumnData::I32(number) => number?.to_string(),
        ColumnData::I64(number) => number?.to_string(),
        ColumnData::F32(number) => number?.to_string(),
        ColumnData::F64(number)
            if matches!(column_type, ColumnType::Money | ColumnType::Money4) =>
        {
            format!("{:.4}", number?)
        }
        ColumnData::F64(number) => number?.to_string(),
        ColumnData::Bit(bit) => if bit? { "1" } else { "0" }.to_string(),
        ColumnData::String(text) => text?.into_owned(),
        ColumnData::Guid(guid) => guid?.to_string().to_uppercase(),
        ColumnData::Binary(bytes) => hexadecimal(&bytes?),
        ColumnData::Numeric(numeric) => decimal(numeric?),
        ColumnData::Xml(xml) => xml?.to_string(),
        ColumnData::DateTime(datetime) => {
            let datetime = datetime?;
            let milliseconds = (u64::from(datetime.seconds_fragments()) * 1000 + 150) / 300;
            format!(
                "{} {}",
                day_since_1900(i64::from(datetime.days())),
                clock(Time::new(milliseconds, 3))
            )
        }
        ColumnData::SmallDateTime(datetime) => {
            let datetime = datetime?;
            format!(
                "{} {}",
                day_since_1900(i64::from(datetime.days())),
                clock(Time::new(u64::from(datetime.seconds_fragments()) * 60, 0))
            )
        }
        ColumnData::Time(time) => clock(time?),
        ColumnData::Date(date) => day(i64::from(date?.days())).to_string(),
        ColumnData::DateTime2(datetime) => moment(datetime?),
        ColumnData::DateTimeOffset(datetime) => {
            let datetime = datetime?;
            offset_moment(datetime.datetime2(), datetime.offset())
        }
    })
}

pub(super) fn scalar(value: ColumnData<'static>, column_type: ColumnType) -> Scalar {
    match value {
        ColumnData::Binary(Some(bytes)) => Scalar::Bytes(bytes.into_owned()),
        ColumnData::U8(_)
        | ColumnData::I16(_)
        | ColumnData::I32(_)
        | ColumnData::I64(_)
        | ColumnData::Bit(_)
        | ColumnData::Numeric(_) => text(value, column_type)
            .map(Scalar::Number)
            .unwrap_or(Scalar::Null),
        ColumnData::F32(Some(number)) => Scalar::real(f64::from(number)),
        ColumnData::F64(Some(number))
            if !matches!(column_type, ColumnType::Money | ColumnType::Money4) =>
        {
            Scalar::real(number)
        }
        ColumnData::F64(Some(number)) => Scalar::Number(format!("{number:.4}")),
        other => Scalar::text(text(other, column_type)),
    }
}

pub(super) fn flag(value: &Option<String>) -> bool {
    value.as_deref() == Some("1")
}
