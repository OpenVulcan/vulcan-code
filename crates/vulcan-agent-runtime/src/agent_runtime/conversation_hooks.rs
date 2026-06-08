//! Conversation hook assembly and dispatch helpers for runtime-visible extension points.
//! 面向运行时可见扩展点的会话 hook 组装与调度辅助。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::question_runtime::QuestionPrompt;
use crate::conversation_protocol::{SessionConversationPayload, SessionConversationState};
use crate::model::{PendingSessionPromptSource, SessionId, VmmProfileSyncState};
use futures_channel::oneshot;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::runtime::Handle;
use vulcan_hooks::{
    HookActor, HookContext, HookDispatchReport, HookDispatchRequest, HookDispatcher, HookMode,
    HookPayload, HookPointDescriptor, HookRegistrationSnapshot, HookScope, SystemPromptHookPhase,
};
use vulcan_vmm::PostActionRequest;

/// Metadata key used by hooks to report VMM writeback side effects.
/// hook 用于报告 VMM 写回副作用的 metadata 键。
pub const HOOK_METADATA_VMM_WRITEBACK_EFFECTS: &str = "vmm_writeback_effects";
/// Metadata key used by hooks to report VMM profile synchronization effects.
/// hook 用于报告 VMM 画像同步副作用的 metadata 键。
pub const HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT: &str = "vmm_profile_sync_effect";

/// Hook point fired before a user turn is sent to provider preparation.
/// 在用户回合发送到 Provider 准备阶段前触发的 hook 点位。
pub const HOOK_CONVERSATION_TURN_PRECHECK: &str = "conversation.turn.precheck";
/// Hook point fired whenever a user-authored event joins a logical turn.
/// 每当用户撰写事件加入一个逻辑回合时触发的 hook 点位。
pub const HOOK_CONVERSATION_USER_EVENT: &str = "conversation.user.event";
/// Hook point fired when the runtime ask tool opens a question.
/// 运行时 ask 工具打开问题时触发的 hook 点位。
pub const HOOK_CONVERSATION_QUESTION_ASKED: &str = "conversation.question.asked";
/// Hook point fired when the user answers a runtime ask question.
/// 用户回答运行时 ask 问题时触发的 hook 点位。
pub const HOOK_CONVERSATION_QUESTION_ANSWERED: &str = "conversation.question.answered";
/// Hook point fired when the user rejects a runtime ask question.
/// 用户拒绝运行时 ask 问题时触发的 hook 点位。
pub const HOOK_CONVERSATION_QUESTION_REJECTED: &str = "conversation.question.rejected";
/// Hook point fired when a tool call starts.
/// 工具调用开始时触发的 hook 点位。
pub const HOOK_CONVERSATION_TOOL_STARTED: &str = "conversation.tool.started";
/// Hook point fired when a tool call finishes.
/// 工具调用完成时触发的 hook 点位。
pub const HOOK_CONVERSATION_TOOL_FINISHED: &str = "conversation.tool.finished";
/// Hook point fired when a logical user turn completes with a final assistant answer.
/// 逻辑用户回合以最终助手回答完成时触发的 hook 点位。
pub const HOOK_CONVERSATION_TURN_COMPLETED: &str = "conversation.turn.completed";
/// Hook point fired when a logical user turn terminates without a final assistant answer.
/// 逻辑用户回合未产出最终助手回答而终止时触发的 hook 点位。
pub const HOOK_CONVERSATION_TURN_TERMINATED: &str = "conversation.turn.terminated";
/// Hook point fired when a session context compaction finishes.
/// 会话上下文压缩完成时触发的 hook 点位。
pub const HOOK_CONVERSATION_COMPACTED: &str = "conversation.compacted";
/// Host-side safety ceiling for waiting on precheck hook results.
/// 等待 precheck hook 结果的宿主侧安全兜底上限。
const PRECHECK_DISPATCH_TIMEOUT_MS: u64 = 120_000;
/// Host-side safety ceiling for completed-turn hook dispatch.
/// 完成回合 hook 调度的宿主侧安全兜底上限。
const COMPLETION_DISPATCH_TIMEOUT_MS: u64 = 120_000;

/// Completion state for one logical conversation turn.
/// 单个逻辑会话回合的完成状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationTurnCompletionStatus {
    /// The model response finished, but child subagents still belong to the same user turn.
    /// 模型响应已经结束，但子代理仍属于同一个用户回合。
    AwaitingSubagents,
    /// The full logical user turn is complete and can be submitted to completed-turn consumers.
    /// 完整逻辑用户回合已经结束，可以提交给完成回合消费方。
    Completed,
}

impl ConversationTurnCompletionStatus {
    /// Returns the stable payload value used by hook consumers.
    /// 返回 hook 消费方使用的稳定载荷值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingSubagents => "awaiting_subagents",
            Self::Completed => "completed",
        }
    }

    /// Returns whether this state closes the active logical turn.
    /// 返回该状态是否会闭合活动逻辑回合。
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// One compact timeline item emitted to conversation hook consumers.
/// 发送给会话 hook 消费方的一条精简 timeline 项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTimelineItem {
    /// Stable role used by memory and audit consumers.
    /// 供记忆与审计消费方使用的稳定角色。
    pub role: String,
    /// Stable event kind inside the logical turn.
    /// 逻辑回合内部的稳定事件类型。
    pub kind: String,
    /// Filtered text content suitable for extension consumption.
    /// 适合扩展消费的过滤后文本内容。
    pub content: String,
    /// Optional source id such as request id or tool call id.
    /// 可选来源标识，例如请求标识或工具调用标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

