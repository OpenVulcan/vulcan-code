use super::*;

pub(super) struct ToolDetailRecordRef {
    /// Stable hidden session record id.
    /// 稳定隐藏会话记录标识。
    pub(super) record_id: String,
    /// Zero-based message index inside the current conversation record list.
    /// 当前会话记录列表中的零基消息索引。
    pub(super) message_index: usize,
}

/// Stores the full tool-result detail outside the visible transcript stream and returns its record id.
/// 将完整工具返回详情存放在可见转录流之外，并返回对应记录标识。
pub(super) fn append_tool_result_detail_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    tool_call_id: &str,
    tool_name: Option<&str>,
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    detail: &str,
) -> Result<Option<ToolDetailRecordRef>, String> {
    let model_output = tool_result_model_output_from_detail(detail);
    if model_output.trim().is_empty() {
        return Ok(None);
    }
    let display = tool_result_display_payload(tool_name, display_state, metadata, &model_output);
    let mut metadata_object = serde_json::Map::new();
    metadata_object.insert(String::from("tool_detail"), serde_json::Value::Bool(true));
    metadata_object.insert(
        String::from("tool_call_id"),
        serde_json::Value::String(tool_call_id.to_string()),
    );
    if let Some(tool_name) = tool_name.filter(|value| !value.trim().is_empty()) {
        metadata_object.insert(
            String::from("tool_name"),
            serde_json::Value::String(tool_name.to_string()),
        );
    }
    if let Some(arguments) = metadata.and_then(|metadata| metadata.get("arguments")) {
        metadata_object.insert(String::from("arguments"), arguments.clone());
    }
    metadata_object.insert(String::from("display"), display);
    metadata_object.insert(
        String::from("detail_chars"),
        serde_json::Value::Number(serde_json::Number::from(model_output.chars().count() as u64)),
    );
    let message_index = runtime_state.conversation_state.records.len();
    let record_id = build_session_record_message_id(session_id, now, message_index);
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolDetailOutput,
            visible: false,
            title: String::new(),
            body: model_output,
            metadata: Some(serde_json::Value::Object(metadata_object)),
            created_at: now.to_string(),
        },
    )
    .map_err(|error| format!("Invalid tool detail record identity: {error}"))?;
    Ok(Some(ToolDetailRecordRef {
        record_id,
        message_index,
    }))
}

/// Extracts the model-facing tool result text from the host detail wrapper.
/// 从宿主详情包装中提取面向模型的工具返回文本。
pub(super) fn tool_result_model_output_from_detail(detail: &str) -> String {
    const MARKER: &str = "## Tool Result";
    let output = detail
        .find(MARKER)
        .map(|index| &detail[index + MARKER.len()..])
        .unwrap_or(detail)
        .trim_start_matches(['\r', '\n'])
        .trim();
    output.to_string()
}

/// Builds the structured display payload used by WebUI tool-specific renderers.
/// 构建 WebUI 工具专属渲染器使用的结构化展示载荷。
pub(super) fn tool_result_display_payload(
    tool_name: Option<&str>,
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    model_output: &str,
) -> serde_json::Value {
    if tool_name == Some("shell_command") {
        return shell_command_display_payload(display_state, metadata, model_output);
    }
    if tool_name == Some("vulcan_file_list") {
        return vulcan_file_list_display_payload(display_state, metadata, model_output);
    }
    if tool_name == Some("ask") {
        return ask_question_display_payload(display_state, metadata, model_output);
    }
    let mut object = serde_json::Map::new();
    object.insert(
        String::from("kind"),
        serde_json::Value::String(tool_name.unwrap_or("generic").to_string()),
    );
    object.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    if let Some(arguments) = metadata.and_then(|metadata| metadata.get("arguments")) {
        object.insert(String::from("arguments"), arguments.clone());
    }
    object.insert(
        String::from("summary"),
        serde_json::Value::String(compact_display_summary(model_output)),
    );
    serde_json::Value::Object(object)
}

/// Builds the structured ask display payload from the native ask host result.
/// 从原生 ask 宿主结果中构建结构化提问展示载荷。
pub(super) fn ask_question_display_payload(
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    model_output: &str,
) -> serde_json::Value {
    if let Some(display) = ask_question_host_display(metadata) {
        let mut display = display.clone();
        if let serde_json::Value::Object(object) = &mut display {
            object.insert(
                String::from("status"),
                serde_json::Value::String(display_state.to_string()),
            );
            object.entry(String::from("summary")).or_insert_with(|| {
                serde_json::Value::String(compact_display_summary(model_output))
            });
        }
        return display;
    }
    let mut object = serde_json::Map::new();
    object.insert(
        String::from("kind"),
        serde_json::Value::String(String::from("ask")),
    );
    object.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    if let Some(arguments) = metadata.and_then(|metadata| metadata.get("arguments")) {
        object.insert(String::from("arguments"), arguments.clone());
    }
    object.insert(
        String::from("summary"),
        serde_json::Value::String(compact_display_summary(model_output)),
    );
    serde_json::Value::Object(object)
}

