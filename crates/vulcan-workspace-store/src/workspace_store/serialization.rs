use super::*;

pub(super) fn parse_session_id(value: &str) -> Result<SessionId, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(String::from("解析持久化会话标识失败：会话标识不能为空。"));
    }

    Uuid::parse_str(normalized)
        .map_err(|error| format!("解析持久化会话标识失败：{normalized}，{error}"))
}

/// Returns the inclusive turn-index bounds used for one search-result context load.
/// 返回单次搜索结果上下文加载使用的闭区间轮次边界。
#[cfg(test)]
pub(super) fn session_history_context_turn_bounds(
    anchor_turn_index: i64,
    radius: i64,
) -> (i64, i64) {
    let safe_anchor = anchor_turn_index.max(0);
    let safe_radius = radius.max(0);
    (
        safe_anchor.saturating_sub(safe_radius).max(0),
        safe_anchor.saturating_add(safe_radius),
    )
}

/// Builds one short focus preview from the latest visible transcript row.
/// 根据最近一条可见转录行构建一段简短聚焦预览。
pub(super) fn session_focus_preview(messages: &[SessionMessageEntry]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.visible)
        .map(|message| truncate_preview_lines(&message.body, 1, 36))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from("等待新的上下文与执行结果"))
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
pub(super) fn current_unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as i64
}

/// Returns the current Unix timestamp in milliseconds as one text value.
/// 以文本值形式返回当前 Unix 毫秒时间戳。
pub(super) fn current_unix_millis_text() -> String {
    current_unix_millis().to_string()
}

/// Returns the provided timestamp text or falls back to the current time when it is empty.
/// 返回给定时间戳文本，若为空则回退到当前时间。
pub(super) fn non_empty_timestamp_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        current_unix_millis_text()
    } else {
        trimmed.to_string()
    }
}

/// Returns a required cloned JSON value from a session boundary wrapper.
/// 从 会话边界包装对象中返回一个必需的 JSON 值副本。
pub(super) fn required_value(payload: &Value, key: &str) -> Result<Value, String> {
    payload
        .get(key)
        .cloned()
        .ok_or_else(|| format!("session boundary payload missing `{key}`"))
}

/// Returns a required string from a session boundary wrapper.
/// 从 会话边界包装对象中返回一个必需字符串。
pub(super) fn required_string(payload: &Value, key: &str) -> Result<String, String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("session boundary payload missing string `{key}`"))
}

/// Returns an optional string from a session boundary wrapper.
/// 从 会话边界包装对象中返回一个可选字符串。
pub(super) fn optional_string(payload: &Value, key: &str) -> Option<String> {
    payload.get(key)?.as_str().map(str::to_string)
}

/// Converts one optional string into the nullable SQLite value expected by the controller SDK.
/// 将一个可选字符串转换为控制器 SDK 期望的可空 SQLite 值。
pub(super) fn optional_sqlite_string(value: &Option<String>) -> ControllerSqliteValue {
    value
        .as_ref()
        .map(|value| ControllerSqliteValue::String(value.clone()))
        .unwrap_or(ControllerSqliteValue::Null)
}

/// Parses one required Unix-milliseconds text value without inventing a fallback timestamp.
/// 解析一条必需的 Unix 毫秒文本值，且不构造兜底时间戳。
///
/// # Parameters
/// 参数
/// `value` is the raw persisted timestamp text.
/// `value` 是原始持久化时间戳文本。
///
/// # Returns
/// 返回值
/// Returns the parsed milliseconds or an explicit empty/malformed-value error.
/// 返回解析后的毫秒值；为空或格式错误时返回显式错误。
pub(super) fn parse_unix_millis_text(value: &str) -> Result<i64, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(String::from("Unix 毫秒时间戳不能为空。"));
    }
    trimmed
        .parse::<i64>()
        .map_err(|error| format!("无效 Unix 毫秒时间戳 `{trimmed}`：{error}"))
}

