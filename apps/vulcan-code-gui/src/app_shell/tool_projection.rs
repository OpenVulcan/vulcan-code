//! Tool projection and transcript helper functions extracted from the main workbench module.
//! 从主工作台模块中抽取的工具投影与转录辅助函数。

use super::*;

/// Returns whether one transcript message is a tool-detail row whose heavy body should stay out of the main UI projection.
/// 返回一条转录消息是否为工具明细行，这类行的重型正文不应进入主界面投影。
pub(super) fn session_message_is_tool_detail(message: &SessionMessageEntry) -> bool {
    matches!(
        message.record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    )
}

/// Builds the lightweight main-transcript projection for one tool-detail row while preserving its display metadata.
/// 为单条工具明细行构建轻量主转录投影，同时保留展示所需元数据。
pub(super) fn compact_tool_detail_message_for_projection(
    message: &SessionMessageEntry,
) -> SessionMessageEntry {
    if !session_message_is_tool_detail(message) {
        return message.clone();
    }

    SessionMessageEntry {
        id: message.id.clone(),
        kind: message.kind,
        record_kind: message.record_kind,
        visible: message.visible,
        title: message.title.clone(),
        body: String::new(),
        metadata: message
            .metadata
            .as_ref()
            .and_then(compact_tool_detail_metadata_for_projection),
        created_at: message.created_at.clone(),
    }
}

/// Builds compact metadata for a visible tool card without carrying raw tool payloads through the render tree.
/// 为可见工具卡构建紧凑元数据，避免原始工具载荷穿过渲染树。
pub(super) fn compact_tool_detail_metadata_for_projection(metadata: &Value) -> Option<Value> {
    let source = metadata.as_object()?;
    let mut compact = serde_json::Map::new();

    for key in [
        "tool_display_state",
        "tool_call_id",
        "tool_result_id",
        "tool_name",
        "name",
        "title",
        "display_title",
        "kind",
        "status",
        "execution_status",
        "run_state",
        "task_id",
        "session_id",
        "parent_session_id",
        "public_id",
    ] {
        insert_compact_metadata_field(&mut compact, source, key);
    }

    if let Some(payload) = source.get("payload").and_then(Value::as_object)
        && let Some(compact_payload) = compact_task_session_payload_for_projection(payload)
    {
        compact.insert(String::from("payload"), compact_payload);
    }

    if let Some(arguments) = source.get("arguments").and_then(Value::as_object)
        && let Some(compact_arguments) = compact_task_arguments_for_projection(arguments)
    {
        compact.insert(String::from("arguments"), compact_arguments);
    }

    if let Some(host_result) = resolved_host_result_from_tool_metadata(metadata)
        .and_then(compact_host_result_for_projection)
    {
        if source.get("host_result").is_some() {
            compact.insert(String::from("host_result"), host_result.clone());
        }
        if source.get("kind").is_some() && source.get("payload").is_some() {
            if let Some(kind) = host_result.get("kind") {
                compact.insert(String::from("kind"), kind.clone());
            }
            if let Some(payload) = host_result.get("payload") {
                compact.insert(String::from("payload"), payload.clone());
                if compact.get("status").is_none()
                    && let Some(status) = payload.get("status")
                {
                    compact.insert(String::from("status"), status.clone());
                }
            }
        } else if compact.get("status").is_none()
            && let Some(status) = host_result
                .get("payload")
                .and_then(|payload| payload.get("status"))
        {
            compact.insert(String::from("status"), status.clone());
        }
    }

    (!compact.is_empty()).then_some(Value::Object(compact))
}

/// Copies one small metadata field into the compact projection when it exists.
/// 当小型元数据字段存在时，将其复制到紧凑投影中。
pub(super) fn insert_compact_metadata_field(
    target: &mut serde_json::Map<String, Value>,
    source: &serde_json::Map<String, Value>,
    key: &str,
) {
    if let Some(value) = source.get(key) {
        target.insert(key.to_string(), value.clone());
    }
}

/// Builds the task-session payload subset needed by cards and subagent navigation.
/// 构建卡片与子代理导航所需的 task-session 载荷子集。
pub(super) fn compact_task_session_payload_for_projection(
    payload: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let mut compact = serde_json::Map::new();
    for key in [
        "task_session_id",
        "task_id",
        "display_title",
        "execution_status",
        "description",
        "subagent",
    ] {
        insert_compact_metadata_field(&mut compact, payload, key);
    }
    insert_compact_preview_string_field(&mut compact, payload, "prompt", 240);
    insert_compact_preview_string_field(&mut compact, payload, "result", 240);

    (!compact.is_empty()).then_some(Value::Object(compact))
}

/// Builds the in-flight task arguments subset used before a task-session result exists.
/// 构建 task-session 结果出现前用于展示的进行中任务参数子集。
pub(super) fn compact_task_arguments_for_projection(
    arguments: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let mut compact = serde_json::Map::new();
    for key in ["task_id", "description", "subagent"] {
        insert_compact_metadata_field(&mut compact, arguments, key);
    }
    insert_compact_preview_string_field(&mut compact, arguments, "prompt", 240);

    (!compact.is_empty()).then_some(Value::Object(compact))
}

