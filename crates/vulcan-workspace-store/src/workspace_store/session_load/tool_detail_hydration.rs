//! Legacy tool-detail hydration for segmented transcript windows.
//! 分段转录窗口的历史工具详情水合逻辑。

use super::*;
use crate::workspace_store::segmented_session_store::read_runtime_log_lines;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use vulcan_context::{
    RuntimeLogEvent, RuntimeLogEventLine, RuntimeToolResultStatus, SessionMessageKind,
};

/// Hydrates missing hidden tool details for legacy display rows from runtime logs loaded with the window.
/// 从随窗口载入的运行时日志中，为历史显示行补齐缺失的隐藏工具详情。
pub(super) fn augment_window_tool_details_from_runtime_logs(
    session_dir: &Path,
    window: &mut SegmentedWindowMessages,
) -> Result<(), String> {
    let missing_refs = missing_tool_detail_refs(
        window.messages.as_slice(),
        window.tool_detail_messages.as_slice(),
    );
    if missing_refs.is_empty() {
        return Ok(());
    }
    let runtime_lines = read_runtime_lines_for_tool_detail_hydration(session_dir)?;
    if runtime_lines.is_empty() {
        return Ok(());
    }
    let synthesized_details =
        synthesize_missing_tool_detail_messages_from_runtime_lines(&missing_refs, &runtime_lines);
    append_unique_tool_detail_messages(&mut window.tool_detail_messages, synthesized_details);
    Ok(())
}

/// Appends synthesized detail rows without duplicating existing stable identities.
/// 追加合成详情行，同时避免重复已有稳定身份。
fn append_unique_tool_detail_messages(
    tool_detail_messages: &mut Vec<SessionMessageEntry>,
    additional_messages: Vec<SessionMessageEntry>,
) {
    let mut seen_keys = tool_detail_messages
        .iter()
        .map(SessionMessageEntry::stable_merge_key)
        .collect::<BTreeSet<_>>();
    for message in additional_messages {
        if seen_keys.insert(message.stable_merge_key()) {
            tool_detail_messages.push(message);
        }
    }
}

/// Reads active and compacted runtime logs that can still contain full legacy tool results.
/// 读取仍可能包含完整历史工具结果的活动与压缩备份运行时日志。
fn read_runtime_lines_for_tool_detail_hydration(
    session_dir: &Path,
) -> Result<Vec<RuntimeLogEventLine>, String> {
    let mut lines = read_runtime_log_lines(session_dir)?.unwrap_or_default();
    for backup_dir in runtime_backup_session_dirs(session_dir)? {
        if let Some(mut backup_lines) = read_runtime_log_lines(&backup_dir)? {
            lines.append(&mut backup_lines);
        }
    }
    Ok(lines)
}

/// Returns runtime backup directories newest-first when compacted logs exist.
/// 当存在压缩日志备份时，按最新优先返回运行时备份目录。
fn runtime_backup_session_dirs(session_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let backup_root = session_dir.join("runtime_backup");
    let index_path = backup_root.join("index.json");
    let encoded = match fs::read(&index_path) {
        Ok(encoded) => encoded,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "Failed to read runtime backup index {}: {error}",
                index_path.display()
            ));
        }
    };
    let index =
        serde_json::from_slice::<RuntimeBackupDirectoryIndex>(&encoded).map_err(|error| {
            format!(
                "Failed to decode runtime backup index {}: {error}",
                index_path.display()
            )
        })?;
    let mut dirs = Vec::with_capacity(index.directories.len());
    for directory_name in index.directories.into_iter().rev() {
        if directory_name.is_empty()
            || directory_name.contains('/')
            || directory_name.contains('\\')
            || directory_name == "."
            || directory_name == ".."
        {
            return Err(String::from(
                "Runtime backup index contains an invalid directory name.",
            ));
        }
        dirs.push(backup_root.join(directory_name));
    }
    Ok(dirs)
}

/// Ordered runtime-backup directory names loaded from the exact session-local index.
/// 从精确会话本地索引读取的有序运行时备份目录名。
#[derive(Debug, Clone, serde::Deserialize)]
struct RuntimeBackupDirectoryIndex {
    /// Directory names in creation order, oldest first.
    /// 按创建顺序排列的目录名，最旧项在前。
    directories: Vec<String>,
}

