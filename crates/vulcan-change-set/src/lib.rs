//! Host-side helpers for structured `change_set` tool results.
//! 结构化 `change_set` 工具结果的宿主侧辅助逻辑。

mod persistent;
mod types;
mod verification;

#[cfg(test)]
mod tests;

pub use persistent::{
    PersistentChangeSet, PersistentChangeSetDiagnostic, PersistentChangeSetError,
    PersistentChangeSetFile, PersistentChangeSetHunk, PersistentChangeSetLine,
    PersistentLifecycleContent, PersistentTextFormat, persistent_change_set_from_host_result,
    persistent_change_set_stats, validate_persistent_change_set,
};
use serde_json::{Map, Value, json};
pub use types::{
    ChangeSetDeleteContentMode, ChangeSetDiagnosticSummary, ChangeSetFileChangeKind,
    ChangeSetFileSummary, ChangeSetModalSummary, ChangeSetMode, ChangeSetStats,
    ChangeSetVerificationStatus, ChangeSetVerificationSummary,
};
use types::{RawChangeSetFileRecord, RawChangeSetHunk, RawChangeSetLineEntry};
use verification::verify_change_set_records;

/// Stable structured host-result kind used by IDE-style file edit tools.
/// IDE 风格文件编辑工具使用的稳定结构化宿主结果类型。
pub const CHANGE_SET_HOST_RESULT_KIND: &str = "change_set";

/// Shared payload byte ceiling announced to LuaSkills so file-edit tools can opt into structured host results without flooding the host metadata channel.
/// 向 LuaSkills 公布的共享载荷字节上限，使文件编辑工具可以启用结构化宿主结果，同时避免淹没宿主元数据通道。
pub const LUASKILLS_CHANGE_SET_HOST_RESULT_MAX_PAYLOAD_BYTES: u64 = 512 * 1024;

/// Maximum retained size of one native change-set extension after compaction.
/// 压缩后单个原生变更集扩展允许保留的最大大小。
const NATIVE_CHANGE_SET_EXTENSION_MAX_BYTES: usize = 128 * 1024;

/// Builds the host-result capability payload injected into LuaSkills request contexts.
/// 构建注入到 LuaSkills 请求上下文中的宿主结果能力载荷。
pub fn luaskills_change_set_host_result_capabilities() -> Value {
    json!({
        "host_result": {
            "enabled": true,
            "allowed_kinds": [CHANGE_SET_HOST_RESULT_KIND],
            "max_payload_bytes": LUASKILLS_CHANGE_SET_HOST_RESULT_MAX_PAYLOAD_BYTES,
        }
    })
}

/// Compacts one raw `change_set` host-result object into the minimal shape needed by transcript projection, persistence, and tool-modal summaries.
/// 将一份原始 `change_set` 宿主结果对象压缩成转录投影、持久化与工具弹层摘要所需的最小形态。
pub fn compact_change_set_host_result(host_result: &Value) -> Option<Value> {
    let object = host_result.as_object()?;
    if object.get("kind").and_then(Value::as_str) != Some(CHANGE_SET_HOST_RESULT_KIND) {
        return None;
    }
    let payload = object.get("payload")?;
    let compact_payload = compact_change_set_payload(payload)?;
    Some(json!({
        "kind": CHANGE_SET_HOST_RESULT_KIND,
        "payload": compact_payload,
    }))
}

/// Derives one modal-ready `change_set` summary from either a raw producer payload or an already-compacted host payload.
/// 从原始产出载荷或已压缩宿主载荷中派生一份可供弹层使用的 `change_set` 摘要。
pub fn change_set_modal_summary_from_payload(payload: &Value) -> Option<ChangeSetModalSummary> {
    let compact_payload = compact_change_set_payload(payload)?;
    let object = compact_payload.as_object()?;
    let mode = ChangeSetMode::parse(object.get("mode")?.as_str()?)?;
    let summary = object
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let stats = parse_compact_stats(object.get("stats")?)?;
    let files = object
        .get("files")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_compact_file_summary)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let diagnostics = object
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_compact_diagnostic_summary)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let verification = parse_compact_verification(object.get("verification")?)?;
    Some(ChangeSetModalSummary {
        mode,
        summary,
        stats,
        files,
        diagnostics,
        verification,
    })
}