/// Copies one string field with a Unicode-safe preview cap into the compact projection.
/// 将一个字符串字段按 Unicode 安全预览上限复制到紧凑投影中。
pub(super) fn insert_compact_preview_string_field(
    target: &mut serde_json::Map<String, Value>,
    source: &serde_json::Map<String, Value>,
    key: &str,
    max_chars: usize,
) {
    let Some(value) = source
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let compact_value =
        crate::agent_runtime::provider_stream_projection::truncate_preview(value, max_chars);
    target.insert(key.to_string(), Value::String(compact_value));
}

/// Preserves only the host-result subsets still needed by inline cards and modal actions after heavy tool payloads are stripped from the main transcript projection.
/// 在主转录投影剥离重型工具载荷后，仅保留内联卡片与弹层动作仍然需要的宿主结果子集。
pub(super) fn compact_host_result_for_projection(host_result: &Value) -> Option<Value> {
    match host_result.get("kind").and_then(Value::as_str) {
        Some("generated_attachments") => compact_generated_host_result_for_projection(host_result),
        Some("command_run") => compact_command_run_host_result_for_projection(host_result),
        Some("command_session") => compact_command_session_host_result_for_projection(host_result),
        Some("change_set") => compact_change_set_host_result(host_result),
        _ => None,
    }
}

/// Preserves only generated-media host-result fields needed by inline cards and modal actions.
/// 仅保留内联卡片与弹层动作所需的生成媒体宿主结果字段。
pub(super) fn compact_generated_host_result_for_projection(host_result: &Value) -> Option<Value> {
    let outputs = host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)?;
    let compact_outputs = outputs
        .iter()
        .filter_map(compact_generated_output_for_projection)
        .collect::<Vec<_>>();
    if compact_outputs.is_empty() {
        return None;
    }

    Some(json!({
        "kind": "generated_attachments",
        "payload": {
            "outputs": compact_outputs,
        },
    }))
}

/// Preserves the command-run payload subset required by dedicated command execution cards and tool modals.
/// 保留专属命令执行卡片与工具弹层所需的 command-run 载荷子集。
pub(super) fn compact_command_run_host_result_for_projection(host_result: &Value) -> Option<Value> {
    let source_payload = host_result.get("payload").and_then(Value::as_object)?;
    let mut compact_payload = serde_json::Map::new();
    for key in [
        "command",
        "cwd",
        "shell",
        "status",
        "failure_reason",
        "exit_code",
        "duration_ms",
        "stdout_bytes",
        "stderr_bytes",
        "combined_output_overflowed",
    ] {
        insert_compact_metadata_field(&mut compact_payload, source_payload, key);
    }
    if compact_payload.is_empty() {
        return None;
    }

    Some(Value::Object(serde_json::Map::from_iter([
        (
            String::from("kind"),
            Value::String(String::from("command_run")),
        ),
        (String::from("payload"), Value::Object(compact_payload)),
    ])))
}

/// Preserves the command-session payload subset required by dedicated command-session cards and tool modals.
/// 保留专属命令会话卡片与工具弹层所需的 command-session 载荷子集。
pub(super) fn compact_command_session_host_result_for_projection(
    host_result: &Value,
) -> Option<Value> {
    let source_payload = host_result.get("payload").and_then(Value::as_object)?;
    let mut compact_payload = serde_json::Map::new();
    for key in [
        "action",
        "owner_session_id",
        "command_session_id",
        "command",
        "cwd",
        "shell",
        "status",
        "failure_reason",
        "exit_code",
        "last_seq",
        "stdout_bytes",
        "stderr_bytes",
        "returned_chunk_count",
        "bytes_written",
        "has_more",
        "output_overflowed",
    ] {
        insert_compact_metadata_field(&mut compact_payload, source_payload, key);
    }
    if compact_payload.is_empty() {
        return None;
    }

    Some(Value::Object(serde_json::Map::from_iter([
        (
            String::from("kind"),
            Value::String(String::from("command_session")),
        ),
        (String::from("payload"), Value::Object(compact_payload)),
    ])))
}

/// Preserves the small generated-output descriptor required to render and open produced media.
/// 保留渲染和打开生成媒体所需的小型生成输出描述符。
pub(super) fn compact_generated_output_for_projection(output: &Value) -> Option<Value> {
    let source = output.as_object()?;
    let mut compact = serde_json::Map::new();
    for key in [
        "kind",
        "file_name",
        "local_path",
        "mime_type",
        "byte_len",
        "preview_local_path",
        "asset_id",
    ] {
        insert_compact_metadata_field(&mut compact, source, key);
    }

    (!compact.is_empty()).then_some(Value::Object(compact))
}

/// Builds the visible transcript slice that should participate in one background agent-view projection refresh.
/// 构建应参与单次后台 agent-view 投影刷新任务的可见转录切片。
pub(super) fn visible_session_messages_for_agent_view_projection(
    messages: Vec<SessionMessageEntry>,
    floating_thinking_message_id: Option<&str>,
) -> Vec<SessionMessageEntry> {
    merge_tool_display_messages(
        messages
            .into_iter()
            .filter(|message| {
                message.is_visible_to_user()
                    && floating_thinking_message_id.is_none_or(|active_id| message.id != active_id)
            })
            .collect(),
    )
}