/// Describes one visible row whose hidden detail reference is missing from the loaded window.
/// 描述一条可见行，其隐藏详情引用未出现在已载入窗口中。
#[derive(Debug, Clone)]
struct MissingToolDetailRef {
    /// Stable hidden detail record id referenced by the visible tool row.
    /// 可见工具行引用的稳定隐藏详情记录标识。
    record_id: String,
    /// Original detail record index when the visible row recorded one.
    /// 可见行记录的原始详情记录索引。
    message_index: Option<usize>,
    /// Stable tool-call id used to match runtime result events.
    /// 用于匹配运行时结果事件的稳定工具调用标识。
    tool_call_id: String,
    /// User-facing tool name copied from the visible row metadata.
    /// 从可见行元数据复制的面向用户工具名。
    tool_name: Option<String>,
    /// Original tool arguments copied from the visible row metadata.
    /// 从可见行元数据复制的原始工具参数。
    arguments: Option<Value>,
    /// Timestamp used for the synthesized hidden detail row.
    /// 合成隐藏详情行使用的时间戳。
    created_at: String,
}

/// Finds visible tool rows whose detail reference points to a missing hidden detail row.
/// 查找详情引用指向缺失隐藏详情行的可见工具行。
fn missing_tool_detail_refs(
    messages: &[SessionMessageEntry],
    tool_detail_messages: &[SessionMessageEntry],
) -> Vec<MissingToolDetailRef> {
    let mut existing_detail_ids = BTreeSet::new();
    let mut existing_detail_tool_call_ids = BTreeSet::new();
    for message in messages.iter().chain(tool_detail_messages.iter()) {
        if message.record_kind != SessionMessageRecordKind::ToolDetailOutput {
            continue;
        }
        if !message.id.trim().is_empty() {
            existing_detail_ids.insert(message.id.clone());
        }
        if let Some(tool_call_id) =
            session_message_metadata_string(message, &["tool_call_id", "toolCallId"])
        {
            existing_detail_tool_call_ids.insert(tool_call_id.to_string());
        }
    }
    messages
        .iter()
        .filter_map(|message| {
            missing_tool_detail_ref_from_message(
                message,
                &existing_detail_ids,
                &existing_detail_tool_call_ids,
            )
        })
        .collect()
}

/// Extracts a missing detail reference from one visible tool message when repair is needed.
/// 在需要修复时，从单条可见工具消息中提取缺失详情引用。
fn missing_tool_detail_ref_from_message(
    message: &SessionMessageEntry,
    existing_detail_ids: &BTreeSet<String>,
    existing_detail_tool_call_ids: &BTreeSet<String>,
) -> Option<MissingToolDetailRef> {
    if !matches!(
        message.record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    ) {
        return None;
    }
    let metadata = message.metadata.as_ref()?;
    let detail_ref = metadata.get("result")?.get("detail_ref")?;
    let record_id = detail_ref
        .get("record_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let tool_call_id = session_message_metadata_string(message, &["tool_call_id", "toolCallId"])?;
    if existing_detail_ids.contains(record_id)
        || existing_detail_tool_call_ids.contains(tool_call_id)
    {
        return None;
    }
    Some(MissingToolDetailRef {
        record_id: record_id.to_string(),
        message_index: detail_ref
            .get("message_index")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok()),
        tool_call_id: tool_call_id.to_string(),
        tool_name: session_message_metadata_string(message, &["tool_name", "toolName"])
            .map(str::to_string),
        arguments: metadata.get("arguments").cloned(),
        created_at: message.created_at.clone(),
    })
}

/// Synthesizes missing hidden tool-detail rows from runtime tool result events.
/// 根据运行时工具结果事件合成缺失的隐藏工具详情行。
fn synthesize_missing_tool_detail_messages_from_runtime_lines(
    missing_refs: &[MissingToolDetailRef],
    runtime_lines: &[RuntimeLogEventLine],
) -> Vec<SessionMessageEntry> {
    let runtime_results = runtime_tool_results_by_call_id(runtime_lines);
    missing_refs
        .iter()
        .filter_map(|missing_ref| {
            runtime_results
                .get(missing_ref.tool_call_id.as_str())
                .map(|result| synthesized_tool_detail_message(missing_ref, result))
        })
        .collect()
}