/// Extracts the workbench-only ask display contract from one tool-finished metadata object.
/// 从单条工具完成元数据中提取仅供工作台使用的 ask 展示契约。
pub(super) fn ask_question_host_display(
    metadata: Option<&serde_json::Value>,
) -> Option<&serde_json::Value> {
    let payload = host_result_payload_by_kind(metadata?, "ask")?;
    payload.get("display").filter(|display| {
        display.get("kind").and_then(serde_json::Value::as_str) == Some("ask_question")
    })
}

/// Extracts a host-result payload by kind from nested or flattened runtime metadata.
/// 从嵌套或扁平运行时元数据中按类型提取宿主结果载荷。
pub(super) fn host_result_payload_by_kind<'a>(
    metadata: &'a serde_json::Value,
    expected_kind: &str,
) -> Option<&'a serde_json::Value> {
    let nested = metadata
        .get("host_result")
        .or_else(|| metadata.get("hostResult"))
        .filter(|host_result| {
            host_result.get("kind").and_then(serde_json::Value::as_str) == Some(expected_kind)
        })
        .and_then(|host_result| host_result.get("payload"));
    nested.or_else(|| {
        (metadata.get("kind").and_then(serde_json::Value::as_str) == Some(expected_kind))
            .then(|| metadata.get("payload"))
            .flatten()
    })
}

/// Builds a compact structured display payload for the file-list tool.
/// 为文件列表工具构建紧凑结构化展示载荷。
pub(super) fn vulcan_file_list_display_payload(
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    model_output: &str,
) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert(
        String::from("kind"),
        serde_json::Value::String(String::from("vulcan_file_list")),
    );
    object.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    if let Some(arguments) = metadata.and_then(|metadata| metadata.get("arguments")) {
        object.insert(String::from("arguments"), arguments.clone());
    }
    if let Some(first_line) = model_output.lines().next() {
        merge_file_list_header_fields(&mut object, first_line);
    }
    let files = model_output
        .lines()
        .skip(1)
        .flat_map(|line| line.split_whitespace())
        .filter(|value| !value.trim().is_empty())
        .map(|value| serde_json::Value::String(value.to_string()))
        .collect::<Vec<_>>();
    if !files.is_empty() {
        object.insert(String::from("files"), serde_json::Value::Array(files));
    }
    object.insert(
        String::from("summary"),
        serde_json::Value::String(compact_display_summary(model_output)),
    );
    serde_json::Value::Object(object)
}

/// Merges known `vulcan_file_list` header counters into one display object.
/// 将已知的 `vulcan_file_list` 头部计数字段合并到展示对象。
pub(super) fn merge_file_list_header_fields(
    object: &mut serde_json::Map<String, serde_json::Value>,
    line: &str,
) {
    let header = line.trim().trim_start_matches('[').trim_end_matches(']');
    if let Some((label, rest)) = header.split_once(':') {
        object.insert(
            String::from("label"),
            serde_json::Value::String(label.trim().to_string()),
        );
        let mut pending_key: Option<String> = None;
        for token in rest.split_whitespace() {
            if let Some((key, value)) = token.split_once(':') {
                insert_file_list_header_field(object, key, value);
                pending_key = None;
            } else if pending_key.is_none() {
                object.insert(
                    String::from("root"),
                    serde_json::Value::String(token.to_string()),
                );
                pending_key = Some(String::from("root"));
            }
        }
    }
}

/// Inserts one typed file-list header field into the display object.
/// 向展示对象写入单个带类型的文件列表头部字段。
pub(super) fn insert_file_list_header_field(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &str,
) {
    let normalized_key = match key {
        "Pattern" => "pattern",
        "Recursive" => "recursive",
        "Ignore" => "ignore",
        "Files" => "files_count",
        "Truncated" => "truncated",
        "Ignored" => "ignored_count",
        _ => return,
    };
    let typed_value = match value {
        "true" | "True" => serde_json::Value::Bool(true),
        "false" | "False" => serde_json::Value::Bool(false),
        _ => value
            .parse::<u64>()
            .map(|number| serde_json::Value::Number(serde_json::Number::from(number)))
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
    };
    object.insert(normalized_key.to_string(), typed_value);
}

