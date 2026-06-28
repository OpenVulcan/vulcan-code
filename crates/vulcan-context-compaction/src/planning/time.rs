use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current Unix-milliseconds timestamp as a stable decimal string.
/// 以稳定十进制字符串形式返回当前 Unix 毫秒时间戳。
pub(super) fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