/// Builds the GUI-side agent-view projection options for one session, keeping only the currently active streaming output mutable.
/// 为单个会话构建图形界面侧 agent-view 投影选项，并仅让当前活跃流式输出保持可变。
pub(super) fn agent_view_build_options_for_streaming_output(
    active_streaming_output_message_id: Option<&str>,
    editable_user_prompt_message_id: Option<&str>,
    locale: AppLocale,
) -> AgentViewBuildOptions {
    AgentViewBuildOptions {
        active_streaming_output_message_id: active_streaming_output_message_id.map(str::to_string),
        editable_user_prompt_message_id: editable_user_prompt_message_id.map(str::to_string),
        locale,
        collapsed_context_compaction_sections: Default::default(),
    }
}

/// Builds one session-scoped unified transcript projection snapshot entirely off the UI thread.
/// 在 UI 线程之外完整构建一份按会话归属的统一转录投影视图快照。
pub(super) fn build_session_agent_view_projection_snapshot(
    session_id: SessionId,
    messages: Vec<SessionMessageEntry>,
    runtime_events: Vec<RuntimeEvent>,
    active_streaming_output_message_id: Option<String>,
    collapsed_context_compaction_sections: BTreeSet<String>,
    locale: AppLocale,
) -> SessionAgentViewProjectionSnapshot {
    let build_options = agent_view_build_options_for_streaming_output(
        active_streaming_output_message_id.as_deref(),
        editable_user_prompt_message_id(&messages).as_deref(),
        locale,
    );
    let build_options = AgentViewBuildOptions {
        collapsed_context_compaction_sections,
        ..build_options
    };
    let document =
        build_agent_view_document_with_options(&messages, &runtime_events, &build_options);
    let selectable_document =
        vulcan_agent_view::AgentViewSelectableDocument::from_document(&document);
    let selectable_text = selectable_document.buffer().to_string();
    let raw_source_text = agent_view_document_raw_source(&document);

    SessionAgentViewProjectionSnapshot {
        session_id,
        document,
        selectable_document,
        selectable_text,
        raw_source_text,
    }
}

/// Extracts the stable tool-call identifier from one display-capable transcript message when metadata provides it.
/// 当元数据中存在稳定工具调用标识时，从单条可显示转录消息中提取该标识。
pub(super) fn tool_display_message_call_id(message: &SessionMessageEntry) -> Option<String> {
    matches!(
        message.record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    )
    .then_some(())?;
    message
        .metadata
        .as_ref()?
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Builds the Markdown summary shown by the legacy tool modal when only a persisted task-session result remains available.
/// 当仅剩持久化 task-session 结果可用时，构建旧工具弹层展示的 Markdown 摘要。
pub(super) fn task_session_modal_markdown_from_tool_metadata(metadata: &Value) -> Option<String> {
    let payload = task_session_payload_from_tool_metadata(metadata)?;
    if let Some(result_text) = payload
        .get("result")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(result_text.to_string());
    }

    match task_session_execution_status_from_tool_metadata(metadata) {
        Some("stopped") => Some(String::from(subagent_task_stopped_summary_text())),
        Some("failed") => Some(String::from(
            "子代理调用已失败，当前仅保留最后一次错误摘要。",
        )),
        _ => None,
    }
}

/// Returns the canonical user-facing stop summary reused by subagent cards, panels, and legacy tool-modal fallbacks.
/// 返回供子代理卡片、面板与旧工具弹层回退共同复用的规范用户可见停止摘要。
pub(super) fn subagent_task_stopped_summary_text() -> &'static str {
    "子代理已按用户请求主动停止，当前没有完整返回结果。请基于现有上下文决定是否重试、改写任务，或直接继续主流程。"
}

/// Returns the emoji-prefixed UI badge text for one subagent execution-status string.
/// 返回单条子代理 execution-status 字符串对应的 emoji 前缀界面徽标文案。
pub(super) fn subagent_execution_status_badge_label(execution_status: &str) -> &'static str {
    match execution_status {
        "running" => "🔄 运行中",
        "stopped" => "⏹️ 已停止",
        "failed" => "❌ 失败",
        "completed" => "✅ 完成",
        _ => "✅ 完成",
    }
}

/// Returns the emoji-prefixed UI badge text for one subagent run-state value.
/// 返回单个子代理运行状态对应的 emoji 前缀界面徽标文案。
pub(super) fn subagent_run_state_badge_label(run_state: SubagentTaskRunState) -> &'static str {
    match run_state {
        SubagentTaskRunState::Running => "🔄 运行中",
        SubagentTaskRunState::Stopped => "⏹️ 已停止",
        SubagentTaskRunState::Completed => "✅ 完成",
        SubagentTaskRunState::Failed => "❌ 失败",
    }
}