/// Indexes runtime tool results by both provider-visible and runtime-owned call ids.
/// 按 Provider 可见与运行时自有调用标识索引运行时工具结果。
fn runtime_tool_results_by_call_id(
    runtime_lines: &[RuntimeLogEventLine],
) -> BTreeMap<&str, &vulcan_context::RuntimeToolResultEvent> {
    let mut results = BTreeMap::new();
    for line in runtime_lines {
        let RuntimeLogEvent::ToolResult(result) = &line.event else {
            continue;
        };
        if let Some(provider_tool_call_id) = result
            .provider_tool_call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            results.entry(provider_tool_call_id).or_insert(result);
        }
        if let Some(runtime_tool_call_id) = result
            .runtime_tool_call_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            results.entry(runtime_tool_call_id).or_insert(result);
        }
    }
    results
}

/// Builds one hidden tool-detail message from a visible-row reference and runtime result event.
/// 根据可见行引用与运行时结果事件构建一条隐藏工具详情消息。
fn synthesized_tool_detail_message(
    missing_ref: &MissingToolDetailRef,
    result: &vulcan_context::RuntimeToolResultEvent,
) -> SessionMessageEntry {
    let mut metadata = serde_json::Map::new();
    metadata.insert(String::from("tool_detail"), Value::Bool(true));
    metadata.insert(
        String::from("tool_call_id"),
        Value::String(missing_ref.tool_call_id.clone()),
    );
    if let Some(tool_name) = missing_ref.tool_name.as_ref() {
        metadata.insert(String::from("tool_name"), Value::String(tool_name.clone()));
    }
    if let Some(arguments) = missing_ref.arguments.as_ref() {
        metadata.insert(String::from("arguments"), arguments.clone());
    }
    metadata.insert(
        String::from("display"),
        synthesized_tool_detail_display_payload(missing_ref, result),
    );
    metadata.insert(
        String::from("detail_chars"),
        Value::Number(serde_json::Number::from(result.text.chars().count() as u64)),
    );
    if let Some(message_index) = missing_ref.message_index {
        metadata.insert(
            String::from("message_index"),
            Value::Number(serde_json::Number::from(message_index as u64)),
        );
    }
    SessionMessageEntry {
        id: missing_ref.record_id.clone(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolDetailOutput,
        visible: false,
        title: String::new(),
        body: result.text.clone(),
        metadata: Some(Value::Object(metadata)),
        created_at: missing_ref.created_at.clone(),
    }
}

/// Builds a conservative structured display payload for synthesized legacy details.
/// 为合成的历史详情构建保守的结构化展示载荷。
fn synthesized_tool_detail_display_payload(
    missing_ref: &MissingToolDetailRef,
    result: &vulcan_context::RuntimeToolResultEvent,
) -> Value {
    let mut display = serde_json::Map::new();
    display.insert(
        String::from("kind"),
        Value::String(
            missing_ref
                .tool_name
                .clone()
                .unwrap_or_else(|| String::from("generic")),
        ),
    );
    display.insert(
        String::from("status"),
        Value::String(runtime_tool_result_status_label(&result.status).to_string()),
    );
    if let Some(arguments) = missing_ref.arguments.as_ref() {
        display.insert(String::from("arguments"), arguments.clone());
    }
    display.insert(
        String::from("summary"),
        Value::String(compact_tool_detail_summary(&result.text)),
    );
    Value::Object(display)
}

/// Returns the persisted display label for one runtime tool-result status.
/// 返回单个运行时工具结果状态的持久化展示标签。
fn runtime_tool_result_status_label(status: &RuntimeToolResultStatus) -> &'static str {
    match status {
        RuntimeToolResultStatus::Completed => "completed",
        RuntimeToolResultStatus::Failed => "failed",
        RuntimeToolResultStatus::Orphan => "completed",
    }
}

/// Builds a short single-line summary for synthesized display metadata.
/// 为合成展示元数据构建短单行摘要。
fn compact_tool_detail_summary(text: &str) -> String {
    const MAX_CHARS: usize = 160;
    let summary = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if summary.chars().count() <= MAX_CHARS {
        return summary;
    }
    summary.chars().take(MAX_CHARS).collect::<String>()
}