/// Host-visible VMM writeback effect returned by hook handlers.
/// hook 处理器返回给宿主可见的 VMM 写回效果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmmWritebackHookEffect {
    /// Previously queued outbox entries were flushed successfully.
    /// 先前排队的 outbox 条目已成功冲刷。
    Flushed {
        /// Local outbox entry ids that should be removed.
        /// 应被移除的本地 outbox 条目标识。
        entry_ids: Vec<String>,
    },
    /// One PostAction request should be retained for later replay.
    /// 一条 PostAction 请求应保留用于后续重放。
    Enqueue {
        /// Original VMM PostAction request.
        /// 原始 VMM PostAction 请求。
        request: PostActionRequest,
        /// Failure message that caused the enqueue.
        /// 导致入队的失败消息。
        error: String,
    },
    /// One queued PostAction request failed during replay.
    /// 一条排队的 PostAction 请求在重放时失败。
    Failed {
        /// Local outbox entry id that should be updated.
        /// 应被更新的本地 outbox 条目标识。
        entry_id: String,
        /// Failure message observed during replay.
        /// 重放期间观察到的失败消息。
        error: String,
    },
}

/// Host-visible VMM profile sync effect returned by hook handlers.
/// hook 处理器返回给宿主可见的 VMM 画像同步效果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmProfileSyncHookEffect {
    /// Updated profile sync state for the owning session.
    /// 拥有会话的已更新画像同步状态。
    pub state: VmmProfileSyncState,
}

impl ConversationTimelineItem {
    /// Builds one timeline item.
    /// 构建一条 timeline 项。
    fn new(
        role: impl Into<String>,
        kind: impl Into<String>,
        content: impl Into<String>,
        source_id: Option<String>,
    ) -> Self {
        Self {
            role: role.into(),
            kind: kind.into(),
            content: content.into(),
            source_id,
        }
    }
}

/// Extracts VMM profile sync effects from one hook dispatch report.
/// 从一次 hook 调度报告中提取 VMM 画像同步效果。
pub fn vmm_profile_sync_effect_from_report(
    report: &HookDispatchReport,
) -> Option<VmmProfileSyncHookEffect> {
    report
        .metadata
        .get(HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT)
        .into_iter()
        .flat_map(|values| values.iter())
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .last()
}

/// In-memory state for one logical user-to-assistant turn.
/// 单个用户到助手逻辑回合的内存状态。
#[derive(Debug, Clone)]
struct ActiveConversationTurn {
    /// Stable turn id generated by the workbench runtime.
    /// 工作台运行时生成的稳定回合标识。
    turn_id: String,
    /// First visible user content that opened the turn.
    /// 打开该回合的首条可见用户内容。
    user_content: String,
    /// Timeline items collected before the final assistant answer.
    /// 最终助手回答前收集到的 timeline 项。
    timeline: Vec<ConversationTimelineItem>,
    /// Prompt entry ids that were already recorded when queued during this turn.
    /// 在该回合排队时已经记录过的提示词条目标识集合。
    recorded_prompt_entry_ids: std::collections::BTreeSet<String>,
}

impl ActiveConversationTurn {
    /// Builds one active turn from the first user prompt.
    /// 根据首条用户提示构建一个活动回合。
    fn new(turn_id: impl Into<String>, user_content: impl Into<String>) -> Self {
        Self {
            turn_id: turn_id.into(),
            user_content: user_content.into(),
            timeline: Vec::new(),
            recorded_prompt_entry_ids: std::collections::BTreeSet::new(),
        }
    }
}

/// Session-level conversation hook dispatcher and turn assembler.
/// 会话级会话 hook 调度器与回合组装器。
#[derive(Clone, Default)]
pub struct ConversationHookRuntime {
    /// Shared hook dispatcher used by every conversation hook point.
    /// 所有会话 hook 点共用的 hook 调度器。
    dispatcher: HookDispatcher,
    /// Active logical turns keyed by session id.
    /// 按会话标识索引的活动逻辑回合。
    active_turns: BTreeMap<SessionId, ActiveConversationTurn>,
    /// Monotonic sequence used to generate local turn ids.
    /// 用于生成本地回合标识的单调序列。
    next_turn_sequence: u64,
}

impl ConversationHookRuntime {
    /// Builds an empty conversation hook runtime.
    /// 构建一个空的会话 hook 运行时。
    pub fn new() -> Self {
        let mut runtime = Self::default();
        runtime.register_builtin_points();
        runtime
    }

    /// Returns the shared hook dispatcher for registration and inspection.
    /// 返回用于注册与检查的共享 hook 调度器。
    #[allow(dead_code)]
    pub fn dispatcher(&self) -> &HookDispatcher {
        &self.dispatcher
    }

    /// Returns the mutable shared hook dispatcher for registration.
    /// 返回用于注册的可变共享 hook 调度器。
    #[allow(dead_code)]
    pub fn dispatcher_mut(&mut self) -> &mut HookDispatcher {
        &mut self.dispatcher
    }

    /// Returns deterministic snapshots for handlers registered on the precheck hook point.
    /// 返回注册在 precheck hook 点位上的确定性处理器快照。
    pub fn precheck_hook_snapshots(&self) -> Vec<HookRegistrationSnapshot> {
        self.dispatcher
            .registry()
            .registration_snapshots(Some(HOOK_CONVERSATION_TURN_PRECHECK))
    }

    /// Returns whether a logical user turn is currently open for the session.
    /// 返回该会话当前是否存在打开中的逻辑用户回合。
    pub fn has_active_turn(&self, session_id: SessionId) -> bool {
        self.active_turns.contains_key(&session_id)
    }

    /// Registers every built-in conversation hook point descriptor.
    /// 注册所有内置会话 hook 点位描述。
    fn register_builtin_points(&mut self) {
        let descriptors = conversation_hook_point_descriptors();
        self.dispatcher
            .register_points(descriptors)
            .expect("built-in conversation hook points should be valid");
    }

