//! Minute-precision UTC timestamps written into node metadata.
//! 写入节点元数据的分钟精度 UTC 时间戳。

use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current UTC time as `YYYY-MM-DDTHH:MMZ`.
/// 以 `YYYY-MM-DDTHH:MMZ` 返回当前 UTC 时间。
pub fn now_utc_minute() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format_utc_minute(seconds)
}

/// Formats Unix seconds as a minute-precision UTC timestamp.
/// 将 Unix 秒格式化为分钟精度 UTC 时间戳。
pub(crate) fn format_utc_minute(unix_seconds: u64) -> String {
    let days = unix_seconds / 86_400;
    let minute_of_day = (unix_seconds % 86_400) / 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}Z",
        minute_of_day / 60,
        minute_of_day % 60
    )
}

/// Returns whether text has the exact `YYYY-MM-DDTHH:MMZ` shape.
/// 返回文本是否严格符合 `YYYY-MM-DDTHH:MMZ` 形状。
#[cfg(test)]
pub(crate) fn is_utc_minute(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 17
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 => *byte == b':',
            16 => *byte == b'Z',
            _ => byte.is_ascii_digit(),
        })
}

/// Converts days since 1970-01-01 into a proleptic Gregorian date (Howard Hinnant's algorithm).
/// 将 1970-01-01 起的天数转换为前推格里历日期（Howard Hinnant 算法）。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known instants format to their calendar dates, and the output passes the shape check.
    /// 已知时刻格式化为对应日历日期，且输出通过形状校验。
    #[test]
    fn formats_known_instants() {
        assert_eq!(format_utc_minute(0), "1970-01-01T00:00Z");
        assert_eq!(format_utc_minute(951_782_400), "2000-02-29T00:00Z");
        assert_eq!(format_utc_minute(1_792_503_240), "2026-10-20T13:34Z");
        assert!(is_utc_minute(&now_utc_minute()));
        assert!(!is_utc_minute("2026-10-20 13:34Z"));
    }
}