/// Builds one English host-side warning notice from the current raw `change_set` payload when the just-applied filesystem state looks suspicious.
/// 当当前原始 `change_set` 载荷对应的刚应用文件系统状态看起来可疑时，构建一条英文宿主侧警告提示。
pub fn change_set_host_verification_notice(payload: &Value) -> Option<String> {
    let object = payload.as_object()?;
    let mode = ChangeSetMode::parse(object.get("mode")?.as_str()?)?;
    if mode != ChangeSetMode::Applied {
        return None;
    }
    let raw_files = object.get("files").and_then(Value::as_array)?;
    let files = raw_files
        .iter()
        .filter_map(parse_raw_file_record)
        .collect::<Vec<_>>();
    if files.is_empty() && !raw_files.is_empty() {
        return None;
    }
    let verification = verify_change_set_records(mode, &files);
    if verification.status != ChangeSetVerificationStatus::Warning {
        return None;
    }
    let mut lines = vec![String::from(
        "Host verification warning: the reported applied change_set could not be fully verified against the current filesystem state.",
    )];
    for warning in verification.warnings.iter().take(6) {
        lines.push(format!("- {warning}"));
    }
    Some(lines.join("\n"))
}

/// Compacts one raw or already-compacted payload into the stable compact payload shape reused by transcript projection and persistence without retaining mutable filesystem verification state.
/// 将原始载荷或已压缩载荷规范化为稳定的紧凑载荷形态，供转录投影与持久化复用，且不保留会随文件变化而漂移的文件系统校验状态。
fn compact_change_set_payload(payload: &Value) -> Option<Value> {
    let object = payload.as_object()?;
    if object.get("stats").is_some()
        && object.get("verification").is_some()
        && object.get("files").and_then(Value::as_array).is_some()
        && object
            .get("mode")
            .and_then(Value::as_str)
            .and_then(ChangeSetMode::parse)
            .is_some()
    {
        let mode = ChangeSetMode::parse(object.get("mode")?.as_str()?)?;
        let mut compact = object.clone();
        compact.remove("extensions");
        if let Some(extensions) = compact_extensions(object.get("extensions")) {
            compact.insert(String::from("extensions"), extensions);
        }
        compact.insert(
            String::from("verification"),
            compact_verification_value(&build_persisted_verification_summary(mode)),
        );
        return Some(Value::Object(compact));
    }

    let mode = ChangeSetMode::parse(object.get("mode")?.as_str()?)?;
    let raw_files = object.get("files").and_then(Value::as_array)?;
    let files = raw_files
        .iter()
        .filter_map(parse_raw_file_record)
        .collect::<Vec<_>>();
    if files.is_empty() && !raw_files.is_empty() {
        return None;
    }

    let stats = build_change_set_stats(&files);
    let verification = build_persisted_verification_summary(mode);
    let compact_files = files.iter().map(compact_file_record).collect::<Vec<_>>();
    let diagnostics = compact_diagnostic_values(object.get("diagnostics"));

    let mut compact = Map::new();
    compact.insert(
        String::from("mode"),
        Value::String(mode.as_str().to_string()),
    );
    if let Some(summary) = object
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact.insert(String::from("summary"), Value::String(summary.to_string()));
    }
    compact.insert(String::from("files"), Value::Array(compact_files));
    if let Some(extensions) = compact_extensions(object.get("extensions")) {
        compact.insert(String::from("extensions"), extensions);
    }
    compact.insert(String::from("stats"), compact_stats_value(&stats));
    if !diagnostics.is_empty() {
        compact.insert(String::from("diagnostics"), Value::Array(diagnostics));
    }
    compact.insert(
        String::from("verification"),
        compact_verification_value(&verification),
    );
    Some(Value::Object(compact))
}