    /// Opens or updates the logical turn for one user prompt and fires precheck/user hooks.
    /// 为一条用户提示打开或更新逻辑回合，并触发 precheck/user hook。
    pub fn prepare_user_prompt_precheck(
        &mut self,
        session_id: SessionId,
        prompt_entry_id: &str,
        project_id: Option<String>,
        workspace_path: String,
        source: PendingSessionPromptSource,
        system_prompt_hook_phase: SystemPromptHookPhase,
        prompt: &str,
        conversation_state: &SessionConversationState,
    ) -> ConversationPrecheckRequest {
        let event_kind = pending_prompt_source_event_kind(source);
        let was_recorded_while_queued = self
            .active_turns
            .get(&session_id)
            .is_some_and(|turn| turn.recorded_prompt_entry_ids.contains(prompt_entry_id));
        let is_new_turn = !self.active_turns.contains_key(&session_id);
        let turn_id = if is_new_turn {
            self.next_turn_sequence = self.next_turn_sequence.saturating_add(1);
            let turn_id = format!("session-{session_id}-turn-{}", self.next_turn_sequence);
            self.active_turns.insert(
                session_id,
                ActiveConversationTurn::new(turn_id.clone(), prompt.to_string()),
            );
            turn_id
        } else if was_recorded_while_queued {
            self.active_turns
                .get(&session_id)
                .map(|turn| turn.turn_id.clone())
                .unwrap_or_default()
        } else {
            let turn = self
                .active_turns
                .get_mut(&session_id)
                .expect("active turn should exist after contains_key");
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                event_kind,
                prompt.trim().to_string(),
                None,
            ));
            turn.turn_id.clone()
        };

        let payload = json!({
            "session_id": session_id.to_string(),
            "project_id": project_id,
            "workspace_path": workspace_path,
            "turn_id": turn_id,
            "event_kind": event_kind,
            "is_new_turn": is_new_turn,
            "system_prompt_hook_phase": system_prompt_hook_phase.as_str(),
            "was_recorded_while_queued": was_recorded_while_queued,
            "user_content": prompt,
            "visible_history_count": visible_history_count(conversation_state),
            "message_count": conversation_state.messages.len(),
            "record_count": conversation_state.records.len(),
        });
        if !was_recorded_while_queued {
            self.dispatch_json(
                HOOK_CONVERSATION_USER_EVENT,
                session_id,
                payload.clone(),
                HookScope::Session,
            );
        }
        ConversationPrecheckRequest::new(self.dispatcher.clone(), session_id, payload, turn_id)
    }

    /// Records a queued user prompt as timeline-only input without running precheck.
    /// 将已排队用户提示记录为仅 timeline 输入，不运行 precheck。
    pub fn record_queued_user_prompt(
        &mut self,
        session_id: SessionId,
        prompt_entry_id: &str,
        project_id: Option<String>,
        workspace_path: String,
        source: PendingSessionPromptSource,
        prompt: &str,
    ) {
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        let event_kind = pending_prompt_source_event_kind(source);
        if let Some(turn) = self.active_turns.get_mut(&session_id)
            && turn
                .recorded_prompt_entry_ids
                .insert(prompt_entry_id.to_string())
        {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                event_kind,
                prompt.trim().to_string(),
                Some(prompt_entry_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_USER_EVENT,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "workspace_path": workspace_path,
                "turn_id": turn_id,
                "event_kind": event_kind,
                "is_new_turn": false,
                "queued": true,
                "prompt_entry_id": prompt_entry_id,
                "user_content": prompt,
            }),
            HookScope::Session,
        );
    }

    /// Records an ask/question prompt opened by the runtime.
    /// 记录运行时打开的 ask/question 提示。
    pub fn record_question_asked(
        &mut self,
        session_id: SessionId,
        request_id: &str,
        questions: &[QuestionPrompt],
    ) {
        let content = questions
            .iter()
            .map(question_prompt_summary)
            .collect::<Vec<_>>()
            .join("\n");
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "assistant",
                "ask",
                content.clone(),
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_ASKED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
                "questions": question_prompt_payloads(questions),
                "content": content,
            }),
            HookScope::Session,
        );
    }

    /// Records user answers submitted to the runtime ask/question prompt.
    /// 记录用户提交给运行时 ask/question 提示的回答。
    pub fn record_question_answered(
        &mut self,
        session_id: SessionId,
        request_id: &str,
        questions: &[QuestionPrompt],
        answers: &[Vec<String>],
    ) {
        let content = answers
            .iter()
            .enumerate()
            .map(|(index, answer)| {
                let title = questions
                    .get(index)
                    .map(|question| question.header.as_str())
                    .unwrap_or("question");
                format!("{title}: {}", answer.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                "ask_answer",
                content.clone(),
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_ANSWERED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
                "answers": answers,
                "content": content,
            }),
            HookScope::Session,
        );
    }

    /// Records user rejection of the runtime ask/question prompt.
    /// 记录用户拒绝运行时 ask/question 提示。
    pub fn record_question_rejected(&mut self, session_id: SessionId, request_id: &str) {
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                "ask_rejected",
                "The user rejected the runtime question.",
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_REJECTED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
            }),
            HookScope::Session,
        );
    }

    /// Records a tool runtime event when it belongs to a conversation turn.
    /// 当工具运行时事件属于会话回合时记录该事件。
    pub fn record_tool_runtime_event(&mut self, session_id: SessionId, event: &RuntimeEvent) {
        let (hook_point, timeline_kind) = match event.kind {
            RuntimeEventKind::ToolStarted => (HOOK_CONVERSATION_TOOL_STARTED, "tool_started"),
            RuntimeEventKind::ToolFinished => (HOOK_CONVERSATION_TOOL_FINISHED, "tool_finished"),
            _ => return,
        };
        let Some(turn_id) = self
            .active_turns
            .get(&session_id)
            .map(|turn| turn.turn_id.clone())
        else {
            return;
        };
        let source_id = event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_call_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        // Tool events still dispatch hook notifications, but they do not belong in the user-turn timeline.
        // 工具事件仍会派发 hook 通知，但不进入用户回合 timeline。
        self.dispatch_json(
            hook_point,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "kind": timeline_kind,
                "title": event.title,
                "detail": event.detail,
                "source_id": source_id,
                "metadata": event.metadata,
            }),
            HookScope::Session,
        );
    }

    /// Completes one logical turn and fires the completed hook payload.
    /// 完成一个逻辑回合并触发 completed hook payload。
    pub fn complete_turn(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
    ) -> (String, Vec<VmmWritebackHookEffect>) {
        self.complete_turn_with_status(
            session_id,
            project_id,
            assistant_content,
            provider_request_count,
            conversation_state,
            ConversationTurnCompletionStatus::Completed,
            0,
        )
    }

    /// Dispatches a completed-turn hook payload with an explicit logical completion state.
    /// 使用显式逻辑完成状态调度完成回合 hook 载荷。
    pub fn complete_turn_with_status(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
        completion_status: ConversationTurnCompletionStatus,
        pending_subagent_count: usize,
    ) -> (String, Vec<VmmWritebackHookEffect>) {
        let (completed_turn_id, request) = self.prepare_turn_completion_dispatch_with_status(
            session_id,
            project_id,
            assistant_content,
            provider_request_count,
            conversation_state,
            completion_status,
            pending_subagent_count,
        );
        let report = request.run_blocking();
        (
            completed_turn_id,
            report
                .as_ref()
                .map(vmm_writeback_effects_from_report)
                .unwrap_or_default(),
        )
    }

    /// Completes one logical turn locally and returns a dispatch request that can run off the critical path.
    /// 在本地闭合一个逻辑回合，并返回可脱离关键路径执行的调度请求。
    pub fn prepare_turn_completion_dispatch(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
    ) -> (String, ConversationCompletionRequest) {
        self.prepare_turn_completion_dispatch_with_status(
            session_id,
            project_id,
            assistant_content,
            provider_request_count,
            conversation_state,
            ConversationTurnCompletionStatus::Completed,
            0,
        )
    }

    /// Prepares one completed-turn hook payload while preserving unfinished logical turns when required.
    /// 准备一条完成回合 hook 载荷，并在需要时保留尚未结束的逻辑回合。
    pub fn prepare_turn_completion_dispatch_with_status(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
        completion_status: ConversationTurnCompletionStatus,
        pending_subagent_count: usize,
    ) -> (String, ConversationCompletionRequest) {
        let turn = if completion_status.is_final() {
            self.active_turns
                .remove(&session_id)
                .unwrap_or_else(|| self.reconstruct_turn_from_state(session_id, conversation_state))
        } else if let Some(turn) = self.active_turns.get(&session_id) {
            turn.clone()
        } else {
            let turn = self.reconstruct_turn_from_state(session_id, conversation_state);
            self.active_turns.insert(session_id, turn.clone());
            turn
        };
        let completed_turn_id = turn.turn_id.clone();
        let turn_complete = completion_status.is_final();
        let payload = json!({
            "session_id": session_id.to_string(),
            "project_id": project_id,
            "turn_id": turn.turn_id,
            "user_content": turn.user_content,
            "timeline": turn.timeline,
            "assistant_content": assistant_content,
            "provider_request_count": provider_request_count,
            "completion_status": completion_status.as_str(),
            "turn_complete": turn_complete,
            "pending_subagent_count": pending_subagent_count,
            "message_count": conversation_state.messages.len(),
            "record_count": conversation_state.records.len(),
            "turn_messages": &conversation_state.messages,
            "turn_records": &conversation_state.records,
            "conversation_state": conversation_state,
        });
        (
            completed_turn_id.clone(),
            ConversationCompletionRequest::new(
                self.dispatcher.clone(),
                session_id,
                payload,
                completed_turn_id,
            ),
        )
    }

    /// Dispatches one prepared completed-turn payload synchronously for legacy callers.
    /// 为旧调用方同步调度一条已准备的完成回合 payload。
    fn dispatch_completion_blocking(
        dispatcher: HookDispatcher,
        session_id: SessionId,
        payload: Value,
    ) -> Option<HookDispatchReport> {
        Self::dispatch_json_blocking_with_dispatcher(
            dispatcher,
            session_id,
            payload,
            HOOK_CONVERSATION_TURN_COMPLETED,
            COMPLETION_DISPATCH_TIMEOUT_MS,
        )
    }

    /// Terminates one logical turn and fires the terminated hook payload.
    /// 终止一个逻辑回合并触发 terminated hook payload。
    pub fn terminate_turn(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        reason: &str,
        provider_request_count: usize,
    ) {
        let turn = self.active_turns.remove(&session_id);
        self.dispatch_json(
            HOOK_CONVERSATION_TURN_TERMINATED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "turn_id": turn.as_ref().map(|turn| turn.turn_id.as_str()),
                "user_content": turn.as_ref().map(|turn| turn.user_content.as_str()),
                "timeline": turn.as_ref().map(|turn| &turn.timeline),
                "reason": reason,
                "provider_request_count": provider_request_count,
            }),
            HookScope::Session,
        );
    }

    /// Records a completed compaction boundary.
    /// 记录一次已完成的压缩边界。
    pub fn record_compacted(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        trigger: &str,
        fallback_used: bool,
        provider_request_count: usize,
        summary: &str,
    ) {
        self.dispatch_json(
            HOOK_CONVERSATION_COMPACTED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "trigger": trigger,
                "fallback_used": fallback_used,
                "provider_request_count": provider_request_count,
                "summary": summary,
            }),
            HookScope::Session,
        );
    }

    /// Reconstructs a minimal turn from canonical state when active state is missing.
    /// 当活动状态缺失时从规范状态重建一个最小回合。
    fn reconstruct_turn_from_state(
        &mut self,
        session_id: SessionId,
        conversation_state: &SessionConversationState,
    ) -> ActiveConversationTurn {
        self.next_turn_sequence = self.next_turn_sequence.saturating_add(1);
        let user_content = conversation_state
            .messages
            .iter()
            .rev()
            .find_map(|message| match &message.payload {
                SessionConversationPayload::UserText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        ActiveConversationTurn::new(
            format!("session-{session_id}-turn-{}", self.next_turn_sequence),
            user_content,
        )
    }

    /// Ensures runtime-only events still have a turn container.
    /// 确保仅运行时事件仍然拥有回合容器。
    fn ensure_turn_for_runtime_event(&mut self, session_id: SessionId) -> String {
        if let Some(turn) = self.active_turns.get(&session_id) {
            return turn.turn_id.clone();
        }
        self.next_turn_sequence = self.next_turn_sequence.saturating_add(1);
        let turn_id = format!("session-{session_id}-turn-{}", self.next_turn_sequence);
        self.active_turns.insert(
            session_id,
            ActiveConversationTurn::new(turn_id.clone(), String::new()),
        );
        turn_id
    }

    /// Dispatches one JSON hook payload without blocking the UI thread.
    /// 在不阻塞 UI 线程的情况下调度一条 JSON hook payload。
    fn dispatch_json(
        &self,
        point: &'static str,
        session_id: SessionId,
        payload: Value,
        scope: HookScope,
    ) {
        let dispatcher = self.dispatcher.clone();
        let context = HookContext {
            run_id: format!("{point}:{session_id}"),
            point: point.to_string(),
            scope,
            actor: HookActor::new("vulcan-code-runtime"),
            workspace_id: None,
            project_id: None,
            session_id: Some(session_id.to_string()),
            conversation_id: Some(session_id.to_string()),
            user_id: None,
            capabilities: Default::default(),
            metadata: BTreeMap::new(),
        };
        let request = HookDispatchRequest::new(context, HookPayload { value: payload });
        spawn_hook_dispatch(dispatcher, request);
    }

    /// Dispatches one JSON hook payload synchronously through the provided dispatcher.
    /// 通过给定调度器同步调度一条 JSON hook payload。
    fn dispatch_json_blocking_with_dispatcher(
        dispatcher: HookDispatcher,
        session_id: SessionId,
        payload: Value,
        point: &'static str,
        timeout_ms: u64,
    ) -> Option<HookDispatchReport> {
        let context = HookContext {
            run_id: format!("{point}:{session_id}"),
            point: point.to_string(),
            scope: HookScope::Session,
            actor: HookActor::new("vulcan-code-runtime"),
            workspace_id: None,
            project_id: None,
            session_id: Some(session_id.to_string()),
            conversation_id: Some(session_id.to_string()),
            user_id: None,
            capabilities: Default::default(),
            metadata: BTreeMap::new(),
        };
        let request = HookDispatchRequest::new(context, HookPayload { value: payload });
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            runtime.block_on(async move {
                tokio::time::timeout(
                    Duration::from_millis(timeout_ms),
                    dispatcher.dispatch(request),
                )
                .await
                .ok()
                .and_then(Result::ok)
            })
        })
        .join()
        .ok()
        .flatten()
    }
}

