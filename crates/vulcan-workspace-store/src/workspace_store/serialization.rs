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

/// Parses one persisted transcript UI-state JSON payload into the typed runtime-state structure.
/// 将一条持久化转录界面状态 JSON 载荷解析为带类型的运行时状态结构。
pub(super) fn parse_session_ui_state_from_json(value: &str) -> Option<SessionTranscriptUiState> {
    serde_json::from_str(value).ok()
}

/// Parses one persisted provider continuation JSON payload into the typed cursor structure.
/// 将一条持久化 Provider continuation JSON 载荷解析为带类型的游标结构。
pub(super) fn parse_session_continuation_from_json(
    value: &str,
) -> Option<SessionProviderContinuationState> {
    serde_json::from_str(value).ok()
}

/// Parses one persisted canonical conversation-state JSON payload into the typed runtime structure.
/// 将一条持久化规范会话状态 JSON 载荷解析为带类型的运行时结构。
pub(super) fn parse_session_conversation_state_from_json(
    value: Option<&str>,
) -> Option<SessionConversationState> {
    value.and_then(|payload| serde_json::from_str(payload).ok())
}

/// Parses one persisted hidden context-injection JSON payload into runtime state.
/// 将一条持久化隐藏上下文注入 JSON 载荷解析为运行态。
pub(super) fn parse_context_injection_state_from_json(
    value: &str,
) -> Option<ContextInjectionState> {
    serde_json::from_str(value).ok()
}

/// Serializes one typed transcript UI-state structure into the persisted JSON payload when needed.
/// 在需要时将一条带类型的转录界面状态结构序列化为持久化 JSON 载荷。
pub(super) fn serialize_session_ui_state_to_json(
    state: &SessionTranscriptUiState,
) -> Option<String> {
    if state.is_empty() {
        None
    } else {
        serde_json::to_string(state).ok()
    }
}

/// Serializes one typed provider continuation cursor into the persisted JSON payload when needed.
/// 在需要时将一条带类型的 Provider continuation 游标序列化为持久化 JSON 载荷。
pub(super) fn serialize_session_continuation_to_json(
    continuation: &SessionProviderContinuationState,
) -> Option<String> {
    if continuation.provider_id.is_none()
        && continuation.model_id.is_none()
        && continuation.last_provider_message_id.is_none()
        && continuation.updated_at.is_none()
    {
        None
    } else {
        serde_json::to_string(continuation).ok()
    }
}

/// Serializes hidden context-injection runtime state into the persisted JSON payload when needed.
/// 在需要时将隐藏上下文注入运行态序列化为持久化 JSON 载荷。
pub(super) fn serialize_context_injection_state_to_json(
    state: &ContextInjectionState,
) -> Option<String> {
    if state.is_empty() {
        None
    } else {
        serde_json::to_string(state).ok()
    }
}

/// Serializes session-level VMM overrides into the persisted JSON payload when needed.
/// 在需要时将会话级 VMM 覆盖序列化为持久化 JSON 载荷。
pub(super) fn serialize_vmm_config_layer_to_json(layer: &VmmConfigLayer) -> Option<String> {
    if layer.is_empty() {
        None
    } else {
        serde_json::to_string(layer).ok()
    }
}

/// Parses persisted session-level VMM overrides from a JSON payload.
/// 从 JSON 载荷解析持久化的会话级 VMM 覆盖。
pub(super) fn parse_vmm_config_layer_from_json(value: &str) -> Option<VmmConfigLayer> {
    serde_json::from_str(value).ok()
}

/// Serializes the VMM writeback outbox into the persisted JSON payload when needed.
/// 在需要时将 VMM 写回 outbox 序列化为持久化 JSON 载荷。
pub(super) fn serialize_vmm_writeback_outbox_to_json(
    outbox: &[VmmWritebackOutboxEntry],
) -> Option<String> {
    if outbox.is_empty() {
        None
    } else {
        serde_json::to_string(outbox).ok()
    }
}

/// Parses a persisted VMM writeback outbox JSON payload.
/// 解析持久化的 VMM 写回 outbox JSON 载荷。
pub(super) fn parse_vmm_writeback_outbox_from_json(
    value: &str,
) -> Option<Vec<VmmWritebackOutboxEntry>> {
    serde_json::from_str(value).ok()
}

/// Serializes the VMM profile sync state into the persisted JSON payload when needed.
/// 在需要时将 VMM 画像同步状态序列化为持久化 JSON 载荷。
pub(super) fn serialize_vmm_profile_sync_to_json(state: &VmmProfileSyncState) -> Option<String> {
    if state.is_empty() {
        None
    } else {
        serde_json::to_string(state).ok()
    }
}

/// Parses a persisted VMM profile sync JSON payload.
/// 解析持久化的 VMM 画像同步 JSON 载荷。
pub(super) fn parse_vmm_profile_sync_from_json(value: &str) -> Option<VmmProfileSyncState> {
    serde_json::from_str(value).ok()
}

/// Serializes one typed canonical conversation state into the persisted JSON payload when it still carries messages or records.
/// 当规范会话状态仍携带消息或记录时，将其序列化为持久化 JSON 载荷。
pub(super) fn serialize_session_conversation_state_to_json(
    conversation_state: &SessionConversationState,
) -> Option<String> {
    if conversation_state.messages.is_empty()
        && conversation_state.records.is_empty()
        && conversation_state.continuation.provider_id.is_none()
        && conversation_state.continuation.model_id.is_none()
        && conversation_state
            .continuation
            .last_provider_message_id
            .is_none()
        && conversation_state.continuation.updated_at.is_none()
    {
        None
    } else {
        serde_json::to_string(conversation_state).ok()
    }
}

/// Returns one default message title when no explicit title is stored in the metadata payload.
/// 当元数据载荷中未存储显式标题时返回默认消息标题。
pub(super) fn default_message_title(role: &str) -> &'static str {
    match role {
        "user" => "用户指令",
        "assistant" => "AI 输出",
        "error" => "执行失败",
        _ => "运行消息",
    }
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

/// Builds one relative-time label from one persisted Unix-milliseconds text value.
/// 根据一条持久化 Unix 毫秒文本值构建相对时间标签。
pub(super) fn relative_time_label_from_unix_millis_text(value: &str) -> String {
    let parsed = value
        .parse::<i64>()
        .unwrap_or_else(|_| current_unix_millis());
    relative_time_label_from_unix_millis(parsed)
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
    let joined = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(max_lines)
        .collect::<Vec<_>>()
        .join(" / ");
    let normalized = if joined.is_empty() {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        joined
    };

    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        let truncated = normalized
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        format!("{truncated}…")
    }
}
