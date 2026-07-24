use super::*;

pub(crate) fn host_hook_point_descriptors() -> Vec<HookPointDescriptor> {
    vec![
        HookPointDescriptor::new(
            HOOK_CONVERSATION_PROFILE_PREPARE,
            [HookMode::Observe, HookMode::Transform, HookMode::Effect],
            "Prepares persistent session profile context independently from user-prompt recall.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_TURN_PRECHECK,
            [HookMode::Observe, HookMode::Transform, HookMode::Effect],
            "Runs before a user prompt enters provider preparation and may return context injections.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_USER_EVENT,
            [HookMode::Observe, HookMode::Effect],
            "Runs when a user-authored prompt, supplement, or interrupt joins the logical turn.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_QUESTION_ASKED,
            [HookMode::Observe, HookMode::Effect],
            "Runs when the ask runtime opens a blocking question for the user.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_QUESTION_ANSWERED,
            [HookMode::Observe, HookMode::Effect],
            "Runs when the user answers an ask runtime question.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_QUESTION_REJECTED,
            [HookMode::Observe, HookMode::Effect],
            "Runs when the user rejects an ask runtime question.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_TOOL_STARTED,
            [HookMode::Observe, HookMode::Effect],
            "Runs when a conversation tool call starts.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_TOOL_FINISHED,
            [HookMode::Observe, HookMode::Effect],
            "Runs when a conversation tool call finishes.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_TURN_COMPLETED,
            [HookMode::Observe, HookMode::Effect],
            "Runs when a logical turn has final user, timeline, and assistant content.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_TURN_TERMINATED,
            [HookMode::Observe, HookMode::Effect],
            "Runs when a logical turn stops before a final assistant answer.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_CONVERSATION_COMPACTED,
            [HookMode::Observe, HookMode::Effect],
            "Runs after session context compaction finishes.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            crate::agent_runtime::provider_request_telemetry::HOOK_PROVIDER_REQUEST_STARTED,
            [HookMode::Observe, HookMode::Effect],
            "Runs after one physical Provider API request starts.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            crate::agent_runtime::provider_request_telemetry::HOOK_PROVIDER_RESPONSE_FIRST_TOKEN,
            [HookMode::Observe, HookMode::Effect],
            "Runs after one physical Provider response exposes its first visible token.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            crate::agent_runtime::provider_request_telemetry::HOOK_PROVIDER_REQUEST_COMPLETED,
            [HookMode::Observe, HookMode::Effect],
            "Runs after one physical Provider API request reaches its terminal state.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            crate::agent_runtime::provider_request_telemetry::HOOK_PROVIDER_REQUEST_RETRYING,
            [HookMode::Observe, HookMode::Effect],
            "Runs while the Provider transport waits before an automatic retry.",
        )
        .with_default_scope(HookScope::Session),
        HookPointDescriptor::new(
            HOOK_TOOL_SHELL_COMMAND_BEFORE_EXECUTE,
            [HookMode::Transform],
            "Transforms one shell command before the host launches its process.",
        )
        .with_default_scope(HookScope::Workspace),
        HookPointDescriptor::new(
            HOOK_GIT_COMMIT_SUMMARY_CONTEXT,
            [HookMode::Transform],
            "Transforms structured Git context before commit-message generation.",
        )
        .with_default_scope(HookScope::Workspace),
    ]
}

/// Formats one continued or queued asynchronous Hook handler failure.
/// 格式化单个继续执行或可入队的异步 Hook 处理器失败。
///
/// # Parameters
/// # 参数
/// `point` is the stable Hook point name from the dispatched request.
/// `point` 是派发请求中的稳定 Hook 点位名称。
/// `run_id` is the stable invocation identity from the dispatched request.
/// `run_id` 是派发请求中的稳定调用标识。
/// `failure` contains the exact handler identity, active failure policy, and message.
/// `failure` 包含精确处理器标识、生效失败策略与消息。
///
/// # Returns
/// # 返回值
/// A stable single-line diagnostic suitable for stderr and captured host logs.
/// 返回适合标准错误与宿主捕获日志的稳定单行诊断。
pub(super) fn async_hook_handler_failure_diagnostic(
    point: &str,
    run_id: &str,
    failure: &HookFailure,
) -> String {
    format!(
        "[conversation-hook] async dispatch handler failed: point={point} run_id={run_id} hook_id={} policy={:?} error={}",
        failure.hook_id, failure.policy, failure.message
    )
}