/// Prepared asynchronous precheck request that can run outside the UI message handler.
/// 可在 UI 消息处理器之外运行的已准备异步 precheck 请求。
#[derive(Clone)]
pub struct ConversationPrecheckRequest {
    /// Dispatcher snapshot used for this precheck run.
    /// 当前 precheck 运行使用的调度器快照。
    dispatcher: HookDispatcher,
    /// Session that owns the precheck request.
    /// 拥有该 precheck 请求的会话。
    session_id: SessionId,
    /// JSON payload sent to the hook point.
    /// 发送给 hook 点位的 JSON 载荷。
    payload: Value,
    /// Logical turn id associated with this precheck request.
    /// 与该 precheck 请求关联的逻辑回合标识。
    turn_id: String,
}

impl ConversationPrecheckRequest {
    /// Builds one prepared asynchronous precheck request.
    /// 构建一条已准备的异步 precheck 请求。
    fn new(
        dispatcher: HookDispatcher,
        session_id: SessionId,
        payload: Value,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            dispatcher,
            session_id,
            payload,
            turn_id: turn_id.into(),
        }
    }

    /// Returns the logical turn id associated with this precheck request.
    /// 返回与该 precheck 请求关联的逻辑回合标识。
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    /// Runs the precheck hook dispatch on a worker thread and returns a non-panicking report result.
    /// 在工作线程执行 precheck hook 调度，并返回不会 panic 的报告结果。
    pub async fn run(self) -> ConversationPrecheckOutcome {
        self.run_on_worker_thread().await
    }

    /// Completes this prepared precheck without dispatching handlers.
    /// 在不调度处理器的情况下完成这条已准备的 precheck。
    pub fn skip(self) -> ConversationPrecheckOutcome {
        ConversationPrecheckOutcome {
            session_id: self.session_id,
            turn_id: self.turn_id,
            report: Ok(HookDispatchReport::empty(HookPayload {
                value: self.payload,
            })),
        }
    }

    /// Runs the async hook dispatcher inside a private Tokio runtime so the UI executor never blocks.
    /// 在私有 Tokio 运行时中执行异步 hook 调度，避免阻塞 UI 执行器。
    async fn run_on_worker_thread(self) -> ConversationPrecheckOutcome {
        let session_id = self.session_id;
        let turn_id = self.turn_id.clone();
        let context = HookContext {
            run_id: format!("{}:{}", HOOK_CONVERSATION_TURN_PRECHECK, session_id),
            point: HOOK_CONVERSATION_TURN_PRECHECK.to_string(),
            scope: HookScope::Session,
            actor: HookActor::new("vulcan-code-runtime"),
            workspace_id: None,
            project_id: None,
            session_id: Some(session_id.to_string()),
            conversation_id: Some(session_id.to_string()),
            user_id: None,
            capabilities: Default::default(),
            metadata: BTreeMap::new(),
        };
        let request = HookDispatchRequest::new(
            context,
            HookPayload {
                value: self.payload,
            },
        );
        let (sender, receiver) = oneshot::channel();
        let spawn_result = std::thread::Builder::new()
            .name(format!("vulcan-precheck-{session_id}"))
            .spawn(move || {
                let result = run_precheck_dispatch_on_private_runtime(self.dispatcher, request);
                let _ = sender.send(result);
            });
        if let Err(error) = spawn_result {
            return ConversationPrecheckOutcome {
                session_id,
                turn_id,
                report: Err(format!("precheck hook thread failed to start: {error}")),
            };
        }
        let report = receiver
            .await
            .unwrap_or_else(|_| Err(String::from("precheck hook dispatch thread disconnected")));
        ConversationPrecheckOutcome {
            session_id,
            turn_id,
            report,
        }
    }
}