/// Returns the emoji-prefixed badge label for one command-run execution status string.
/// 返回单条命令执行状态字符串对应的 emoji 前缀徽标文案。
pub(super) fn command_run_status_badge_label(status: &str) -> &'static str {
    match status.trim() {
        "running" => "🔄 运行中",
        "completed" => "✅ 已完成",
        "failed" => "❌ 失败",
        "timed_out" => "⏱️ 已超时",
        _ => "⌘ 命令结果",
    }
}

/// Returns the emoji-prefixed badge label for one command-session lifecycle status string.
/// 返回单条命令会话生命周期状态字符串对应的 emoji 前缀徽标文案。
pub(super) fn command_session_status_badge_label(status: &str) -> &'static str {
    match status.trim() {
        "running" => "🔄 运行中",
        "completed" => "✅ 已完成",
        "failed" => "❌ 失败",
        "stopped" => "⏹️ 已停止",
        _ => "⌨ 命令会话",
    }
}

/// Returns the compact human-facing label for one shell-session action string.
/// 返回单条 shell-session 动作字符串对应的紧凑用户可见标签。
pub(super) fn command_session_action_label(action: &str) -> &'static str {
    match action.trim() {
        "start" => "启动",
        "read" => "读取",
        "write" => "写入",
        "stop" => "停止",
        _ => "未知动作",
    }
}

/// Returns the emoji-prefixed badge label for one change-set mode.
/// 返回单个变更集模式对应的 emoji 前缀徽标文案。
pub(super) fn change_set_mode_badge_label(mode: ChangeSetMode) -> &'static str {
    match mode {
        ChangeSetMode::Preview => "👀 预览变更",
        ChangeSetMode::Applied => "✅ 已应用",
    }
}

/// Parses one optional boolean-like transcript attribute into a concrete boolean value.
/// 将一条可选类布尔转录属性解析为确定布尔值。
pub(super) fn parse_agent_view_attribute_bool(value: Option<&String>) -> Option<bool> {
    match value.map(String::as_str).map(str::trim) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

/// Parses one optional transcript attribute into an unsigned 64-bit integer.
/// 将一条可选转录属性解析为无符号 64 位整数。
pub(super) fn parse_agent_view_attribute_u64(value: Option<&String>) -> Option<u64> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u64>().ok())
}

/// Parses one optional transcript attribute into a signed 32-bit integer.
/// 将一条可选转录属性解析为有符号 32 位整数。
pub(super) fn parse_agent_view_attribute_i32(value: Option<&String>) -> Option<i32> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<i32>().ok())
}

/// Parses one optional transcript attribute into a stable session identifier.
/// 将一条可选转录属性解析为稳定会话标识。
pub(super) fn parse_agent_view_attribute_session_id(value: Option<&String>) -> Option<SessionId> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| SessionId::parse_str(value).ok())
}