/// Formats one top-level asynchronous Hook dispatch error and any nested stopped-handler failures.
/// 格式化单个异步 Hook 顶层调度错误及其嵌套的停止处理器失败。
///
/// # Parameters
/// # 参数
/// `point` is the stable Hook point name from the dispatched request.
/// `point` 是派发请求中的稳定 Hook 点位名称。
/// `run_id` is the stable invocation identity from the dispatched request.
/// `run_id` 是派发请求中的稳定调用标识。
/// `error` is the top-level dispatcher error, including stop-policy termination.
/// `error` 是顶层调度器错误，包括停止策略终结。
///
/// # Returns
/// # 返回值
/// Stable single-line diagnostics containing nested handler details before the top-level summary.
/// 返回稳定单行诊断集合，其中嵌套处理器详情位于顶层摘要之前。
pub(super) fn async_hook_dispatch_error_diagnostics(
    point: &str,
    run_id: &str,
    error: &HookError,
) -> Vec<String> {
    // diagnostics preserves every stop-policy handler identity before appending the dispatcher summary.
    // diagnostics 在追加调度器摘要前保留每个停止策略处理器身份。
    let mut diagnostics = match error {
        HookError::DispatchStopped(failures) => failures
            .iter()
            .map(|failure| async_hook_handler_failure_diagnostic(point, run_id, failure))
            .collect(),
        _ => Vec::new(),
    };
    diagnostics.push(format!(
        "[conversation-hook] async dispatch failed: point={point} run_id={run_id} error={error}"
    ));
    diagnostics
}

/// Records every failed handler or top-level error returned by one asynchronous effect Hook dispatch.
/// 记录单次异步 effect Hook 调度返回的每个处理器失败或顶层错误。
///
/// # Parameters
/// # 参数
/// `point` is the stable Hook point name from the dispatched request.
/// `point` 是派发请求中的稳定 Hook 点位名称。
/// `run_id` is the stable invocation identity from the dispatched request.
/// `run_id` 是派发请求中的稳定调用标识。
/// `outcome` is the complete dispatcher result, including continued and queued handler failures.
/// `outcome` 是完整调度器结果，包括继续执行与可入队的处理器失败。
///
/// # Returns
/// # 返回值
/// This function returns no value; successful reports without failures produce no diagnostic.
/// 此函数无返回值；不含失败的成功报告不会产生诊断。
fn record_async_hook_dispatch_outcome(
    point: &str,
    run_id: &str,
    outcome: Result<HookDispatchReport, HookError>,
) {
    match outcome {
        Ok(report) => {
            for failure in report.failures {
                eprintln!(
                    "{}",
                    async_hook_handler_failure_diagnostic(point, run_id, &failure)
                );
            }
        }
        Err(error) => {
            for diagnostic in async_hook_dispatch_error_diagnostics(point, run_id, &error) {
                eprintln!("{diagnostic}");
            }
        }
    }
}