/// Builds the stable verification snapshot persisted with compact `change_set` payloads so later transcript rendering never reinterprets mutable filesystem drift as a historical tool failure.
/// 构建与紧凑 `change_set` 载荷一同持久化的稳定校验快照，避免后续转录渲染把可变文件系统漂移误解成历史工具失败。
fn build_persisted_verification_summary(_mode: ChangeSetMode) -> ChangeSetVerificationSummary {
    ChangeSetVerificationSummary {
        status: ChangeSetVerificationStatus::Skipped,
        checked_file_count: 0,
        warnings: Vec::new(),
    }
}

/// Parses one raw canonical file record emitted by the producing tool.
/// 解析一条由产出工具发出的原始 canonical 文件记录。
fn parse_raw_file_record(file: &Value) -> Option<RawChangeSetFileRecord> {
    let object = file.as_object()?;
    let change = ChangeSetFileChangeKind::parse(object.get("change")?.as_str()?)?;
    let resource_id = compact_optional_string(object.get("resource_id"));
    let path = compact_optional_string(object.get("path"));
    let old_path = compact_optional_string(object.get("old_path"));
    let new_path = compact_optional_string(object.get("new_path"));
    let content = object
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_string);
    let delete_content_mode = if change == ChangeSetFileChangeKind::Delete {
        ChangeSetDeleteContentMode::parse(object.get("content_mode"))
    } else {
        None
    };
    let total_line_count = compact_optional_usize(object.get("total_line_count"));
    let hunks = object
        .get("hunks")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_raw_change_set_hunk)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(RawChangeSetFileRecord {
        resource_id,
        change,
        path,
        old_path,
        new_path,
        content,
        delete_content_mode,
        total_line_count,
        hunks,
        patch_present: object
            .get("patch")
            .is_some_and(|value| !value.is_null() && value.is_string()),
    })
}

/// Parses one raw canonical modify hunk emitted by the producing tool.
/// 解析一条由产出工具发出的原始 canonical modify hunk。
fn parse_raw_change_set_hunk(hunk: &Value) -> Option<RawChangeSetHunk> {
    let object = hunk.as_object()?;
    Some(RawChangeSetHunk {
        before: object
            .get("before")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        after: object
            .get("after")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        deleted: parse_raw_line_entries(object.get("delete")),
        inserted: parse_raw_line_entries(object.get("insert")),
    })
}