/// Copies one non-empty transcript attribute into an owned optional string field.
/// 将一条非空转录属性复制为拥有所有权的可选字符串字段。
pub(super) fn cloned_agent_view_attribute(
    attributes: &vulcan_agent_view::AgentViewAttributes,
    key: &str,
) -> Option<String> {
    attributes
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Builds the richer tool-modal presentation metadata from one rendered tool-block attribute map.
/// 根据单个已渲染工具区块属性映射构建增强的工具弹层展示元数据。
pub(super) fn tool_modal_presentation_from_agent_view_attributes(
    attributes: &vulcan_agent_view::AgentViewAttributes,
) -> ToolModalPresentation {
    match vulcan_agent_view::AgentViewToolPresentationKind::resolve_from_attributes(attributes) {
        vulcan_agent_view::AgentViewToolPresentationKind::CommandRun => {
            command_run_tool_modal_summary_from_agent_view_attributes(attributes)
                .map(ToolModalPresentation::CommandRun)
                .unwrap_or(ToolModalPresentation::Generic)
        }
        vulcan_agent_view::AgentViewToolPresentationKind::CommandSession => {
            command_session_tool_modal_summary_from_agent_view_attributes(attributes)
                .map(ToolModalPresentation::CommandSession)
                .unwrap_or(ToolModalPresentation::Generic)
        }
        _ => ToolModalPresentation::Generic,
    }
}

/// Builds one command-run tool-modal summary from rendered transcript block attributes.
/// 根据已渲染转录区块属性构建一份命令执行工具弹层摘要。
pub(super) fn command_run_tool_modal_summary_from_agent_view_attributes(
    attributes: &vulcan_agent_view::AgentViewAttributes,
) -> Option<CommandRunToolModalSummary> {
    let status = cloned_agent_view_attribute(attributes, "command-run-status")?;
    Some(CommandRunToolModalSummary {
        status,
        shell: cloned_agent_view_attribute(attributes, "command-shell"),
        working_directory: cloned_agent_view_attribute(attributes, "command-working-directory"),
        command_text: cloned_agent_view_attribute(attributes, "command-text"),
        failure_reason: cloned_agent_view_attribute(attributes, "command-failure-reason"),
        exit_code: parse_agent_view_attribute_i32(attributes.get("command-exit-code")),
        duration_ms: parse_agent_view_attribute_u64(attributes.get("command-duration-ms")),
        stdout_bytes: parse_agent_view_attribute_u64(attributes.get("command-stdout-bytes")),
        stderr_bytes: parse_agent_view_attribute_u64(attributes.get("command-stderr-bytes")),
        output_overflowed: parse_agent_view_attribute_bool(
            attributes.get("command-output-overflowed"),
        )
        .unwrap_or(false),
    })
}

/// Builds one command-session tool-modal summary from rendered transcript block attributes.
/// 根据已渲染转录区块属性构建一份命令会话工具弹层摘要。
pub(super) fn command_session_tool_modal_summary_from_agent_view_attributes(
    attributes: &vulcan_agent_view::AgentViewAttributes,
) -> Option<CommandSessionToolModalSummary> {
    let command_session_id =
        parse_agent_view_attribute_session_id(attributes.get("command-session-id"))?;
    let status = cloned_agent_view_attribute(attributes, "command-session-status")?;
    Some(CommandSessionToolModalSummary {
        command_session_id,
        owner_session_id: parse_agent_view_attribute_session_id(
            attributes.get("command-session-owner-id"),
        ),
        action: cloned_agent_view_attribute(attributes, "command-session-action"),
        status,
        shell: cloned_agent_view_attribute(attributes, "command-shell"),
        working_directory: cloned_agent_view_attribute(attributes, "command-working-directory"),
        command_text: cloned_agent_view_attribute(attributes, "command-text"),
        failure_reason: cloned_agent_view_attribute(attributes, "command-failure-reason"),
        exit_code: parse_agent_view_attribute_i32(attributes.get("command-exit-code")),
        last_seq: parse_agent_view_attribute_u64(attributes.get("command-last-seq")),
        stdout_bytes: parse_agent_view_attribute_u64(attributes.get("command-stdout-bytes")),
        stderr_bytes: parse_agent_view_attribute_u64(attributes.get("command-stderr-bytes")),
        returned_chunk_count: parse_agent_view_attribute_u64(
            attributes.get("command-session-returned-chunk-count"),
        ),
        bytes_written: parse_agent_view_attribute_u64(
            attributes.get("command-session-bytes-written"),
        ),
        has_more: parse_agent_view_attribute_bool(attributes.get("command-session-has-more")),
        output_overflowed: parse_agent_view_attribute_bool(
            attributes.get("command-output-overflowed"),
        )
        .unwrap_or(false),
    })
}

/// Builds one change-set tool-modal summary from structured tool metadata when the current tool result carries one canonical `change_set` host payload.
/// 当当前工具结果携带 canonical `change_set` 宿主载荷时，根据结构化工具元数据构建一份变更集工具弹层摘要。
pub(super) fn change_set_tool_modal_summary_from_tool_metadata(
    metadata: &Value,
) -> Option<ChangeSetModalSummary> {
    let host_result = resolved_host_result_from_tool_metadata(metadata)?;
    if host_result.get("kind").and_then(Value::as_str) != Some("change_set") {
        return None;
    }
    change_set_modal_summary_from_payload(host_result.get("payload")?)
}

/// Routes task-session label rendering through the shared workbench projection while preserving iced prompt-preview formatting.
/// 通过共享工作台投影路由 task-session 标签渲染，同时保留 iced 的提示词预览格式。
pub(super) fn task_session_tool_display_label(
    display_title: Option<&str>,
    description: Option<&str>,
    subagent: Option<&str>,
    public_task_id: Option<&str>,
    prompt: Option<&str>,
) -> String {
    let prompt_preview = prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|prompt| pending_prompt_preview_text(prompt, &[]));
    vulcan_workbench_model::task_session_tool_display_label(
        display_title,
        description,
        subagent,
        public_task_id,
        prompt_preview.as_deref(),
    )
}

/// Resolves the localized task-session status copy shown inside the dedicated subagent transcript card title.
/// 解析展示在专属子代理转录卡片标题中的本地化 task-session 状态文案。
pub(super) fn task_session_tool_display_status(
    record_kind: SessionMessageRecordKind,
    metadata: &Value,
) -> &'static str {
    if let Some(execution_status) = task_session_execution_status_from_tool_metadata(metadata) {
        return subagent_execution_status_badge_label(execution_status);
    }
    if let Some(display_state) = metadata
        .get("tool_display_state")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return match display_state {
            "failed" => "❌ 失败",
            "completed" => "✅ 完成",
            _ => "🔄 运行中",
        };
    }

    match record_kind {
        SessionMessageRecordKind::ToolResultOutput => "✅ 完成",
        _ => "🔄 运行中",
    }
}

/// Builds the dedicated task-session title when one tool-display metadata object belongs to the subagent surface.
/// 当某个工具显示元数据对象属于子代理工具面时，构建其专属 task-session 标题。
pub(super) fn task_session_tool_display_title(
    record_kind: SessionMessageRecordKind,
    metadata: &Value,
) -> Option<String> {
    let label = if let Some(payload) = task_session_payload_from_tool_metadata(metadata) {
        task_session_tool_display_label(
            task_session_display_title_from_payload(payload),
            payload.get("description").and_then(Value::as_str),
            payload.get("subagent").and_then(Value::as_str),
            task_session_public_id_from_payload(payload),
            payload.get("prompt").and_then(Value::as_str),
        )
    } else if let Some(arguments) = task_session_arguments_from_tool_metadata(metadata) {
        task_session_tool_display_label(
            task_session_display_title_from_tool_metadata(metadata),
            arguments.get("description").and_then(Value::as_str),
            arguments.get("subagent").and_then(Value::as_str),
            arguments.get("task_id").and_then(Value::as_str),
            arguments.get("prompt").and_then(Value::as_str),
        )
    } else {
        return None;
    };
    let status = task_session_tool_display_status(record_kind, metadata);
    Some(format!("调度 {label} {status}"))
}

