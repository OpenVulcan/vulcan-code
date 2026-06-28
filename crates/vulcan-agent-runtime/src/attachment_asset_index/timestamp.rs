use super::*;

/// Builds current local timestamp parts for attachment staging and cleanup records.
/// 为附件暂存与清理记录构建当前本地时间戳片段。
pub fn current_attachment_timestamp_parts() -> AttachmentTimestampParts {
    let local_now = OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc().to_offset(UtcOffset::UTC));
    build_attachment_timestamp_parts(local_now)
}

/// Parses one cleanup cutoff date text in `YYYY-MM-DD` format.
/// 解析一段 `YYYY-MM-DD` 格式的清理截止日期文本。
pub fn parse_attachment_cleanup_cutoff_date(cutoff_date_text: &str) -> Result<Date, String> {
    let trimmed_date = cutoff_date_text.trim();
    if trimmed_date.is_empty() {
        return Err(String::from("请输入清理截止日期，格式为 YYYY-MM-DD。"));
    }
    Date::parse(trimmed_date, ATTACHMENT_DATE_FORMAT).map_err(|error| {
        format!("清理截止日期 `{trimmed_date}` 无效：{error}。请使用 YYYY-MM-DD 格式。")
    })
}
pub(super) fn build_attachment_timestamp_parts(now: OffsetDateTime) -> AttachmentTimestampParts {
    let compact_prefix = now
        .format(format_description!(
            "[year][month][day]-[hour][minute][second]"
        ))
        .unwrap_or_else(|_| String::from("19700101-000000"));
    let local_timestamp_text = now
        .format(ATTACHMENT_TIMESTAMP_FORMAT)
        .unwrap_or_else(|_| String::from("1970-01-01 00:00:00"));
    let local_date_text = now
        .format(ATTACHMENT_DATE_FORMAT)
        .unwrap_or_else(|_| String::from("1970-01-01"));
    let unix_millis = now.unix_timestamp_nanos().div_euclid(1_000_000) as i64;
    AttachmentTimestampParts {
        compact_prefix,
        local_timestamp_text,
        local_date_text,
        unix_millis,
    }
}