/// Extracts the first non-empty string metadata value from one session message.
/// 从单条会话消息中提取第一个非空字符串元数据值。
fn session_message_metadata_string<'a>(
    message: &'a SessionMessageEntry,
    keys: &[&str],
) -> Option<&'a str> {
    let metadata = message.metadata.as_ref()?;
    keys.iter()
        .find_map(|key| metadata.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vulcan_context::RuntimeToolResultEvent;

    /// Builds one visible legacy tool row whose hidden detail is missing from display storage.
    /// 构建一条显示存储缺少隐藏详情的历史可见工具行。
    fn visible_tool_message_with_detail_ref() -> SessionMessageEntry {
        SessionMessageEntry {
            id: String::from("visible-tool-row"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::from("Query file list"),
            body: String::from("No output"),
            metadata: Some(json!({
                "tool_call_id": "call-legacy-1",
                "tool_name": "vulcan_file_list",
                "arguments": {
                    "path": "D:\\openvulcan\\vulcan-code"
                },
                "result": {
                    "detail_ref": {
                        "kind": "session_record",
                        "record_id": "hidden-detail-row",
                        "message_index": 11
                    }
                }
            })),
            created_at: String::from("1783526952985"),
        }
    }

    /// Builds one runtime tool-result line that still carries the full legacy output.
    /// 构建一条仍携带完整历史输出的运行时工具结果行。
    fn runtime_tool_result_line() -> RuntimeLogEventLine {
        RuntimeLogEventLine::new(
            14,
            "1783526954377",
            RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                turn_id: None,
                message_id: String::from("runtime-result-message-1"),
                tool_batch_id: None,
                runtime_tool_call_id: Some(String::from("runtime-call-1")),
                provider_tool_call_id: Some(String::from("call-legacy-1")),
                text: String::from("完整的历史工具输出\nline 2"),
                status: RuntimeToolResultStatus::Completed,
                error: None,
            }),
        )
    }

    /// Verifies missing visible detail references synthesize hidden detail rows from runtime results.
    /// 验证缺失的可见详情引用会从运行时结果合成隐藏详情行。
    #[test]
    fn synthesizes_missing_tool_detail_from_runtime_result() {
        let visible_message = visible_tool_message_with_detail_ref();
        let missing_refs = missing_tool_detail_refs(std::slice::from_ref(&visible_message), &[]);
        let runtime_line = runtime_tool_result_line();

        let synthesized = synthesize_missing_tool_detail_messages_from_runtime_lines(
            &missing_refs,
            &[runtime_line],
        );

        assert_eq!(synthesized.len(), 1);
        let detail = &synthesized[0];
        assert_eq!(detail.id, "hidden-detail-row");
        assert_eq!(
            detail.record_kind,
            SessionMessageRecordKind::ToolDetailOutput
        );
        assert!(!detail.visible);
        assert_eq!(detail.body, "完整的历史工具输出\nline 2");
        let metadata = detail
            .metadata
            .as_ref()
            .and_then(Value::as_object)
            .expect("synthesized detail metadata should be an object");
        assert_eq!(
            metadata.get("tool_call_id").and_then(Value::as_str),
            Some("call-legacy-1")
        );
        assert!(!metadata.contains_key("tool_result_id"));
        assert_eq!(
            metadata
                .get("display")
                .and_then(Value::as_object)
                .and_then(|display| display.get("kind"))
                .and_then(Value::as_str),
            Some("vulcan_file_list")
        );
    }

    /// Verifies existing hidden detail rows prevent duplicate legacy synthesis.
    /// 验证已存在的隐藏详情行会阻止重复合成历史详情。
    #[test]
    fn existing_tool_detail_suppresses_synthesis() {
        let visible_message = visible_tool_message_with_detail_ref();
        let existing_detail = SessionMessageEntry {
            id: String::from("hidden-detail-row"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolDetailOutput,
            visible: false,
            title: String::new(),
            body: String::from("already loaded"),
            metadata: Some(json!({
                "tool_call_id": "call-legacy-1"
            })),
            created_at: String::from("1783526953854"),
        };

        let missing_refs = missing_tool_detail_refs(&[visible_message], &[existing_detail]);

        assert!(missing_refs.is_empty());
    }
}