/// Runs one precheck dispatch inside a private Tokio runtime on the already-created worker thread.
/// 在已创建的工作线程内使用私有 Tokio 运行时执行一次 precheck 调度。
fn run_precheck_dispatch_on_private_runtime(
    dispatcher: HookDispatcher,
    request: HookDispatchRequest,
) -> Result<HookDispatchReport, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("precheck hook runtime failed: {error}"))?;
        runtime.block_on(async move {
            tokio::time::timeout(
                Duration::from_millis(PRECHECK_DISPATCH_TIMEOUT_MS),
                dispatcher.dispatch(request),
            )
            .await
            .map_err(|_| String::from("precheck hook dispatch timed out"))?
            .map_err(|error| error.to_string())
        })
    }))
    .unwrap_or_else(|_| Err(String::from("precheck hook dispatch thread panicked")))
}

/// Result produced by one asynchronous conversation precheck dispatch.
/// 单次异步会话 precheck 调度产生的结果。
#[derive(Debug, Clone)]
pub struct ConversationPrecheckOutcome {
    /// Session that owns the completed precheck.
    /// 拥有已完成 precheck 的会话。
    pub session_id: SessionId,
    /// Hook dispatch report or a non-fatal error message.
    /// hook 调度报告或非致命错误消息。
    pub report: Result<HookDispatchReport, String>,
    /// Logical turn id associated with this precheck.
    /// 与该 precheck 关联的逻辑回合标识。
    pub turn_id: String,
}