/// Builds a bounded one-line display summary without duplicating the full model output.
/// 构建有界单行展示摘要，避免重复保存完整模型输出。
pub(super) fn compact_display_summary(source: &str) -> String {
    const MAX_SUMMARY_CHARS: usize = 240;
    let mut summary = String::new();
    let mut summary_chars = 0usize;
    // Build the compact preview directly and stop once the visible limit is reached.
    // 直接构建紧凑预览，并在达到可见长度上限后停止。
    for word in source.split_whitespace() {
        let separator_chars = if summary.is_empty() { 0 } else { 1 };
        let word_chars = word.chars().count();
        let next_chars = summary_chars
            .saturating_add(separator_chars)
            .saturating_add(word_chars);
        if next_chars > MAX_SUMMARY_CHARS {
            if separator_chars == 1 && summary_chars < MAX_SUMMARY_CHARS {
                summary.push(' ');
                summary_chars += 1;
            }
            let remaining_chars = MAX_SUMMARY_CHARS.saturating_sub(summary_chars);
            summary.extend(word.chars().take(remaining_chars));
            summary.push_str("...");
            return summary;
        }
        if separator_chars == 1 {
            summary.push(' ');
            summary_chars += 1;
        }
        summary.push_str(word);
        summary_chars += word_chars;
    }
    summary
}

/// Parses a shell-command model output into command, status, and stream fields for display.
/// 将 shell_command 模型输出解析为命令、状态和输出流字段用于展示。
pub(super) fn shell_command_display_payload(
    display_state: &str,
    metadata: Option<&serde_json::Value>,
    model_output: &str,
) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert(
        String::from("kind"),
        serde_json::Value::String(String::from("shell_command")),
    );
    object.insert(
        String::from("status"),
        serde_json::Value::String(display_state.to_string()),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut section: Option<&str> = None;
    for line in model_output.lines() {
        let trimmed = line.trim();
        match trimmed {
            "[stdout]" => {
                section = Some("stdout");
                continue;
            }
            "[stderr]" => {
                section = Some("stderr");
                continue;
            }
            _ => {}
        }
        match section {
            Some("stdout") => stdout.push(line),
            Some("stderr") => stderr.push(line),
            _ => {
                if let Some((key, value)) = line.split_once(": ") {
                    let normalized_key = match key.trim() {
                        "Command" => Some("command"),
                        "Shell" => Some("shell"),
                        "Working directory" => Some("working_directory"),
                        "Status" => Some("status"),
                        "Duration" => Some("duration"),
                        "Exit code" => Some("exit_code"),
                        _ => None,
                    };
                    if let Some(normalized_key) = normalized_key {
                        if normalized_key == "exit_code"
                            && let Ok(exit_code) = value.trim().parse::<i64>()
                        {
                            object.insert(
                                normalized_key.to_string(),
                                serde_json::Value::Number(serde_json::Number::from(exit_code)),
                            );
                            continue;
                        }
                        object.insert(
                            normalized_key.to_string(),
                            serde_json::Value::String(value.trim().to_string()),
                        );
                    }
                }
            }
        }
    }
    if !stdout.is_empty() {
        object.insert(
            String::from("stdout"),
            serde_json::Value::String(stdout.join("\n").trim().to_string()),
        );
    }
    if !stderr.is_empty() {
        object.insert(
            String::from("stderr"),
            serde_json::Value::String(stderr.join("\n").trim().to_string()),
        );
    }
    merge_shell_command_rtk_display_fields(&mut object, metadata);
    serde_json::Value::Object(object)
}

/// Merges structured RTK routing status from the command_run host payload into the shell display.
/// 将 command_run 宿主载荷中的结构化 RTK 路由状态合并进 shell 展示。
pub(super) fn merge_shell_command_rtk_display_fields(
    object: &mut serde_json::Map<String, serde_json::Value>,
    metadata: Option<&serde_json::Value>,
) {
    let Some(payload) =
        metadata.and_then(|metadata| host_result_payload_by_kind(metadata, "command_run"))
    else {
        return;
    };
    let Some(rtk) = payload.get("rtk").filter(|value| value.is_object()) else {
        return;
    };
    object.insert(String::from("rtk"), rtk.clone());
    if rtk.get("status").and_then(serde_json::Value::as_str) != Some("applied") {
        return;
    }
    if let Some(original_command) = rtk
        .get("original_command")
        .and_then(serde_json::Value::as_str)
    {
        object.insert(
            String::from("command"),
            serde_json::Value::String(original_command.to_string()),
        );
    }
    if let Some(rewritten_command) = rtk
        .get("rewritten_command")
        .and_then(serde_json::Value::as_str)
    {
        object.insert(
            String::from("effective_command"),
            serde_json::Value::String(rewritten_command.to_string()),
        );
    }
}