/// Spawns a Hook dispatch task and records every asynchronous failure without blocking the caller.
/// 派生 Hook 调度任务，并在不阻塞调用方的情况下记录每个异步失败。
///
/// # Parameters
/// # 参数
/// `dispatcher` owns the immutable Hook registry snapshot used by the asynchronous dispatch.
/// `dispatcher` 持有异步调度使用的不可变 Hook 注册表快照。
/// `request` carries the exact Hook point, run identity, context, and payload.
/// `request` 携带精确 Hook 点位、运行标识、上下文与载荷。
///
/// # Returns
/// # 返回值
/// This function returns immediately after scheduling; scheduling failures are logged explicitly.
/// 此函数在完成调度后立即返回；调度基础设施失败会被显式记录。
pub(super) fn spawn_hook_dispatch(dispatcher: HookDispatcher, request: HookDispatchRequest) {
    // point preserves the request point after the request moves into asynchronous execution.
    // point 在请求移入异步执行后保留其点位。
    let point = request.context.point.clone();
    // run_id preserves the request identity after the request moves into asynchronous execution.
    // run_id 在请求移入异步执行后保留其运行标识。
    let run_id = request.context.run_id.clone();
    if let Ok(handle) = Handle::try_current() {
        handle.spawn(async move {
            // outcome retains continued, queued, and stop-policy failures for unified diagnostics.
            // outcome 为统一诊断保留继续执行、可入队及停止策略失败。
            let outcome = dispatcher.dispatch(request).await;
            record_async_hook_dispatch_outcome(&point, &run_id, outcome);
        });
        return;
    }

    // thread_result exposes OS thread admission failure instead of panicking or dropping the dispatch.
    // thread_result 显式暴露系统线程准入失败，而不是 panic 或丢弃调度。
    // thread_failure_point preserves correlation for an OS thread admission error before the closure starts.
    // thread_failure_point 为闭包启动前的系统线程准入错误保留关联点位。
    let thread_failure_point = point.clone();
    // thread_failure_run_id preserves correlation for an OS thread admission error before the closure starts.
    // thread_failure_run_id 为闭包启动前的系统线程准入错误保留运行标识。
    let thread_failure_run_id = run_id.clone();
    let thread_result = std::thread::Builder::new()
        .name(String::from("vulcan-effect-hook-dispatch"))
        .spawn(move || {
            // Effect hooks can perform async IO such as gRPC calls even when fired from the UI thread.
            // 即使 effect hook 从 UI 线程触发，也允许它执行 gRPC 等异步 IO。
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(async move {
                    // outcome retains continued, queued, and stop-policy failures for unified diagnostics.
                    // outcome 为统一诊断保留继续执行、可入队及停止策略失败。
                    let outcome = dispatcher.dispatch(request).await;
                    record_async_hook_dispatch_outcome(&point, &run_id, outcome);
                }),
                Err(error) => eprintln!(
                    "[conversation-hook] async dispatch runtime failed: point={point} run_id={run_id} error={error}"
                ),
            }
        });
    if let Err(error) = thread_result {
        eprintln!(
            "[conversation-hook] async dispatch thread failed to start: point={thread_failure_point} run_id={thread_failure_run_id} error={error}"
        );
    }
}

/// Returns the hook event kind for one pending prompt source.
/// 返回单个待发送提示词来源对应的 hook 事件类型。
pub(super) fn pending_prompt_source_event_kind(source: PendingSessionPromptSource) -> &'static str {
    match source {
        PendingSessionPromptSource::UserFollowup => "user_followup",
        PendingSessionPromptSource::UserInterrupt => "user_interrupt",
        PendingSessionPromptSource::SessionMailbox => "session_mailbox",
        PendingSessionPromptSource::GoalContinuation => "goal_continuation",
    }
}

/// Counts host-visible canonical history messages.
/// 统计宿主可见的规范历史消息数量。
pub(super) fn visible_history_count(conversation_state: &SessionConversationState) -> usize {
    conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .count()
}

/// Builds a compact question prompt summary.
/// 构建一条精简问题提示摘要。
pub(super) fn question_prompt_summary(question: &QuestionPrompt) -> String {
    let option_text = question
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if option_text.is_empty() {
        format!("{}: {}", question.header, question.question)
    } else {
        format!(
            "{}: {} [{}]",
            question.header, question.question, option_text
        )
    }
}

/// Builds JSON-safe question prompt payloads for hook consumers.
/// 为 hook 消费方构建 JSON 安全的问题提示 payload。
pub(super) fn question_prompt_payloads(questions: &[QuestionPrompt]) -> Vec<Value> {
    questions
        .iter()
        .map(|question| {
            json!({
                "header": question.header,
                "question": question.question,
                "multiple": question.multiple,
                "options": question.options.iter().map(|option| {
                    json!({
                        "label": option.label,
                        "description": option.description,
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect()
}
