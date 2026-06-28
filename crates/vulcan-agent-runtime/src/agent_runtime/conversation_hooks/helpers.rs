use super::*;

pub(super) fn conversation_hook_point_descriptors() -> Vec<HookPointDescriptor> {
    vec![
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
    ]
}

/// Spawns a hook dispatch task on the current Tokio runtime when available.
/// 在当前 Tokio 运行时可用时派生一个 hook 调度任务。
pub(super) fn spawn_hook_dispatch(dispatcher: HookDispatcher, request: HookDispatchRequest) {
    if let Ok(handle) = Handle::try_current() {
        handle.spawn(async move {
            let _ = dispatcher.dispatch(request).await;
        });
        return;
    }

    std::thread::spawn(move || {
        // Effect hooks can perform async IO such as gRPC calls even when fired from the UI thread.
        // 即使 effect hook 从 UI 线程触发，也允许它执行 gRPC 等异步 IO。
        if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            runtime.block_on(async move {
                let _ = dispatcher.dispatch(request).await;
            });
        }
    });
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
