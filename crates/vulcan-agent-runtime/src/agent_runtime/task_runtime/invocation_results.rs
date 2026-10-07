use super::*;

/// Builds one model-visible task invocation result from the execution outcome.
/// 根据执行结果构建一条模型可见的任务调用结果。
pub(super) fn build_task_invocation_result(
    invocation: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
) -> VulcanToolInvocationResult {
    let task_status = task_invocation_status_text(invocation.is_resumed);
    let execution_status = outcome.execution_status.as_str();
    let model_output = json!({
        "task_id": invocation.public_task_id.clone(),
        (SUBAGENT_TASK_ID_KEY): outcome.task_id.to_string(),
        "display_title": invocation.display_title.clone(),
        "status": task_status,
        "execution_status": execution_status,
        "origin_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": invocation.parent_task_id.map(|value| value.to_string()),
        "depth": invocation.depth,
        "description": invocation.description.clone(),
        "subagent": invocation.subagent.clone(),
        "final_assistant_message_id": outcome.final_assistant_message_id.clone(),
        "debug": subagent_runtime_debug_payload(&invocation.invocation_config),
        "result": outcome.final_output_text.clone(),
    });
    let payload = build_task_host_payload(
        invocation,
        execution_status,
        outcome.final_assistant_message_id.as_deref(),
        outcome.final_output_text.as_str(),
        current_unix_millis_text(),
    );
    VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
        VulcanToolHostResult {
            kind: String::from(SUBAGENT_TASK_KIND),
            payload,
        },
    )
}

/// Builds one model-visible spawn_agent scheduling acknowledgement from the execution outcome.
/// 根据执行结果构建一条模型可见的 spawn_agent 调度确认。
pub(super) fn build_spawn_agent_invocation_result(
    invocation: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
) -> VulcanToolInvocationResult {
    let execution_status = outcome.execution_status.as_str();
    let model_output = json!({
        "task_name": invocation.public_task_id.clone(),
        (SUBAGENT_TASK_ID_KEY): outcome.task_id.to_string(),
        "display_title": invocation.display_title.clone(),
        "execution_status": execution_status,
        "message": "Subagent scheduled.",
        "debug": subagent_runtime_debug_payload(&invocation.invocation_config),
    });
    let payload = build_task_host_payload(
        invocation,
        execution_status,
        outcome.final_assistant_message_id.as_deref(),
        "",
        current_unix_millis_text(),
    );
    VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
        VulcanToolHostResult {
            kind: String::from(SUBAGENT_TASK_KIND),
            payload,
        },
    )
}

/// Returns whether a runtime task record can be read without launching another backend execution.
/// 返回运行时任务记录是否可直接读取而无需再次启动后端执行。
pub(super) fn task_session_record_has_cached_status(record: &TaskSessionRecord) -> bool {
    match task_session_status_from_record_status(record.last_status.as_str()) {
        Ok(status) => status.is_terminal(),
        Err(_) => false,
    }
}

/// Builds one cached task result from the in-memory task registry.
/// 从内存任务注册表构建一条缓存任务结果。
pub(super) fn build_cached_task_invocation_result_from_record(
    current_session_id: SessionId,
    record: &TaskSessionRecord,
) -> VulcanToolInvocationResult {
    build_cached_task_invocation_result(TaskCachedInvocationResultInput {
        current_session_id,
        task_id: record.task_id,
        public_task_id: record.public_task_id.clone(),
        display_title: record.display_title.clone(),
        execution_status: record.last_status.clone(),
        parent_task_id: record.parent_task_id,
        depth: record.depth,
        prompt: record.last_prompt.clone(),
        description: record.description.clone(),
        subagent: record.subagent.clone(),
        result_text: record.last_result_text.clone(),
        updated_at: record.updated_at.clone(),
    })
}

/// Builds one running-status result for a duplicate invocation of an active task.
/// 为活动任务的重复调用构建一条运行中状态结果。
pub(super) fn build_running_task_invocation_result(
    current_session_id: SessionId,
    resolved_identifier: &ResolvedTaskSessionIdentifier,
    incoming_prompt: &str,
) -> VulcanToolInvocationResult {
    build_cached_task_invocation_result(TaskCachedInvocationResultInput {
        current_session_id,
        task_id: resolved_identifier.task_id,
        public_task_id: resolved_identifier.public_task_id.clone(),
        display_title: resolved_identifier.public_task_id.clone(),
        execution_status: String::from("running"),
        parent_task_id: None,
        depth: 1,
        prompt: incoming_prompt.to_string(),
        description: None,
        subagent: None,
        result_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        updated_at: current_unix_millis_text(),
    })
}