/// Builds one compact single-line preview used by the pending prompt queue strip.
/// 构建待发送提示词队列条使用的一条紧凑单行预览文本。
pub(super) fn pending_prompt_preview_text(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> String {
    let parts = build_composer_input_parts(prompt, attachments);
    composer_parts_preview_text(&parts, 48usize)
}

/// Builds one full single-line label used by the visible pending-prompt queue panel.
/// 构建显示在可见待发送队列面板中的完整单行标签文本。
pub(super) fn pending_prompt_panel_label_text(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> String {
    let parts = build_composer_input_parts(prompt, attachments);
    composer_parts_label_text(&parts)
}

/// Builds one transcript-visible body that keeps user text and attachment names together.
/// 构建一段会在转录中可见的正文，同时保留用户文本与附件名称。
pub(super) fn build_user_prompt_transcript_body(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> String {
    build_shared_user_prompt_transcript_body(prompt, attachments)
}

/// Builds transcript-only metadata for one visible user prompt so GUI projections can recover canonical text and attachment strips after reload.
/// 为一条可见用户提示词构建仅供转录使用的元数据，使图形界面投影能在重开后恢复规范文本与附件面板。
pub(super) fn build_user_prompt_transcript_metadata(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> Option<Value> {
    build_shared_user_prompt_transcript_metadata(prompt, attachments)
}

/// Returns whether one visible tool transcript message belongs to the dedicated host-managed subagent surface.
/// 返回某条可见工具转录消息是否属于宿主管理的专属子代理工具面。
pub(super) fn tool_display_message_is_task_session(message: &SessionMessageEntry) -> bool {
    message.metadata.as_ref().is_some_and(|metadata| {
        task_session_payload_from_tool_metadata(metadata).is_some()
            || task_session_arguments_from_tool_metadata(metadata).is_some()
    })
}

/// Parses the generated-attachments host payload attached to one tool-display metadata object into managed conversation attachments.
/// 将附着在工具显示元数据对象上的 generated-attachments 宿主载荷解析为托管会话附件集合。
pub(super) fn generated_attachments_from_tool_metadata(
    metadata: &Value,
) -> Vec<ConversationAttachment> {
    let Some(host_result) = resolved_host_result_from_tool_metadata(metadata) else {
        return Vec::new();
    };
    if host_result.get("kind").and_then(Value::as_str) != Some("generated_attachments") {
        return Vec::new();
    }
    host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)
        .map(|outputs| {
            outputs
                .iter()
                .filter_map(parse_generated_attachment_output)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Resolves the structured host-result payload stored on one tool-display metadata object, accepting both nested and flattened runtime-event shapes.
/// 解析单份工具显示元数据对象上保存的结构化宿主结果载荷，同时兼容嵌套与扁平两种运行时事件形状。
pub(super) fn resolved_host_result_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    metadata
        .get("host_result")
        .filter(|value| value.is_object())
        .or_else(|| {
            (metadata.get("kind").is_some() && metadata.get("payload").is_some())
                .then_some(metadata)
        })
}

/// Parses one serialized generated-attachment output object back into the local attachment structure used by the GUI.
/// 将单条序列化生成附件输出对象解析回图形界面使用的本地附件结构。
pub(super) fn parse_generated_attachment_output(output: &Value) -> Option<ConversationAttachment> {
    let kind = parse_generated_attachment_kind(
        output
            .get("kind")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?,
    )?;
    let file_name = output
        .get("file_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let local_path = output
        .get("local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let mime_type = output
        .get("mime_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();

    Some(ConversationAttachment {
        kind,
        file_name,
        local_path,
        source_path: output
            .get("source_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        asset_id: output
            .get("asset_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        byte_len: output.get("byte_len").and_then(Value::as_u64),
        preview_local_path: output
            .get("preview_local_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        mime_type,
    })
}

/// Maps one serialized generated-attachment modality name back to the local attachment-kind enum.
/// 将序列化生成附件模态名称映射回本地附件类型枚举。
pub(super) fn parse_generated_attachment_kind(value: &str) -> Option<ConversationAttachmentKind> {
    match value {
        "image" => Some(ConversationAttachmentKind::Image),
        "audio" => Some(ConversationAttachmentKind::Audio),
        "video" => Some(ConversationAttachmentKind::Video),
        "pdf" => Some(ConversationAttachmentKind::Pdf),
        _ => None,
    }
}

/// Normalizes one tool display message into the unified transcript-card title expected by the merged human-facing view.
/// 将单条工具显示消息规范化为合并后面向人类视图所需的统一转录卡片标题。
pub(super) fn normalize_tool_display_message(
    mut message: SessionMessageEntry,
) -> SessionMessageEntry {
    if let Some(metadata) = message.metadata.as_ref() {
        if let Some(task_title) = task_session_tool_display_title(message.record_kind, metadata) {
            message.title = task_title;
        } else if let Some(tool_name) = metadata
            .get("tool_name")
            .and_then(Value::as_str)
            .filter(|tool_name| !tool_name.trim().is_empty())
        {
            let display_state = tool_display_state_for_message(message.record_kind, Some(metadata));
            message.title = human_tool_display_title(tool_name, display_state);
        }
    }
    let display_state =
        tool_display_state_for_message(message.record_kind, message.metadata.as_ref());
    message.metadata = attach_tool_display_state_metadata(message.metadata.take(), display_state);
    message
}

/// Merges one completed tool-result display message into the existing tool-call display card shown to the user.
/// 将一条已完成工具结果显示消息合并到当前展示给用户的工具调用显示卡片中。
pub(super) fn merge_tool_result_into_display_entry(
    existing: &mut SessionMessageEntry,
    tool_result_message: SessionMessageEntry,
) {
    let tool_result_was_task_session = tool_display_message_is_task_session(&tool_result_message);
    let tool_result_title = tool_result_message.title.clone();
    let merged_display_state = tool_display_state_for_message(
        tool_result_message.record_kind,
        tool_result_message.metadata.as_ref(),
    );
    existing.record_kind = SessionMessageRecordKind::ToolCallOutput;
    existing.body = merge_tool_display_body(&existing.body, &tool_result_message.body);
    existing.metadata =
        merge_tool_display_metadata(existing.metadata.take(), tool_result_message.metadata);
    existing.metadata =
        attach_tool_display_state_metadata(existing.metadata.take(), merged_display_state);
    if let Some(metadata) = existing.metadata.as_ref() {
        if let Some(task_title) = task_session_tool_display_title(existing.record_kind, metadata) {
            existing.title = task_title;
            return;
        }
        if let Some(tool_name) = metadata
            .get("tool_name")
            .and_then(Value::as_str)
            .filter(|tool_name| !tool_name.trim().is_empty())
        {
            existing.title = human_tool_display_title(tool_name, merged_display_state);
            return;
        }
    }
    if (existing.title.trim().is_empty() || tool_result_was_task_session)
        && !tool_result_title.trim().is_empty()
    {
        existing.title = tool_result_title;
    }
}

/// Combines one tool-call body and one tool-result body into the single Markdown payload shown inside the merged display card.
/// 将工具调用正文与工具结果正文组合成合并显示卡片中的单份 Markdown 载荷。
pub(super) fn merge_tool_display_body(tool_call_body: &str, tool_result_body: &str) -> String {
    let tool_call_body = tool_call_body.trim();
    let tool_result_body = tool_result_body.trim();
    match (tool_call_body.is_empty(), tool_result_body.is_empty()) {
        (true, true) => String::new(),
        (false, true) => tool_call_body.to_string(),
        (true, false) => tool_result_body.to_string(),
        (false, false) => format!("{tool_call_body}\n\n---\n\n{tool_result_body}"),
    }
}

/// Shallow-merges one tool-call metadata object with the later tool-result metadata object so the display layer can see the full execution snapshot.
/// 将工具调用元数据对象与后续工具结果元数据对象做浅层合并，便于显示层看到完整执行快照。
pub(super) fn merge_tool_display_metadata(
    tool_call_metadata: Option<Value>,
    tool_result_metadata: Option<Value>,
) -> Option<Value> {
    vulcan_workbench_model::merge_tool_display_metadata(tool_call_metadata, tool_result_metadata)
}

/// Returns the stable display state used by one human-facing tool execution card.
/// 返回单张面向人类的工具执行卡片所使用的稳定显示状态。
pub(super) fn tool_display_state_for_message(
    record_kind: SessionMessageRecordKind,
    metadata: Option<&Value>,
) -> &'static str {
    match record_kind {
        SessionMessageRecordKind::ToolResultOutput => "completed",
        _ if metadata.is_some_and(tool_metadata_represents_failed_result) => "failed",
        _ => "running",
    }
}

/// Returns whether the provided tool metadata semantically describes one failed host-side tool result.
/// 返回提供的工具元数据是否在语义上描述了一条失败的宿主侧工具结果。
pub(super) fn tool_metadata_represents_failed_result(metadata: &Value) -> bool {
    metadata
        .get("host_result")
        .and_then(|host_result| host_result.get("kind"))
        .and_then(Value::as_str)
        == Some("tool_error")
        || metadata
            .get("payload")
            .and_then(|payload| payload.get("status"))
            .and_then(Value::as_str)
            == Some("failed")
        || metadata.get("status").and_then(Value::as_str) == Some("failed")
}

/// Builds one stable human-facing title for a tool execution card from the tool name and display state.
/// 根据工具名称与显示状态构建一条稳定的人类可读工具执行卡片标题。
pub(super) fn human_tool_display_title(tool_name: &str, display_state: &str) -> String {
    let status_label = match display_state {
        "failed" => "失败",
        "completed" => "已完成",
        _ => "进行中",
    };
    format!("调用 {tool_name} 工具 {status_label}")
}

/// Injects one display-only execution state flag into tool-card metadata without affecting the model-facing protocol payload.
/// 向工具卡片元数据注入一个仅用于显示的执行状态标记，而不影响面向模型的协议载荷。
pub(super) fn attach_tool_display_state_metadata(
    metadata: Option<Value>,
    state: &str,
) -> Option<Value> {
    vulcan_workbench_model::attach_tool_display_state_to_metadata(metadata, state)
}

/// Builds the human-facing Markdown body appended to the transcript after one GUI-initiated command-session stop succeeds.
/// 构建在一次 GUI 发起的命令会话停止成功后追加到转录中的人类可读 Markdown 正文。
pub(super) fn build_command_session_stop_result_body(snapshot: &CommandSessionSnapshot) -> String {
    let leading_summary = match snapshot.status {
        CommandSessionStatus::Stopped => "命令会话已按用户请求停止。",
        CommandSessionStatus::Failed => "命令会话在停止过程中进入失败状态。",
        CommandSessionStatus::Completed => "命令会话在停止请求完成前已经自然结束。",
        CommandSessionStatus::Running => "命令会话停止请求已发出，但当前快照仍显示为运行中。",
    };
    let mut body = format!(
        "## 命令会话停止\n\n{leading_summary}\n\n- 会话 ID: `{}`\n- 命令: `{}`\n- Shell: `{}`\n- 工作目录: `{}`\n- 最终状态: `{}`\n- 退出码: `{}`\n- stdout: `{}` bytes\n- stderr: `{}` bytes\n- last_seq: `{}`",
        snapshot.command_session_id,
        snapshot.command,
        snapshot.shell_label,
        snapshot.cwd.display(),
        snapshot.status.as_str(),
        snapshot
            .exit_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("-")),
        snapshot.stdout_bytes,
        snapshot.stderr_bytes,
        snapshot.last_seq,
    );
    if let Some(failure_reason) = snapshot
        .failure_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body.push_str(&format!("\n- 失败原因: `{failure_reason}`"));
    }
    body
}

/// Builds one runtime-like tool-result metadata object for a GUI-initiated command-session stop so transcript persistence stays structurally aligned with provider-emitted tool results.
/// 为 GUI 发起的命令会话停止构建一份类运行时工具结果元数据对象，使转录持久化在结构上与 Provider 发出的工具结果保持一致。
pub(super) fn build_command_session_stop_result_metadata(
    tool_call_id: &str,
    snapshot: &CommandSessionSnapshot,
) -> Option<Value> {
    let tool_result_id = format!(
        "tool-result-manual-stop-{tool_call_id}-{}",
        current_unix_millis_text()
    );
    let host_metadata = json!({
        "kind": "command_session",
        "payload": {
            "action": "stop",
            "owner_session_id": snapshot.owner_session_id.to_string(),
            "command_session_id": snapshot.command_session_id.to_string(),
            "command": snapshot.command,
            "cwd": snapshot.cwd.display().to_string(),
            "shell": snapshot.shell_label,
            "status": snapshot.status.as_str(),
            "exit_code": snapshot.exit_code,
            "failure_reason": snapshot.failure_reason,
            "stdout_bytes": snapshot.stdout_bytes,
            "stderr_bytes": snapshot.stderr_bytes,
            "last_seq": snapshot.last_seq,
            "has_more": false,
            "output_overflowed": false,
        },
    });
    attach_tool_name_to_runtime_metadata(
        build_tool_result_runtime_metadata(
            tool_call_id,
            &tool_result_id,
            Some(&host_metadata),
            "completed",
        ),
        "shell_session",
    )
}

/// Parses one UUID-like session identifier text into the local `SessionId` type when the input is non-empty and valid.
/// 当输入非空且格式合法时，将一段类 UUID 的会话标识文本解析为本地 `SessionId` 类型。
pub(super) fn parse_session_id_text(value: Option<&str>) -> Option<SessionId> {
    let value = value?.trim();
    (!value.is_empty())
        .then_some(value)
        .and_then(|value| SessionId::parse_str(value).ok())
}

/// Routes one merged task-session metadata object through the shared subagent projection.
/// 将一份合并后的 task-session 元数据对象路由到共享子代理投影。
pub(super) fn parse_subagent_task_panel_metadata(
    parent_session_id: SessionId,
    metadata: &Value,
) -> Option<SubagentTaskPanelState> {
    let prompt_preview = parse_subagent_task_prompt_metadata(metadata)
        .as_deref()
        .map(|prompt| pending_prompt_preview_text(prompt, &[]));
    vulcan_workbench_model::parse_subagent_task_panel_metadata(
        parent_session_id,
        metadata,
        prompt_preview.as_deref(),
    )
}

/// Extracts the task prompt text through the shared task-session projection helper.
/// 通过共享 task-session 投影辅助函数提取任务提示词文本。
pub(super) fn parse_subagent_task_prompt_metadata(metadata: &Value) -> Option<String> {
    vulcan_workbench_model::parse_subagent_task_prompt_metadata(metadata)
}