/// Prepared completed-turn hook request that can run after the next queued prompt starts.
/// 可在下一条排队提示词启动后执行的已准备完成回合 hook 请求。
#[derive(Clone)]
pub struct ConversationCompletionRequest {
    /// Dispatcher snapshot used for this completed-turn hook.
    /// 当前完成回合 hook 使用的调度器快照。
    dispatcher: HookDispatcher,
    /// Session that owns the completed turn.
    /// 拥有已完成回合的会话。
    session_id: SessionId,
    /// JSON payload sent to the completed-turn hook point.
    /// 发送给完成回合 hook 点位的 JSON 载荷。
    payload: Value,
    /// Logical turn id associated with this completed turn.
    /// 与该完成回合关联的逻辑回合标识。
    turn_id: String,
}

impl ConversationCompletionRequest {
    /// Builds one prepared completed-turn hook request.
    /// 构建一条已准备的完成回合 hook 请求。
    fn new(
        dispatcher: HookDispatcher,
        session_id: SessionId,
        payload: Value,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            dispatcher,
            session_id,
            payload,
            turn_id: turn_id.into(),
        }
    }

    /// Returns the session that owns the completed turn.
    /// 返回拥有该完成回合的会话。
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Returns the logical turn id associated with this completed-turn request.
    /// 返回与这条完成回合请求关联的逻辑回合标识。
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    /// Runs the completed-turn hook dispatch on a worker thread.
    /// 在工作线程执行完成回合 hook 调度。
    pub async fn run(self) -> ConversationCompletionOutcome {
        self.run_on_worker_thread().await
    }

    /// Runs the completed-turn hook dispatch synchronously for legacy callers.
    /// 为旧调用方同步执行完成回合 hook 调度。
    fn run_blocking(self) -> Option<HookDispatchReport> {
        ConversationHookRuntime::dispatch_completion_blocking(
            self.dispatcher,
            self.session_id,
            self.payload,
        )
    }

    /// Runs the completed-turn hook dispatch in a private worker thread.
    /// 在私有工作线程中执行完成回合 hook 调度。
    async fn run_on_worker_thread(self) -> ConversationCompletionOutcome {
        let session_id = self.session_id;
        let turn_id = self.turn_id.clone();
        let (sender, receiver) = oneshot::channel();
        let spawn_result = std::thread::Builder::new()
            .name(format!("vulcan-completion-{session_id}"))
            .spawn(move || {
                let result = self.run_blocking();
                let _ = sender.send(result);
            });
        if let Err(error) = spawn_result {
            return ConversationCompletionOutcome {
                session_id,
                turn_id,
                report: Err(format!(
                    "completed-turn hook thread failed to start: {error}"
                )),
                vmm_writeback_effects: Vec::new(),
            };
        }
        let report = receiver
            .await
            .map_err(|_| String::from("completed-turn hook dispatch thread disconnected"))
            .and_then(|report| {
                report
                    .ok_or_else(|| String::from("completed-turn hook dispatch produced no report"))
            });
        let vmm_writeback_effects = report
            .as_ref()
            .map(vmm_writeback_effects_from_report)
            .unwrap_or_default();
        ConversationCompletionOutcome {
            session_id,
            turn_id,
            report,
            vmm_writeback_effects,
        }
    }
}