/// Parses one ordered delete/insert line-entry array.
/// 解析一组有序的 delete/insert 行记录数组。
fn parse_raw_line_entries(value: Option<&Value>) -> Vec<RawChangeSetLineEntry> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| {
                    let object = entry.as_object()?;
                    Some(RawChangeSetLineEntry {
                        line: object
                            .get("line")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok())?,
                        content: object.get("content").and_then(Value::as_str)?.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Aggregates raw file records into one compact counter snapshot.
/// 将原始文件记录聚合为一份紧凑计数快照。
fn build_change_set_stats(files: &[RawChangeSetFileRecord]) -> ChangeSetStats {
    let mut stats = ChangeSetStats {
        file_count: files.len(),
        create_count: 0,
        modify_count: 0,
        delete_count: 0,
        rename_count: 0,
        inserted_line_count: 0,
        deleted_line_count: 0,
    };
    for file in files {
        match file.change {
            ChangeSetFileChangeKind::Create => stats.create_count += 1,
            ChangeSetFileChangeKind::Modify => stats.modify_count += 1,
            ChangeSetFileChangeKind::Delete => stats.delete_count += 1,
            ChangeSetFileChangeKind::Rename => stats.rename_count += 1,
        }
        for hunk in &file.hunks {
            stats.inserted_line_count += hunk.inserted.len();
            stats.deleted_line_count += hunk.deleted.len();
        }
    }
    stats
}

/// Converts one raw file record into the compact JSON row persisted by the host.
/// 将一条原始文件记录转换成由宿主持久化的紧凑 JSON 行。
fn compact_file_record(file: &RawChangeSetFileRecord) -> Value {
    let mut object = Map::new();
    object.insert(
        String::from("change"),
        Value::String(file.change.as_str().to_string()),
    );
    if let Some(path) = file.path.as_ref() {
        object.insert(String::from("path"), Value::String(path.clone()));
    }
    if let Some(resource_id) = file.resource_id.as_ref() {
        object.insert(
            String::from("resource_id"),
            Value::String(resource_id.clone()),
        );
    }
    if let Some(old_path) = file.old_path.as_ref() {
        object.insert(String::from("old_path"), Value::String(old_path.clone()));
    }
    if let Some(new_path) = file.new_path.as_ref() {
        object.insert(String::from("new_path"), Value::String(new_path.clone()));
    }
    let inserted_line_count = file
        .hunks
        .iter()
        .map(|hunk| hunk.inserted.len())
        .sum::<usize>();
    let deleted_line_count = file
        .hunks
        .iter()
        .map(|hunk| hunk.deleted.len())
        .sum::<usize>();
    if !file.hunks.is_empty() {
        object.insert(String::from("hunk_count"), json!(file.hunks.len()));
    }
    if inserted_line_count > 0 {
        object.insert(
            String::from("inserted_line_count"),
            json!(inserted_line_count),
        );
    }
    if deleted_line_count > 0 {
        object.insert(
            String::from("deleted_line_count"),
            json!(deleted_line_count),
        );
    }
    match file.change {
        ChangeSetFileChangeKind::Create => {
            if let Some(content) = file.content.as_deref() {
                let normalized_content = normalize_text(content);
                object.insert(
                    String::from("content_bytes"),
                    json!(normalized_content.len()),
                );
                object.insert(
                    String::from("content_lines"),
                    json!(count_change_set_content_lines(&normalized_content)),
                );
            }
        }
        ChangeSetFileChangeKind::Delete => {
            let delete_content_mode = file
                .delete_content_mode
                .unwrap_or(ChangeSetDeleteContentMode::Full);
            object.insert(
                String::from("content_mode"),
                Value::String(delete_content_mode.as_str().to_string()),
            );
            if let Some(total_line_count) = file.total_line_count {
                object.insert(String::from("content_lines"), json!(total_line_count));
            }
            if delete_content_mode == ChangeSetDeleteContentMode::Full
                && let Some(content) = file.content.as_deref()
            {
                let normalized_content = normalize_text(content);
                object.insert(
                    String::from("content_bytes"),
                    json!(normalized_content.len()),
                );
                if file.total_line_count.is_none() {
                    object.insert(
                        String::from("content_lines"),
                        json!(count_change_set_content_lines(&normalized_content)),
                    );
                }
            }
        }
        ChangeSetFileChangeKind::Modify | ChangeSetFileChangeKind::Rename => {}
    }
    if file.patch_present {
        object.insert(String::from("patch_present"), Value::Bool(true));
    }
    Value::Object(object)
}

/// Parses one compact file row back into the typed summary used by the tool modal.
/// 将一条紧凑文件行解析回工具弹层使用的带类型摘要。
fn parse_compact_file_summary(file: &Value) -> Option<ChangeSetFileSummary> {
    let object = file.as_object()?;
    let change = ChangeSetFileChangeKind::parse(object.get("change")?.as_str()?)?;
    Some(ChangeSetFileSummary {
        resource_id: compact_optional_string(object.get("resource_id")),
        change,
        path: compact_optional_string(object.get("path")),
        old_path: compact_optional_string(object.get("old_path")),
        new_path: compact_optional_string(object.get("new_path")),
        hunk_count: compact_usize(object.get("hunk_count")),
        inserted_line_count: compact_usize(object.get("inserted_line_count")),
        deleted_line_count: compact_usize(object.get("deleted_line_count")),
        content_bytes: compact_optional_usize(object.get("content_bytes")),
        content_lines: compact_optional_usize(object.get("content_lines")),
        delete_content_mode: if change == ChangeSetFileChangeKind::Delete {
            ChangeSetDeleteContentMode::parse(object.get("content_mode"))
        } else {
            None
        },
        patch_present: object
            .get("patch_present")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Retains only the bounded, registered native extension namespace supported by the shared host contract.
/// 仅保留共享宿主契约支持且经过大小限制的已注册原生扩展命名空间。
fn compact_extensions(value: Option<&Value>) -> Option<Value> {
    let object = value?.as_object()?;
    let mut extensions = Map::new();
    if let Some(native_v1) = object.get("vulcan.file_edit.v1").and_then(Value::as_object)
        && let Some(retained) = compact_registered_extension(
            native_v1,
            &[("operation_results", "array"), ("transaction", "object")],
        )
    {
        extensions.insert(String::from("vulcan.file_edit.v1"), retained);
    }
    if let Some(native_v2) = object
        .get("vulcan.file_tools.v2")
        .and_then(Value::as_object)
        && let Some(retained) = compact_registered_extension(
            native_v2,
            &[
                ("source_tool", "string"),
                ("operation_results", "array"),
                ("resources", "array"),
                ("transaction", "object"),
                ("encoding", "string"),
                ("bom", "boolean"),
                ("line_ending", "string"),
            ],
        )
    {
        extensions.insert(String::from("vulcan.file_tools.v2"), retained);
    }
    if extensions.is_empty() {
        return None;
    }
    let extensions = Value::Object(extensions);
    let encoded = serde_json::to_vec(&extensions).ok()?;
    (encoded.len() <= NATIVE_CHANGE_SET_EXTENSION_MAX_BYTES).then_some(extensions)
}

/// Retains typed top-level fields from one registered native extension object.
/// 从一个已注册原生扩展对象中保留类型正确的顶层字段。
fn compact_registered_extension(
    extension: &Map<String, Value>,
    allowed_fields: &[(&str, &str)],
) -> Option<Value> {
    let mut retained = Map::new();
    for (field, expected_type) in allowed_fields {
        let Some(value) = extension.get(*field) else {
            continue;
        };
        let accepted = match *expected_type {
            "array" => value.is_array(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "string" => value.is_string(),
            _ => false,
        };
        if accepted {
            retained.insert((*field).to_string(), value.clone());
        }
    }
    (!retained.is_empty()).then_some(Value::Object(retained))
}

/// Compacts one optional diagnostics array into the host-owned minimal JSON shape.
/// 将可选 diagnostics 数组压缩为宿主自有的最小 JSON 形态。
fn compact_diagnostic_values(value: Option<&Value>) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|diagnostic| {
                    let object = diagnostic.as_object()?;
                    let level = compact_optional_string(object.get("level"))?;
                    let message = compact_optional_string(object.get("message"))?;
                    Some(json!({
                        "level": level,
                        "message": message,
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Parses one compact diagnostic row into the typed modal summary shape.
/// 将一条紧凑诊断行解析为带类型的弹层摘要形态。
fn parse_compact_diagnostic_summary(value: &Value) -> Option<ChangeSetDiagnosticSummary> {
    let object = value.as_object()?;
    Some(ChangeSetDiagnosticSummary {
        level: compact_optional_string(object.get("level"))?,
        message: compact_optional_string(object.get("message"))?,
    })
}

/// Converts one counter snapshot into the compact JSON shape persisted by the host.
/// 将一份计数快照转换成由宿主持久化的紧凑 JSON 形态。
fn compact_stats_value(stats: &ChangeSetStats) -> Value {
    json!({
        "file_count": stats.file_count,
        "create_count": stats.create_count,
        "modify_count": stats.modify_count,
        "delete_count": stats.delete_count,
        "rename_count": stats.rename_count,
        "inserted_line_count": stats.inserted_line_count,
        "deleted_line_count": stats.deleted_line_count,
    })
}

/// Parses one compact stats object back into the typed summary shape.
/// 将一份紧凑 stats 对象解析回带类型的摘要形态。
fn parse_compact_stats(value: &Value) -> Option<ChangeSetStats> {
    let object = value.as_object()?;
    Some(ChangeSetStats {
        file_count: compact_usize(object.get("file_count")),
        create_count: compact_usize(object.get("create_count")),
        modify_count: compact_usize(object.get("modify_count")),
        delete_count: compact_usize(object.get("delete_count")),
        rename_count: compact_usize(object.get("rename_count")),
        inserted_line_count: compact_usize(object.get("inserted_line_count")),
        deleted_line_count: compact_usize(object.get("deleted_line_count")),
    })
}

/// Converts one host-side verification snapshot into the compact JSON shape persisted by the host.
/// 将一份宿主侧校验快照转换成由宿主持久化的紧凑 JSON 形态。
fn compact_verification_value(verification: &ChangeSetVerificationSummary) -> Value {
    json!({
        "status": verification.status.as_str(),
        "checked_file_count": verification.checked_file_count,
        "warning_count": verification.warnings.len(),
        "warnings": verification.warnings,
    })
}

/// Parses one compact verification object back into the typed summary shape.
/// 将一份紧凑校验对象解析回带类型的摘要形态。
fn parse_compact_verification(value: &Value) -> Option<ChangeSetVerificationSummary> {
    let object = value.as_object()?;
    let warnings = object
        .get("warnings")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(ChangeSetVerificationSummary {
        status: ChangeSetVerificationStatus::parse(object.get("status")?.as_str()?)?,
        checked_file_count: compact_usize(object.get("checked_file_count")),
        warnings,
    })
}

/// Returns one trimmed non-empty string clone when present.
/// 当存在时返回一份去首尾空白后的非空字符串克隆。
fn compact_optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Returns one required-or-zero unsigned counter value from compact JSON.
/// 从紧凑 JSON 中返回一条必填或默认零的无符号计数值。
fn compact_usize(value: Option<&Value>) -> usize {
    compact_optional_usize(value).unwrap_or(0)
}

/// Returns one optional unsigned counter value from compact JSON.
/// 从紧凑 JSON 中返回一条可选无符号计数值。
fn compact_optional_usize(value: Option<&Value>) -> Option<usize> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

/// Normalizes mixed newline sequences into `\n` so textual comparisons stay deterministic across platforms.
/// 将混合换行序列规范化为 `\n`，使跨平台文本比较保持确定性。
/// Pairs are read left to right like the host text document, so `\n\r` is one break, as `\r\n` is.
/// 换行对按与宿主文本文档相同的从左到右方式读取，因此 `\n\r` 与 `\r\n` 一样算一个换行。
fn normalize_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(current) = chars.next() {
        match current {
            '\r' => {
                chars.next_if_eq(&'\n');
                normalized.push('\n');
            }
            '\n' => {
                chars.next_if_eq(&'\r');
                normalized.push('\n');
            }
            other => normalized.push(other),
        }
    }
    normalized
}

/// Splits normalized text into logical lines while preserving empty trailing results when they matter for line counts.
/// 将规范化后的文本拆分为逻辑行，并在影响行数时保留尾部空结果。
fn split_normalized_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').map(str::to_string).collect()
    }
}

/// Counts logical lifecycle-content lines while ignoring one purely trailing newline marker.
/// 统计生命周期内容的逻辑行数，并忽略一个纯粹表示文件结尾的尾随换行。
fn count_change_set_content_lines(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut line_count = text.split('\n').count();
    if text.ends_with('\n') {
        line_count = line_count.saturating_sub(1);
    }
    line_count
}