/// Converts required Unix-milliseconds text into the canonical UTC session-directory date.
/// 将必需的 Unix 毫秒文本转换为规范 UTC 会话目录日期。
///
/// # Parameters
/// 参数
/// `value` is the raw persisted session creation timestamp.
/// `value` 是原始持久化会话创建时间戳。
///
/// # Returns
/// 返回值
/// Returns `YYYY-MM-DD` for valid input or an explicit timestamp error.
/// 输入有效时返回 `YYYY-MM-DD`，否则返回显式时间戳错误。
pub(super) fn session_directory_date_from_unix_millis_text(value: &str) -> Result<String, String> {
    // millis is parsed strictly so corrupted locators never collapse onto the Unix epoch directory.
    // millis 采用严格解析，使损坏定位信息绝不会折叠到 Unix 纪元目录。
    let millis = parse_unix_millis_text(value)?;
    // days uses Euclidean division so pre-epoch timestamps keep the correct UTC date.
    // days 使用欧几里得除法，使纪元前时间戳保持正确 UTC 日期。
    let days = millis.div_euclid(1_000).div_euclid(86_400);
    // year, month, and day are the canonical Gregorian path components.
    // year、month 与 day 是规范格里高利路径组成部分。
    let (year, month, day) = civil_date_from_unix_days(days);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

/// Converts days since the Unix epoch into one proleptic Gregorian UTC date.
/// 将 Unix 纪元以来的天数转换为一个前推格里高利 UTC 日期。
///
/// # Parameters
/// 参数
/// `days_since_unix_epoch` is the signed whole-day offset from 1970-01-01 UTC.
/// `days_since_unix_epoch` 是相对 1970-01-01 UTC 的有符号完整天数偏移。
///
/// # Returns
/// 返回值
/// Returns `(year, month, day)` in the proleptic Gregorian calendar.
/// 返回前推格里高利历中的 `(年, 月, 日)`。
fn civil_date_from_unix_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let shifted_days = days_since_unix_epoch + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

/// Builds one relative-time label from one Unix-milliseconds timestamp.
/// 根据一条 Unix 毫秒时间戳构建相对时间标签。
pub(super) fn relative_time_label_from_unix_millis(timestamp_millis: i64) -> String {
    let now = current_unix_millis();
    let delta_millis = now.saturating_sub(timestamp_millis).max(0);
    let delta_secs = delta_millis / 1_000;

    if delta_secs < 60 {
        return String::from("刚刚");
    }

    let delta_minutes = delta_secs / 60;
    if delta_minutes < 60 {
        return format!("{delta_minutes}分钟前");
    }

    let delta_hours = delta_minutes / 60;
    if delta_hours < 24 {
        return format!("{delta_hours}小时前");
    }

    let delta_days = delta_hours / 24;
    if delta_days < 7 {
        return format!("{delta_days}天前");
    }

    if delta_days < 30 {
        return format!("{}周前", (delta_days / 7).max(1));
    }

    format!("{}月前", (delta_days / 30).max(1))
}

/// Truncates one multi-line preview into a single compact snippet used by persistence-derived summaries.
/// 将一段多行预览截断为单条紧凑片段，供基于持久化的摘要使用。
pub(super) fn truncate_preview_lines(value: &str, max_lines: usize, max_chars: usize) -> String {
    let mut preview = String::new();
    let mut preview_chars = 0usize;
    let mut exceeded = false;
    let mut selected_lines = 0usize;
    // Keep only the bounded prefix needed for the final preview.
    // 只保留最终预览所需的有界前缀。
    for line in value.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if selected_lines >= max_lines {
            break;
        }
        if selected_lines > 0 {
            append_preview_piece(
                &mut preview,
                &mut preview_chars,
                &mut exceeded,
                max_chars,
                " / ",
            );
        }
        append_preview_piece(
            &mut preview,
            &mut preview_chars,
            &mut exceeded,
            max_chars,
            line,
        );
        selected_lines += 1;
        if exceeded {
            break;
        }
    }
    if selected_lines == 0 {
        for word in value.split_whitespace() {
            if !preview.is_empty() {
                append_preview_piece(
                    &mut preview,
                    &mut preview_chars,
                    &mut exceeded,
                    max_chars,
                    " ",
                );
            }
            append_preview_piece(
                &mut preview,
                &mut preview_chars,
                &mut exceeded,
                max_chars,
                word,
            );
            if exceeded {
                break;
            }
        }
    }
    finish_preview_text(preview, exceeded, max_chars)
}

/// Appends one preview piece while preserving enough overflow state to mirror final truncation.
/// 追加单段预览文本，并保留足够的溢出状态以复现最终截断语义。
fn append_preview_piece(
    preview: &mut String,
    preview_chars: &mut usize,
    exceeded: &mut bool,
    max_chars: usize,
    piece: &str,
) {
    if *exceeded {
        return;
    }
    for character in piece.chars() {
        if *preview_chars < max_chars {
            preview.push(character);
            *preview_chars += 1;
        } else {
            *exceeded = true;
            return;
        }
    }
}

/// Finalizes one bounded preview with the existing single-ellipsis truncation rule.
/// 使用既有的单省略号截断规则完成有界预览。
fn finish_preview_text(preview: String, exceeded: bool, max_chars: usize) -> String {
    if !exceeded {
        return preview;
    }
    let keep_chars = max_chars.saturating_sub(1);
    let mut truncated = preview.chars().take(keep_chars).collect::<String>();
    truncated.push('…');
    truncated
}