/// Result produced by one asynchronous completed-turn hook dispatch.
/// 单次异步完成回合 hook 调度产生的结果。
#[derive(Debug, Clone)]
pub struct ConversationCompletionOutcome {
    /// Session that owns the completed turn.
    /// 拥有已完成回合的会话。
    pub session_id: SessionId,
    /// Logical turn id associated with the completed turn.
    /// 与已完成回合关联的逻辑回合标识。
    pub turn_id: String,
    /// Hook dispatch report or a non-fatal error message.
    /// hook 调度报告或非致命错误消息。
    pub report: Result<HookDispatchReport, String>,
    /// VMM writeback effects extracted from the completed-turn hook report.
    /// 从完成回合 hook 报告中提取的 VMM 写回效果。
    pub vmm_writeback_effects: Vec<VmmWritebackHookEffect>,
}

/// Extracts VMM writeback effects from one hook dispatch report.
/// 从一次 hook 调度报告中提取 VMM 写回效果。
fn vmm_writeback_effects_from_report(report: &HookDispatchReport) -> Vec<VmmWritebackHookEffect> {
    report
        .metadata
        .get(HOOK_METADATA_VMM_WRITEBACK_EFFECTS)
        .into_iter()
        .flat_map(|values| values.iter())
        .flat_map(|value| {
            if let Some(items) = value.as_array() {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect::<Vec<_>>()
            } else {
                serde_json::from_value(value.clone())
                    .ok()
                    .into_iter()
                    .collect()
            }
        })
        .collect()
}