/// Input bundle for a cached task invocation response.
/// 缓存任务调用响应的输入集合。
struct TaskCachedInvocationResultInput {
    /// Invoking controller session identifier that owns the current tool call.
    /// 拥有当前工具调用的执行中控制器会话标识。
    current_session_id: SessionId,
    /// Stable task-session identifier.
    /// 稳定的任务会话标识。
    task_id: SessionId,
    /// Caller-facing task identifier.
    /// 调用方面向的任务标识。
    public_task_id: String,
    /// Stable human-readable title.
    /// 稳定的人类可读标题。
    display_title: String,
    /// Cached execution status name.
    /// 缓存的执行状态名称。
    execution_status: String,
    /// Optional parent task identifier.
    /// 可选的父任务标识。
    parent_task_id: Option<SessionId>,
    /// Task nesting depth.
    /// 任务嵌套深度。
    depth: usize,
    /// Prompt text associated with the task call.
    /// 与任务调用关联的提示词文本。
    prompt: String,
    /// Optional human-readable task description.
    /// 可选的人类可读任务描述。
    description: Option<String>,
    /// Optional subagent label.
    /// 可选的子代理标签。
    subagent: Option<String>,
    /// Cached task result text.
    /// 缓存的任务结果文本。
    result_text: String,
    /// Cached update timestamp.
    /// 缓存的更新时间戳。
    updated_at: String,
}

/// Builds one standard tool result payload for a cached task invocation.
/// 为缓存任务调用构建标准工具结果载荷。
fn build_cached_task_invocation_result(
    input: TaskCachedInvocationResultInput,
) -> VulcanToolInvocationResult {
    let model_output = json!({
        "task_id": input.public_task_id,
        (SUBAGENT_TASK_ID_KEY): input.task_id.to_string(),
        "display_title": input.display_title,
        "status": "resumed",
        "execution_status": input.execution_status,
        "origin_session_id": input.current_session_id.to_string(),
        "parent_task_id": input.parent_task_id.map(|value| value.to_string()),
        "depth": input.depth,
        "description": input.description,
        "subagent": input.subagent,
        "final_assistant_message_id": Value::Null,
        "result": input.result_text,
        "cached": true,
    });
    let payload = json!({
        "task_id": input.public_task_id,
        (SUBAGENT_TASK_ID_KEY): input.task_id.to_string(),
        "display_title": input.display_title,
        "status": "resumed",
        "execution_status": input.execution_status,
        "origin_session_id": input.current_session_id.to_string(),
        "parent_task_id": input.parent_task_id.map(|value| value.to_string()),
        "depth": input.depth,
        "prompt": input.prompt,
        "description": input.description,
        "subagent": input.subagent,
        "final_assistant_message_id": Value::Null,
        "result": input.result_text,
        "updated_at": input.updated_at,
        "cached": true,
    });
    VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
        VulcanToolHostResult {
            kind: String::from(SUBAGENT_TASK_KIND),
            payload,
        },
    )
}

/// Returns one RFC3339-like Unix-millis text stamp for task-session records.
/// 为任务会话记录返回一条类似 RFC3339 的 Unix 毫秒时间戳文本。
pub(super) fn current_unix_millis_text() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| String::from("0"))
}

/// Returns one Unix-millis timestamp as an integer for persisted TaskSession records.
/// 为持久化 TaskSession 记录返回整数形式的 Unix 毫秒时间戳。
pub(super) fn current_unix_millis_u64() -> u64 {
    current_unix_millis_text().parse::<u64>().unwrap_or(0)
}

/// Returns a UTC timestamp for task-runtime diagnostic traces.
/// 返回任务运行时诊断日志使用的 UTC 时间戳。
pub(super) fn current_task_runtime_trace_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    )
}

/// Returns whether task-runtime tracing should be emitted with the workbench trace switch.
/// 返回是否应随工作台跟踪开关输出任务运行时日志。
pub(super) fn task_runtime_trace_enabled() -> bool {
    std::env::var("VULCAN_WEBUI_TRACE")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Emits one task-runtime timing trace line when workbench tracing is enabled.
/// 当工作台跟踪启用时输出一条任务运行时耗时日志。
pub(super) fn task_runtime_trace(stage: &str, detail: impl AsRef<str>) {
    if !task_runtime_trace_enabled() {
        return;
    }
    eprintln!(
        "[task-runtime][webui-trace] timestamp={} unix_ms={} stage={} {}",
        current_task_runtime_trace_timestamp(),
        current_unix_millis_text(),
        stage,
        detail.as_ref()
    );
}