/// Builds descriptors for every built-in conversation hook point.
/// 构建所有内置会话 hook 点位的描述。
fn conversation_hook_point_descriptors() -> Vec<HookPointDescriptor> {
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
fn spawn_hook_dispatch(dispatcher: HookDispatcher, request: HookDispatchRequest) {
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
fn pending_prompt_source_event_kind(source: PendingSessionPromptSource) -> &'static str {
    match source {
        PendingSessionPromptSource::UserFollowup => "user_followup",
        PendingSessionPromptSource::UserInterrupt => "user_interrupt",
        PendingSessionPromptSource::SessionMailbox => "session_mailbox",
    }
}

/// Counts host-visible canonical history messages.
/// 统计宿主可见的规范历史消息数量。
fn visible_history_count(conversation_state: &SessionConversationState) -> usize {
    conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .count()
}

/// Builds a compact question prompt summary.
/// 构建一条精简问题提示摘要。
fn question_prompt_summary(question: &QuestionPrompt) -> String {
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
fn question_prompt_payloads(questions: &[QuestionPrompt]) -> Vec<Value> {
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

#[cfg(test)]
mod tests {
    //! Tests for conversation hook turn assembly.
    //! 会话 hook 回合组装测试。

    use super::*;
    use crate::conversation_protocol::{SessionConversationMessage, SessionConversationPayload};

    #[test]
    fn turn_completion_keeps_supplement_inside_timeline() {
        let mut runtime = ConversationHookRuntime::new();
        let session_id = SessionId::from_u128(42);
        let state = SessionConversationState::default();

        runtime.prepare_user_prompt_precheck(
            session_id,
            "entry-1",
            Some(String::from("project")),
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "first",
            &state,
        );
        runtime.prepare_user_prompt_precheck(
            session_id,
            "entry-2",
            Some(String::from("project")),
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserInterrupt,
            SystemPromptHookPhase::SubsequentSession,
            "supplement",
            &state,
        );

        let turn = runtime
            .active_turns
            .get(&session_id)
            .expect("turn should be active");
        assert_eq!(turn.user_content, "first");
        assert_eq!(turn.timeline.len(), 1);
        assert_eq!(turn.timeline[0].kind, "user_interrupt");
        assert_eq!(turn.timeline[0].content, "supplement");
    }

    /// Verifies orphan tool events do not synthesize empty user turns.
    /// 验证孤立工具事件不会合成空用户回合。
    #[test]
    fn orphan_tool_runtime_event_does_not_create_user_turn() {
        let mut runtime = ConversationHookRuntime::new();
        let session_id = SessionId::from_u128(143);

        runtime.record_tool_runtime_event(
            session_id,
            &RuntimeEvent::new(RuntimeEventKind::ToolStarted, "tool", "started"),
        );

        assert!(!runtime.active_turns.contains_key(&session_id));
    }

    /// Verifies tool events remain hook notifications instead of completed-turn timeline rows.
    /// 验证工具事件只保留为 hook 通知，不进入完成回合 timeline。
    #[test]
    fn tool_runtime_event_stays_out_of_completed_timeline() {
        let mut runtime = ConversationHookRuntime::new();
        let session_id = SessionId::from_u128(144);
        let state = SessionConversationState::default();

        runtime.prepare_user_prompt_precheck(
            session_id,
            "entry-1",
            Some(String::from("project")),
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "first",
            &state,
        );
        runtime.record_tool_runtime_event(
            session_id,
            &RuntimeEvent::new(RuntimeEventKind::ToolFinished, "tool", "finished"),
        );

        let (_turn_id, request) =
            runtime.prepare_turn_completion_dispatch(session_id, None, "done", 1, &state);
        let timeline = request
            .payload
            .get("timeline")
            .and_then(Value::as_array)
            .expect("completion payload should expose a timeline array");
        assert!(timeline.is_empty());
    }

    /// Verifies unfinished subagent completion dispatch keeps the original user turn open.
    /// 验证未完成的子代理完成调度会保留原始用户回合。
    #[test]
    fn awaiting_subagent_completion_keeps_active_turn() {
        let mut runtime = ConversationHookRuntime::new();
        let session_id = SessionId::from_u128(145);
        let state = SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("m1"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("spawn two subagents"),
                    attachments: Vec::new(),
                },
            }],
            ..SessionConversationState::default()
        };
        runtime.prepare_user_prompt_precheck(
            session_id,
            "entry-1",
            Some(String::from("project")),
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "spawn two subagents",
            &state,
        );

        let (awaiting_turn_id, awaiting_request) = runtime
            .prepare_turn_completion_dispatch_with_status(
                session_id,
                Some(String::from("project")),
                "children are still running",
                1,
                &state,
                ConversationTurnCompletionStatus::AwaitingSubagents,
                2,
            );

        assert!(runtime.has_active_turn(session_id));
        assert_eq!(
            awaiting_request
                .payload
                .get("completion_status")
                .and_then(Value::as_str),
            Some("awaiting_subagents")
        );
        assert_eq!(
            awaiting_request
                .payload
                .get("turn_complete")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            awaiting_request
                .payload
                .get("pending_subagent_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert!(
            awaiting_request
                .payload
                .get("turn_messages")
                .and_then(Value::as_array)
                .is_some_and(|messages| messages.len() == 1)
        );

        let (completed_turn_id, completed_request) = runtime.prepare_turn_completion_dispatch(
            session_id,
            Some(String::from("project")),
            "all children returned",
            2,
            &state,
        );

        assert_eq!(awaiting_turn_id, completed_turn_id);
        assert!(!runtime.has_active_turn(session_id));
        assert_eq!(
            completed_request
                .payload
                .get("completion_status")
                .and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            completed_request
                .payload
                .get("turn_complete")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn completed_turn_falls_back_to_latest_user_from_conversation_state() {
        let mut runtime = ConversationHookRuntime::new();
        let session_id = SessionId::from_u128(43);
        let state = SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("m1"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("latest user"),
                    attachments: Vec::new(),
                },
            }],
            ..SessionConversationState::default()
        };

        let turn = runtime.reconstruct_turn_from_state(session_id, &state);
        assert_eq!(turn.user_content, "latest user");
    }

    #[tokio::test]
    async fn precheck_hook_injections_are_returned_to_send_pipeline() {
        use std::sync::Arc;
        use vulcan_context_injection::{InjectionInsert, InjectionKind};
        use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

        let mut runtime = ConversationHookRuntime::new();
        runtime
            .dispatcher_mut()
            .register(
                HookRegistration::new(
                    "injector",
                    HOOK_CONVERSATION_TURN_PRECHECK,
                    HookMode::Transform,
                ),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult {
                        injections: vec![InjectionInsert::single(
                            InjectionKind::Transient,
                            "test_inject",
                            "test description",
                            "remember this",
                            Some(1),
                        )],
                        ..HookResult::default()
                    })
                })),
            )
            .expect("hook registration should succeed");

        let outcome = runtime
            .prepare_user_prompt_precheck(
                SessionId::from_u128(44),
                "entry-1",
                None,
                String::from("D:/workspace"),
                PendingSessionPromptSource::UserFollowup,
                SystemPromptHookPhase::FirstSession,
                "hello",
                &SessionConversationState::default(),
            )
            .run()
            .await;
        let report = outcome.report.expect("precheck hook should complete");

        assert_eq!(report.injections.len(), 1);
        assert_eq!(report.injections[0].tag, "test_inject");
    }

    #[tokio::test]
    async fn precheck_hook_timeout_records_failure_without_hanging() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

        const TEST_HOOK_TIMEOUT_MS: u64 = 250;

        let mut runtime = ConversationHookRuntime::new();
        let mut registration =
            HookRegistration::new("slow", HOOK_CONVERSATION_TURN_PRECHECK, HookMode::Transform);
        registration.timeout_ms = Some(TEST_HOOK_TIMEOUT_MS);
        runtime
            .dispatcher_mut()
            .register(
                registration,
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    tokio::time::sleep(Duration::from_millis(TEST_HOOK_TIMEOUT_MS + 500)).await;
                    Ok(HookResult::default())
                })),
            )
            .expect("hook registration should succeed");

        let started_at = Instant::now();
        let outcome = runtime
            .prepare_user_prompt_precheck(
                SessionId::from_u128(45),
                "entry-1",
                None,
                String::from("D:/workspace"),
                PendingSessionPromptSource::UserFollowup,
                SystemPromptHookPhase::FirstSession,
                "hello",
                &SessionConversationState::default(),
            )
            .run()
            .await;

        let report = outcome
            .report
            .expect("registration-level timeout should still return a dispatch report");
        assert!(report.injections.is_empty());
        assert!(!report.failures.is_empty());
        assert!(started_at.elapsed() < Duration::from_millis(TEST_HOOK_TIMEOUT_MS + 1_500));
    }

    #[test]
    fn effect_hook_dispatch_runs_without_ambient_tokio_runtime() {
        use std::sync::{Arc, mpsc};
        use std::time::Duration;
        use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

        let mut runtime = ConversationHookRuntime::new();
        let (sender, receiver) = mpsc::channel();
        let sender = Arc::new(sender);
        runtime
            .dispatcher_mut()
            .register(
                HookRegistration::new(
                    "effect-listener",
                    HOOK_CONVERSATION_TURN_COMPLETED,
                    HookMode::Effect,
                ),
                Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                    let sender = Arc::clone(&sender);
                    async move {
                        let assistant_content = invocation
                            .payload
                            .value
                            .get("assistant_content")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let _ = sender.send(assistant_content);
                        Ok(HookResult::default())
                    }
                })),
            )
            .expect("hook registration should succeed");

        let session_id = SessionId::from_u128(46);
        runtime.prepare_user_prompt_precheck(
            session_id,
            "entry-1",
            None,
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "hello",
            &SessionConversationState::default(),
        );
        let _ = runtime.complete_turn(
            session_id,
            None,
            "done",
            1,
            &SessionConversationState::default(),
        );

        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("effect hook should run"),
            "done"
        );
    }
}
