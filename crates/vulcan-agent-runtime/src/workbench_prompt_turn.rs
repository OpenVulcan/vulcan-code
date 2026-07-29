//! Shared prompt-turn reducers used by workbench and service shells.
//! 工作台与服务外壳共用的提示词轮次归约器。

use std::fmt;

use serde_json::Value;
use time::OffsetDateTime;
use vulcan_core::types::ConversationAttachment;
use vulcan_hooks::HookRegistrationSnapshot;
use vulcan_provider_config::config::{ProjectVulcanCodeConfigurationFile, ReasoningEffortLevel};
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
};
use vulcan_session_core::SessionNodeId;
use vulcan_vmm::{VmmConfigDiagnostic, VmmEffectiveConfig};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionMessageEntry, SessionRunPhase,
    SessionRuntimeStateEntry,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::{
    ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionProviderVisibleAgentMessage,
    agent_runtime::{
        context_injection_runtime::{
            collect_builtin_manual_injection_ops_and_sanitized_prompt,
            core_provider_context_injection_from_state_for_turn,
        },
        conversation_hooks::{
            ConversationHookRuntime, ConversationPrecheckOutcome, ConversationPrecheckRequest,
            ConversationProfileOutcome, ConversationProfileRequest,
            vmm_profile_sync_effect_from_report, vmm_recall_batch_effect_from_report,
        },
        events::{RuntimeEvent, RuntimeEventKind},
        vmm_hooks::{
            VMM_PRECHECK_HOOK_ID, stabilize_vmm_profile_system_prompt_blocks,
            vmm_profile_placement_phase,
        },
    },
    conversation_protocol::{
        append_user_text_message, merge_session_messages_with_conversation_state_records,
    },
    session_event_runtime::append_runtime_event_projection_to_state,
};

mod transcript;

#[cfg(test)]
mod tests;

use transcript::{
    append_shared_user_prompt_record_with_attachments, patch_latest_prechecked_user_prompt_record,
};
pub use transcript::{
    build_shared_user_prompt_transcript_body, build_shared_user_prompt_transcript_metadata,
};

/// Shared decision for running or skipping prompt precheck hooks.
/// 提示词 precheck hook 是否运行或跳过的共享决策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedPromptPrecheckPlan {
    /// Whether at least one precheck hook handler is registered.
    /// 是否至少注册了一个 precheck hook 处理器。
    pub has_precheck_hooks: bool,
    /// Whether the prepared precheck request should complete without dispatching handlers.
    /// 已准备的 precheck 请求是否应不调度处理器而直接完成。
    pub should_skip_precheck: bool,
    /// User-facing progress labels that should be shown while precheck runs.
    /// precheck 运行期间应显示给用户的进度标签。
    pub progress_labels: Vec<String>,
}

/// Prepared prompt turn that must continue after profile preparation and optional precheck complete.
/// 需要在画像准备及可选 precheck 完成后继续的提示词回合。
#[derive(Clone)]
pub struct SharedPromptTurnStart {
    /// Session that owns this prompt turn.
    /// 拥有该提示词回合的会话。
    pub session_id: SessionId,
    /// Shared immutable identity used to correlate both preparation lines and the provider turn.
    /// 用于关联两条准备线路及 Provider 回合的共享不可变标识。
    pub preparation_turn_id: String,
    /// Human-readable session title used by diagnostics.
    /// 诊断信息使用的人类可读会话标题。
    pub session_title: String,
    /// Workspace path locked for this prompt turn.
    /// 该提示词回合锁定的工作区路径。
    pub workspace_path: String,
    /// Prompt entry source shared with service queue semantics.
    /// 与服务队列语义共享的提示词来源。
    pub source: PendingSessionPromptSource,
    /// Prepared precheck request returned by the shared hook runtime.
    /// 共享 hook 运行时返回的已准备 precheck 请求。
    pub precheck_request: ConversationPrecheckRequest,
    /// Independent profile preparation request sharing the same prompt-turn identity.
    /// 共享同一提示词回合身份的独立画像准备请求。
    pub profile_request: ConversationProfileRequest,
    /// Prompt text after builtin manual-injection syntax is removed.
    /// 移除内建手动注入语法后的提示词文本。
    pub prompt: String,
    /// Optional model-only guidance appended after visible-user prompt handling.
    /// 可选的仅模型可见引导，会在可见用户提示处理后追加。
    pub hidden_model_guidance: Option<String>,
    /// Provider-visible agent messages carried by this turn without becoming user prompts.
    /// 本回合携带但不会变成用户提示词的 Provider 可见智能体消息。
    pub provider_visible_agent_messages: Vec<ExecutionProviderVisibleAgentMessage>,
    /// Structured attachments bound to the prompt.
    /// 绑定在提示词上的结构化附件。
    pub attachments: Vec<ConversationAttachment>,
    /// Execution mode requested by the shell command.
    /// 外壳命令请求的执行模式。
    pub execution_mode: String,
    /// Reasoning effort requested by the shell command.
    /// 外壳命令请求的推理强度。
    pub reasoning_effort: Option<ReasoningEffortLevel>,
    /// session-boundary boundary events produced when this prompt turn is accepted.
    /// 当前提示词回合被接受时生成的 会话边界边界事件。
    pub boundary_events: Vec<SessionBoundaryEvent>,
    /// Visible user transcript record generated for this turn, when one was appended.
    /// 本回合追加可见用户转录记录时生成的记录标识。
    pub visible_user_prompt_record_id: Option<String>,
    /// Exact canonical user-message id appended before both preparation lines, when persisted.
    /// 当该来源会持久化规范消息时，在两条准备线路前追加的精确规范用户消息标识。
    pub canonical_user_message_id: Option<String>,
}

impl fmt::Debug for SharedPromptTurnStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedPromptTurnStart")
            .field("session_id", &self.session_id)
            .field("preparation_turn_id", &self.preparation_turn_id)
            .field("session_title", &self.session_title)
            .field("workspace_path", &self.workspace_path)
            .field("source", &self.source)
            .field("prompt", &self.prompt)
            .field(
                "provider_visible_agent_messages",
                &self.provider_visible_agent_messages.len(),
            )
            .field("attachments", &self.attachments)
            .field("execution_mode", &self.execution_mode)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("boundary_events", &self.boundary_events.len())
            .field(
                "visible_user_prompt_record_id",
                &self.visible_user_prompt_record_id,
            )
            .field("canonical_user_message_id", &self.canonical_user_message_id)
            .finish_non_exhaustive()
    }
}

/// Result of applying a completed precheck to one shared prompt turn.
/// 将完成的 precheck 应用于一个共享提示词回合后的结果。
pub struct SharedPromptTurnContinuation {
    /// Provider execution request built from the shared prompt turn state.
    /// 基于共享提示词回合状态构建出的 Provider 执行请求。
    pub request: ExecutionPreparationRequest,
    /// Precheck failure detail that shells may surface using their own UI.
    /// 外壳可用自身界面呈现的 precheck 失败详情。
    pub precheck_failure_detail: Option<String>,
    /// Profile preparation failure detail that does not suppress the persisted baseline.
    /// 不会抑制持久化基线的画像准备失败详情。
    pub profile_failure_detail: Option<String>,
}

/// Input required to begin one shared prompt turn.
/// 开始一个共享提示词回合所需的输入。
pub struct SharedPromptTurnBeginInput<'a> {
    /// Session that owns this prompt turn.
    /// 拥有该提示词回合的会话。
    pub session_id: SessionId,
    /// Human-readable session title used by diagnostics.
    /// 诊断信息使用的人类可读会话标题。
    pub session_title: &'a str,
    /// Workspace path locked for this prompt turn.
    /// 该提示词回合锁定的工作区路径。
    pub workspace_path: &'a str,
    /// Prompt entry identifier passed into hook routing.
    /// 传递给 hook 路由的提示词条目标识。
    pub prompt_entry_id: String,
    /// Raw prompt text submitted by the user or queue.
    /// 用户或队列提交的原始提示词文本。
    pub prompt: &'a str,
    /// Optional hidden host guidance appended only to the provider-facing prompt.
    /// 仅追加到 Provider 可见提示词中的可选宿主隐藏引导。
    pub hidden_model_guidance: Option<&'a str>,
    /// Provider-visible agent messages carried by this turn without becoming user prompts.
    /// 本回合携带但不会变成用户提示词的 Provider 可见智能体消息。
    pub provider_visible_agent_messages: Vec<ExecutionProviderVisibleAgentMessage>,
    /// Structured attachments bound to the prompt.
    /// 绑定在提示词上的结构化附件。
    pub attachments: Vec<ConversationAttachment>,
    /// Whether the shared reducer should append the canonical user message.
    /// 共享归约器是否需要追加规范用户消息。
    pub append_canonical_user_message: bool,
    /// Whether the shared reducer should append the visible user transcript record.
    /// 共享归约器是否需要追加可见用户转录记录。
    pub append_visible_user_prompt_record: bool,
    /// Prompt entry source shared with queue routing.
    /// 与队列路由共享的提示词来源。
    pub source: PendingSessionPromptSource,
    /// Execution mode requested by the shell command.
    /// 外壳命令请求的执行模式。
    pub execution_mode: String,
    /// Reasoning effort requested by the shell command.
    /// 外壳命令请求的推理强度。
    pub reasoning_effort: Option<ReasoningEffortLevel>,
}

/// Runtime-state scoped input required to begin one shared prompt turn.
/// 在运行态作用域内开始一个共享提示词回合所需的输入。
pub struct SharedPromptTurnRuntimeBeginInput<'a> {
    /// Session that owns this prompt turn.
    /// 拥有该提示词回合的会话。
    pub session_id: SessionId,
    /// Human-readable session title used by diagnostics.
    /// 诊断信息使用的人类可读会话标题。
    pub session_title: &'a str,
    /// Workspace path locked for this prompt turn.
    /// 该提示词回合锁定的工作区路径。
    pub workspace_path: &'a str,
    /// Prompt entry identifier passed into hook routing.
    /// 传递给 hook 路由的提示词条目标识。
    pub prompt_entry_id: String,
    /// Raw prompt text submitted by the user or queue.
    /// 用户或队列提交的原始提示词文本。
    pub prompt: &'a str,
    /// Optional hidden host guidance appended only to the provider-facing prompt.
    /// 仅追加到 Provider 可见提示词中的可选宿主隐藏引导。
    pub hidden_model_guidance: Option<&'a str>,
    /// Provider-visible agent messages carried by this turn without becoming user prompts.
    /// 本回合携带但不会变成用户提示词的 Provider 可见智能体消息。
    pub provider_visible_agent_messages: Vec<ExecutionProviderVisibleAgentMessage>,
    /// Structured attachments bound to the prompt.
    /// 绑定在提示词上的结构化附件。
    pub attachments: Vec<ConversationAttachment>,
    /// Whether the shared reducer should append the canonical user message.
    /// 共享归约器是否需要追加规范用户消息。
    pub append_canonical_user_message: bool,
    /// Whether the shared reducer should append the visible user transcript record.
    /// 共享归约器是否需要追加可见用户转录记录。
    pub append_visible_user_prompt_record: bool,
    /// Prompt entry source shared with queue routing.
    /// 与队列路由共享的提示词来源。
    pub source: PendingSessionPromptSource,
    /// Execution mode requested by the shell command.
    /// 外壳命令请求的执行模式。
    pub execution_mode: String,
    /// Reasoning effort requested by the shell command.
    /// 外壳命令请求的推理强度。
    pub reasoning_effort: Option<ReasoningEffortLevel>,
    /// Owning project id used by hook routing.
    /// hook 路由使用的所属项目标识。
    pub owning_project_id: Option<String>,
    /// Persisted session messages used when runtime state has not been hydrated yet.
    /// 运行态尚未水合时用于恢复会话状态的持久化消息。
    pub prior_session_messages: Option<&'a [SessionMessageEntry]>,
}

/// Input required to continue one shared prompt turn after independent preparation lines finish.
/// 独立准备线路完成后继续一个共享提示词回合所需的输入。
pub struct SharedPromptTurnContinueInput {
    /// Started prompt turn waiting for both independent preparation outcomes.
    /// 正在等待两条独立准备线路结果的已启动提示词回合。
    pub start: SharedPromptTurnStart,
    /// Completed precheck outcome.
    /// 已完成的 precheck 结果。
    pub precheck_outcome: ConversationPrecheckOutcome,
    /// Completed independent profile preparation outcome.
    /// 已完成的独立画像准备结果。
    pub profile_outcome: ConversationProfileOutcome,
    /// Project-local configuration for the execution workspace.
    /// 执行工作区的项目级配置。
    pub project_configuration: ProjectVulcanCodeConfigurationFile,
    /// Runtime-state fallback reasoning effort.
    /// 运行态回退推理强度。
    pub runtime_reasoning_effort: Option<ReasoningEffortLevel>,
    /// Provider-selection fallback reasoning effort.
    /// Provider 选择层回退推理强度。
    pub provider_reasoning_effort: Option<ReasoningEffortLevel>,
    /// Whether prompt rewrites should patch the visible user transcript record.
    /// 提示词改写时是否同步回写可见用户转录记录。
    pub patch_visible_user_prompt_record: bool,
}

/// Begins one prompt turn using the shared workbench semantics.
/// 使用工作台与服务共用语义开始一个提示词回合。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the shell-owned workspace snapshot being mutated.
/// `workspace_snapshot` 是外壳持有并将被修改的工作区快照。
/// `conversation_hook_runtime` prepares the shared precheck request.
/// `conversation_hook_runtime` 用于准备共享 precheck 请求。
/// `input` carries shell-independent prompt-turn inputs.
/// `input` 携带与外壳无关的提示词回合输入。
///
/// # Returns
/// 返回值
/// A started prompt turn that must continue after precheck completes.
/// 返回需要在 precheck 完成后继续的已启动提示词回合。
pub fn begin_shared_prompt_turn(
    workspace_snapshot: &mut WorkspaceSnapshot,
    conversation_hook_runtime: &mut ConversationHookRuntime,
    input: SharedPromptTurnBeginInput<'_>,
) -> Result<SharedPromptTurnStart, String> {
    let prior_dialogue_messages = workspace_snapshot
        .session_runtime_state
        .get(&input.session_id)
        .map(|runtime_state| {
            merge_session_messages_with_conversation_state_records(
                &[],
                &runtime_state.conversation_state,
            )
        })
        .filter(|messages| !messages.is_empty());
    let owning_project_id = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == input.session_id)
        .and_then(|session| session.project_id.clone())
        .filter(|project_id| !project_id.trim().is_empty());
    let now = current_unix_millis_text();
    let start = {
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .entry(input.session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        begin_shared_prompt_turn_with_runtime_state(
            runtime_state,
            conversation_hook_runtime,
            SharedPromptTurnRuntimeBeginInput {
                session_id: input.session_id,
                session_title: input.session_title,
                workspace_path: input.workspace_path,
                prompt_entry_id: input.prompt_entry_id,
                prompt: input.prompt,
                hidden_model_guidance: input.hidden_model_guidance,
                provider_visible_agent_messages: input.provider_visible_agent_messages,
                attachments: input.attachments,
                append_canonical_user_message: input.append_canonical_user_message,
                append_visible_user_prompt_record: input.append_visible_user_prompt_record,
                source: input.source,
                execution_mode: input.execution_mode,
                reasoning_effort: input.reasoning_effort,
                owning_project_id,
                prior_session_messages: prior_dialogue_messages.as_deref(),
            },
        )?
    };
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get(&input.session_id)
    {
        let merged_messages = merge_session_messages_with_conversation_state_records(
            workspace_snapshot
                .session_messages
                .get(&input.session_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            &runtime_state.conversation_state,
        );
        workspace_snapshot
            .session_messages
            .insert(input.session_id, merged_messages);
    }
    let initial_phase = if input.source.runs_user_prompt_precheck() {
        SessionRunPhase::MemoryRecall
    } else {
        SessionRunPhase::Preparing
    };
    workspace_snapshot
        .session_run_phases
        .insert(input.session_id, initial_phase);
    Ok(start)
}

/// Begins one prompt turn using shared runtime-state scoped semantics.
/// 使用共享的运行态作用域语义开始一个提示词回合。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state being mutated.
/// `runtime_state` 是将被修改的会话运行态。
/// `conversation_hook_runtime` prepares the shared precheck request.
/// `conversation_hook_runtime` 用于准备共享 precheck 请求。
/// `input` carries shell-independent prompt-turn inputs.
/// `input` 携带与外壳无关的提示词回合输入。
///
/// # Returns
/// 返回值
/// A started prompt turn that must continue after precheck completes.
/// 返回需要在 precheck 完成后继续的已启动提示词回合。
pub fn begin_shared_prompt_turn_with_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    conversation_hook_runtime: &mut ConversationHookRuntime,
    input: SharedPromptTurnRuntimeBeginInput<'_>,
) -> Result<SharedPromptTurnStart, String> {
    // draft_runtime_state isolates hydration, paired message/record append, and runtime flags until prompt start succeeds.
    // draft_runtime_state 会隔离水合、成对消息/记录追加与运行态标记，直到提示词启动成功。
    let mut draft_runtime_state = runtime_state.clone();
    // start is produced only after every canonical mutation on the detached draft succeeds.
    // start 仅在分离草稿上的每项规范修改成功后才会产出。
    let start = begin_shared_prompt_turn_with_runtime_state_draft(
        &mut draft_runtime_state,
        conversation_hook_runtime,
        input,
    )?;
    *runtime_state = draft_runtime_state;
    Ok(start)
}

/// Begins one prompt turn against a detached runtime-state draft.
/// 针对一份分离运行态草稿开始提示词回合。
///
/// # Returns
/// 返回值
/// Returns the started prompt turn, or the first mutation or hook-preparation failure.
/// 返回已启动的提示词回合，或首个修改/Hook 准备失败。
fn begin_shared_prompt_turn_with_runtime_state_draft(
    runtime_state: &mut SessionRuntimeStateEntry,
    conversation_hook_runtime: &mut ConversationHookRuntime,
    input: SharedPromptTurnRuntimeBeginInput<'_>,
) -> Result<SharedPromptTurnStart, String> {
    let now = current_unix_millis_text();
    let (injection_ops, sanitized_prompt) =
        collect_builtin_manual_injection_ops_and_sanitized_prompt(input.prompt);
    let start_attachments = input.attachments.clone();
    // hydrated_conversation_state remains detached until the candidate prompt identity is proven valid.
    // hydrated_conversation_state 会保持分离，直到候选提示词身份被证明有效。
    let hydrated_conversation_state = if runtime_state.conversation_state.messages.is_empty()
        && runtime_state.conversation_state.records.is_empty()
        && let Some(messages) = input.prior_session_messages
    {
        Some(crate::conversation_protocol::conversation_state_from_session_messages(messages)?)
    } else {
        None
    };
    let append_canonical_user_message =
        input.append_canonical_user_message && input.source.persists_canonical_user_message();
    let append_visible_user_prompt_record = input.append_visible_user_prompt_record
        && input.source.appends_visible_user_prompt_record();
    // canonical_user_message_id must remain unique even when consecutive starts share one millisecond timestamp.
    // canonical_user_message_id 即使连续启动共享同一个毫秒时间戳也必须保持唯一。
    let canonical_user_message_id =
        append_canonical_user_message.then(|| new_canonical_user_message_id(input.session_id));
    let mut visible_user_prompt_record_id = None;
    if append_canonical_user_message
        && let Some(hydrated_conversation_state) = hydrated_conversation_state.as_ref()
    {
        // validation_state proves the detached hydrated window can accept the generated prompt identity before assignment.
        // validation_state 在赋值前证明分离的水合窗口能够接收生成的提示词身份。
        let mut validation_state = hydrated_conversation_state.clone();
        append_user_text_message(
            &mut validation_state,
            canonical_user_message_id
                .as_ref()
                .expect("canonical id exists when canonical append is enabled")
                .clone(),
            sanitized_prompt.clone(),
            input.attachments.clone(),
        )
        .map_err(|error| format!("Invalid canonical user message identity: {error}"))?;
    }
    if let Some(hydrated_conversation_state) = hydrated_conversation_state {
        runtime_state.conversation_state = hydrated_conversation_state;
    }
    if append_canonical_user_message && append_visible_user_prompt_record {
        visible_user_prompt_record_id = Some(append_shared_user_prompt_record_with_attachments(
            runtime_state,
            input.session_id,
            canonical_user_message_id
                .as_ref()
                .expect("canonical id exists when canonical append is enabled")
                .clone(),
            &now,
            &sanitized_prompt,
            input.attachments.clone(),
            input.source,
        )?);
    } else if append_canonical_user_message {
        append_user_text_message(
            &mut runtime_state.conversation_state,
            canonical_user_message_id
                .as_ref()
                .expect("canonical id exists when canonical append is enabled")
                .clone(),
            sanitized_prompt.clone(),
            input.attachments.clone(),
        )
        .map_err(|error| format!("Invalid canonical user message identity: {error}"))?;
        runtime_state.updated_at = now.clone();
    }
    runtime_state.ui_state.unread = false;
    let system_prompt_hook_phase = vmm_profile_placement_phase(
        runtime_state.conversation_state.messages.len(),
        runtime_state.last_compaction_id.as_deref(),
        runtime_state
            .vmm_profile_sync
            .covered_compaction_id
            .as_deref(),
    );
    let hook_conversation_state = runtime_state.conversation_state.clone();
    let hook_requests = conversation_hook_runtime.prepare_user_prompt_hook_requests(
        input.session_id,
        &input.prompt_entry_id,
        input.owning_project_id.clone(),
        input.workspace_path.to_string(),
        input.source,
        system_prompt_hook_phase,
        &sanitized_prompt,
        &hook_conversation_state,
    );
    // preparation_turn_id correlates sibling requests without making either line the owner.
    // preparation_turn_id 关联同级请求，而不让任一线路成为另一线路的所有者。
    let preparation_turn_id = hook_requests.turn_id;
    runtime_state.active_precheck_turn_id = Some(preparation_turn_id.clone());
    if !injection_ops.is_empty() {
        runtime_state
            .context_injection_state
            .apply_inserts_for_turn(injection_ops, &now, Some(&preparation_turn_id));
        runtime_state.updated_at = now.clone();
    }
    let boundary_events = prompt_turn_boundary_events(
        &input,
        &preparation_turn_id,
        &sanitized_prompt,
        &start_attachments,
        &now,
    );
    Ok(SharedPromptTurnStart {
        session_id: input.session_id,
        preparation_turn_id,
        session_title: input.session_title.to_string(),
        workspace_path: input.workspace_path.to_string(),
        source: input.source,
        precheck_request: hook_requests.precheck,
        profile_request: hook_requests.profile,
        prompt: sanitized_prompt,
        hidden_model_guidance: input
            .hidden_model_guidance
            .map(str::trim)
            .filter(|guidance| !guidance.is_empty())
            .map(str::to_string),
        provider_visible_agent_messages: input.provider_visible_agent_messages,
        attachments: start_attachments,
        execution_mode: input.execution_mode,
        reasoning_effort: input.reasoning_effort,
        boundary_events,
        visible_user_prompt_record_id,
        canonical_user_message_id,
    })
}

/// Generates one collision-resistant canonical user-message identifier for exact anchor ownership.
/// 为精确锚点归属生成一个抗碰撞的规范用户消息标识。
///
/// # Parameters
/// 参数
/// `session_id` is the exact session owner embedded for diagnostics.
/// `session_id` 是嵌入标识、用于诊断的精确会话归属。
///
/// # Returns
/// 返回值
/// A session-scoped UUID message id suitable for unique transcript and continuation boundaries.
/// 返回适用于唯一转录与 continuation 边界的会话作用域 UUID 消息标识。
fn new_canonical_user_message_id(session_id: SessionId) -> String {
    format!(
        "session-{session_id}-protocol-user-{}",
        uuid::Uuid::new_v4()
    )
}

/// Builds session-boundary prompt-turn boundary events from the shared reducer input.
/// 从共享归约器输入构建 会话边界的提示词轮次边界事件。
fn prompt_turn_boundary_events(
    input: &SharedPromptTurnRuntimeBeginInput<'_>,
    turn_id: &str,
    prompt: &str,
    attachments: &[ConversationAttachment],
    now: &str,
) -> Vec<SessionBoundaryEvent> {
    let thread_id = SessionNodeId::from(input.session_id);
    let timestamp_ms = unix_millis_text_to_u64(now);
    let append_canonical_user_message =
        input.append_canonical_user_message && input.source.persists_canonical_user_message();
    let append_visible_user_prompt_record = input.append_visible_user_prompt_record
        && input.source.appends_visible_user_prompt_record();
    let user_input_visibility = if append_visible_user_prompt_record {
        SessionEventVisibility::Visible
    } else {
        SessionEventVisibility::Hidden
    };
    let turn_started = SessionBoundaryEvent::new(
        format!("{}:{turn_id}:turn-started", input.session_id),
        thread_id,
        0,
        SessionBoundaryEventKind::TurnStarted,
        SessionEventScope::Turn,
        SessionEventVisibility::Hidden,
        timestamp_ms,
    )
    .with_turn_id(turn_id.to_string())
    .with_payload(serde_json::json!({
        "prompt_entry_id": &input.prompt_entry_id,
        "source": input.source,
        "starts_user_turn": input.source.starts_user_parent_turn(),
        "turn_process": input.source.folds_into_active_turn_process(),
        "editable": input.source.allows_user_prompt_edit(),
        "execution_mode": &input.execution_mode,
        "workspace_path": input.workspace_path,
        "session_title": input.session_title,
    }));
    let user_input = SessionBoundaryEvent::new(
        format!("{}:{turn_id}:user-input", input.session_id),
        thread_id,
        1,
        SessionBoundaryEventKind::UserInput,
        SessionEventScope::Turn,
        user_input_visibility,
        timestamp_ms,
    )
    .with_turn_id(turn_id.to_string())
    .with_payload(serde_json::json!({
        "prompt": prompt,
        "attachments": attachments,
        "source": input.source,
        "canonical": append_canonical_user_message,
        "starts_user_turn": input.source.starts_user_parent_turn(),
        "turn_process": input.source.folds_into_active_turn_process(),
        "collapsed_by_default": input.source.folds_into_active_turn_process(),
        "editable": input.source.allows_user_prompt_edit(),
    }));
    vec![turn_started, user_input]
}

/// Parses Unix-milliseconds text into the session boundary event timestamp domain.
/// 将 Unix 毫秒文本解析到 会话边界事件时间戳域。
fn unix_millis_text_to_u64(value: &str) -> u64 {
    value.trim().parse::<u64>().unwrap_or_default()
}

/// Appends host-only guidance to the provider prompt while keeping transcript records unchanged.
/// 在保持转录记录不变的同时，将仅宿主可见引导追加到 Provider 提示词。
fn append_hidden_model_guidance_to_prompt(prompt: String, guidance: Option<&str>) -> String {
    let Some(guidance) = guidance.map(str::trim).filter(|value| !value.is_empty()) else {
        return prompt;
    };
    format!("{prompt}\n\n<hidden_host_guidance>\n{guidance}\n</hidden_host_guidance>")
}

/// Continues one prompt turn after independent profile and precheck preparation.
/// 在独立画像与 precheck 准备完成后继续一个提示词回合。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the shell-owned workspace snapshot being mutated.
/// `workspace_snapshot` 是外壳持有并将被修改的工作区快照。
/// `vmm_session_memory` is the isolated VMM candidate owned by the calling Core controller command.
/// `vmm_session_memory` 是调用方 Core 控制器命令拥有的隔离 VMM 候选状态。
/// `input` carries independent profile, precheck, and execution inputs.
/// `input` 携带相互独立的画像、precheck 与执行输入。
///
/// # Returns
/// 返回值
/// A continuation containing the real provider execution request.
/// 返回包含真实 Provider 执行请求的 continuation。
pub fn continue_shared_prompt_turn_after_preparation(
    workspace_snapshot: &mut WorkspaceSnapshot,
    vmm_session_memory: &mut vulcan_vmm::VmmSessionMemoryState,
    input: SharedPromptTurnContinueInput,
) -> Result<SharedPromptTurnContinuation, String> {
    let session_id = input.start.session_id;
    let continuation = {
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("会话 `{session_id}` 缺少运行态，无法继续提示词回合。"))?;
        continue_shared_prompt_turn_after_preparation_with_runtime_state(
            runtime_state,
            vmm_session_memory,
            input,
        )?
    };
    let merged_messages = merge_session_messages_with_conversation_state_records(
        workspace_snapshot
            .session_messages
            .get(&session_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &continuation.request.conversation_state,
    );
    workspace_snapshot
        .session_messages
        .insert(session_id, merged_messages);
    Ok(continuation)
}

/// Continues one prompt turn after independent preparation using runtime-state scoped semantics.
/// 使用运行态作用域语义在独立准备完成后继续一个提示词回合。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state being mutated.
/// `runtime_state` 是将被修改的会话运行态。
/// `vmm_session_memory` is the isolated VMM candidate mutated only within one controller command.
/// `vmm_session_memory` 是仅在一个控制器命令内修改的隔离 VMM 候选状态。
/// `input` carries precheck and execution inputs.
/// `input` 携带 precheck 与执行输入。
///
/// # Returns
/// 返回值
/// A continuation containing the real provider execution request.
/// 返回包含真实 Provider 执行请求的 continuation。
pub fn continue_shared_prompt_turn_after_preparation_with_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    vmm_session_memory: &mut vulcan_vmm::VmmSessionMemoryState,
    input: SharedPromptTurnContinueInput,
) -> Result<SharedPromptTurnContinuation, String> {
    let session_id = input.start.session_id;
    if input.precheck_outcome.session_id != session_id {
        return Err(format!(
            "precheck 结果会话不匹配：期望 {session_id}，实际 {}",
            input.precheck_outcome.session_id
        ));
    }
    let expected_turn_id = input.start.preparation_turn_id.clone();
    if input.precheck_outcome.turn_id != expected_turn_id {
        return Err(format!(
            "precheck 结果回合不匹配：期望 {expected_turn_id}，实际 {}",
            input.precheck_outcome.turn_id
        ));
    }
    if input.profile_outcome.session_id != session_id {
        return Err(format!(
            "画像准备结果会话不匹配：期望 {session_id}，实际 {}",
            input.profile_outcome.session_id
        ));
    }
    if input.profile_outcome.turn_id != expected_turn_id {
        return Err(format!(
            "画像准备结果回合不匹配：期望 {expected_turn_id}，实际 {}",
            input.profile_outcome.turn_id
        ));
    }
    if runtime_state.active_precheck_turn_id.as_deref() != Some(expected_turn_id.as_str()) {
        return Err(format!(
            "提示词准备结果已过期：当前等待 {:?}，收到 {expected_turn_id}",
            runtime_state.active_precheck_turn_id
        ));
    }
    runtime_state.active_precheck_turn_id = None;
    let mut request_prompt = input.start.prompt;
    let request_attachments = input.start.attachments.clone();
    let mut system_prompt_hook_blocks = Vec::new();
    let mut precheck_failure_detail = None;
    let mut profile_failure_detail = None;
    // feature_policy governs local VMM state independently from the remote PreCheck outcome.
    // feature_policy 独立于远端 PreCheck 结果控制本地 VMM 状态。
    let feature_policy = vulcan_vmm::VmmRuntimeFeaturePolicy::from_env();
    if feature_policy.session_state_enabled {
        if let Some(target_message_id) = input.start.canonical_user_message_id.as_deref() {
            // now is shared by the atomic local pre-provider VMM boundary transition.
            // now 由 Provider 调用前的原子本地 VMM 边界转换共享。
            let now = current_unix_millis_text();
            // mode_report records an explicit rollout epoch transition when policy changed.
            // mode_report 记录策略变化时的显式灰度纪元转换。
            let mode_report = vmm_session_memory
                .synchronize_stable_anchor_rendering(feature_policy.stable_anchors_enabled, &now)?;
            // pending_report binds local expiry and compaction state even when the remote hook fails.
            // pending_report 即使在远端 Hook 失败时也会绑定本地到期与压缩状态。
            let pending_report =
                vmm_session_memory.bind_pending_state_to_user(target_message_id, &now)?;
            // active_turn_report fences terminal TTL consumption to this exact logical turn.
            // active_turn_report 将终态 TTL 消费限定到当前精确逻辑回合。
            let active_turn_report =
                vmm_session_memory.register_active_turn(&expected_turn_id, &now)?;
            if mode_report.changed {
                runtime_state.conversation_state.continuation = Default::default();
            }
            if mode_report.changed || pending_report.changed || active_turn_report.changed {
                runtime_state.updated_at = now;
            }
        } else if input.start.source.runs_completed_turn_hooks()
            && vmm_session_memory.requires_persistence()
        {
            return Err(String::from(
                "VMM active session state requires an exact canonical user-message id before a terminal user turn can start.",
            ));
        }
    }
    // Profile preparation owns only the persistent baseline and append-only profile deltas.
    // 画像准备仅持有持久化基线与仅追加画像增量。
    match input.profile_outcome.report {
        Ok(report) => {
            if let Some(effect) = vmm_profile_sync_effect_from_report(&report) {
                runtime_state.vmm_profile_sync = effect.state;
                runtime_state.updated_at = current_unix_millis_text();
            }
            if !report.injections.is_empty() {
                let now = current_unix_millis_text();
                runtime_state
                    .context_injection_state
                    .apply_inserts_for_turn(report.injections, &now, Some(&expected_turn_id));
                runtime_state.updated_at = now;
            }
            system_prompt_hook_blocks = report.system_prompt_blocks;
        }
        Err(error) => {
            let detail = format!(
                "会话 `{}` 的画像准备未能完成，已继续复用持久化画像基线：{error}",
                input.start.session_title
            );
            append_runtime_event_projection_to_state(
                runtime_state,
                session_id,
                &current_unix_millis_text(),
                &RuntimeEvent::new(
                    RuntimeEventKind::Failed,
                    "画像准备 Hook 执行失败",
                    detail.clone(),
                ),
            )?;
            profile_failure_detail = Some(detail);
        }
    }
    // User-prompt precheck owns recall, prompt transformation, and its own non-profile Hook outputs.
    // 用户提示 precheck 仅持有召回、提示词转换及其自身的非画像 Hook 输出。
    match input.precheck_outcome.report {
        Ok(report) => {
            if feature_policy.session_state_enabled
                && let Some(effect) = vmm_recall_batch_effect_from_report(&report)?
            {
                let target_message_id = input
                    .start
                    .canonical_user_message_id
                    .as_deref()
                    .ok_or_else(|| {
                        String::from(
                            "VMM recall effect requires the exact canonical user-message id created before precheck.",
                        )
                    })?;
                let now = current_unix_millis_text();
                vmm_session_memory.apply_recall_batch(
                    &session_id.to_string(),
                    target_message_id,
                    &effect.batch,
                    &now,
                )?;
                vmm_session_memory.register_active_turn(&expected_turn_id, &now)?;
                runtime_state.updated_at = now;
            }
            if let Some(rewritten_prompt) = report
                .payload
                .value
                .get("user_content")
                .and_then(Value::as_str)
                .filter(|content| *content != request_prompt)
            {
                request_prompt = rewritten_prompt.to_string();
                patch_latest_prechecked_user_prompt_record(
                    runtime_state,
                    &request_prompt,
                    &request_attachments,
                    input.patch_visible_user_prompt_record,
                );
            }
            if !report.injections.is_empty() {
                let now = current_unix_millis_text();
                runtime_state
                    .context_injection_state
                    .apply_inserts_for_turn(report.injections, &now, Some(&expected_turn_id));
                runtime_state.updated_at = now;
            }
            system_prompt_hook_blocks.extend(report.system_prompt_blocks);
        }
        Err(error) => {
            let detail = format!(
                "会话 `{}` 的 precheck 未能完成，已继续真实请求：{error}",
                input.start.session_title
            );
            append_runtime_event_projection_to_state(
                runtime_state,
                session_id,
                &current_unix_millis_text(),
                &RuntimeEvent::new(
                    RuntimeEventKind::Failed,
                    "Precheck hook 执行失败",
                    detail.clone(),
                ),
            )?;
            precheck_failure_detail = Some(detail);
        }
    }
    // Persisted profile projection is authoritative after both lines, so precheck cannot add or replace it.
    // 两条线路结束后持久化画像投影仍具权威性，因此 precheck 无法添加或替换它。
    stabilize_vmm_profile_system_prompt_blocks(
        &mut system_prompt_hook_blocks,
        &runtime_state.vmm_profile_sync,
    );
    request_prompt = append_hidden_model_guidance_to_prompt(
        request_prompt,
        input.start.hidden_model_guidance.as_deref(),
    );
    let prompt_purpose = ExecutionPromptPurpose::UserTurn;
    let runtime_policy = vulcan_session_core::ExecutionRuntimePolicy::main_session();
    let context_injection = core_provider_context_injection_from_state_for_turn(
        &runtime_state.context_injection_state,
        vmm_session_memory,
        Some(&expected_turn_id),
    );
    let request = ExecutionPreparationRequest {
        session_id,
        managed_codebase_memory: None,
        execution_scope: vulcan_session_core::ExecutionScope::MainSession {
            session_id: vulcan_session_core::SessionNodeId::from(session_id),
        },
        runtime_policy,
        conversation_hook_turn_id: Some(expected_turn_id),
        execution_agent_id: None,
        agent_prompt_snapshot: None,
        provider_id_override: None,
        model_id_override: None,
        connection_id_override: None,
        provider_tool_selections_override: runtime_state
            .provider_tool_selections_overridden
            .then(|| runtime_state.provider_tool_selections.clone()),
        subagent_prompt_context: None,
        task_session_store_root: None,
        session_title: input.start.session_title,
        workspace_path: input.start.workspace_path,
        prompt: request_prompt,
        provider_visible_agent_messages: input.start.provider_visible_agent_messages,
        conversation_state: runtime_state.conversation_state.clone(),
        context_compaction_policy: None,
        context_budget_override: None,
        execution_mode: input.start.execution_mode,
        temperature: runtime_state.active_temperature,
        reasoning_effort: input
            .start
            .reasoning_effort
            .or(input.runtime_reasoning_effort)
            .or(input.provider_reasoning_effort)
            .unwrap_or(ReasoningEffortLevel::Medium),
        tool_exposure_override: Default::default(),
        tool_request_rejection_reason: None,
        tool_request_rejection_all_tools: false,
        tool_request_rejection_tool_ids: Vec::new(),
        tool_request_hidden_tool_ids: Vec::new(),
        prompt_purpose,
        prompt_skill_policy_override: Default::default(),
        project_prompt_skills: input.project_configuration.prompt_skills,
        session_prompt_skill_overrides: runtime_state.prompt_skill_overrides.clone(),
        context_injection,
        system_prompt_hook_blocks,
        project_vmm: input.project_configuration.vmm,
        session_vmm: runtime_state.vmm.clone(),
        boundary_events: input.start.boundary_events.clone(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    };
    Ok(SharedPromptTurnContinuation {
        request,
        precheck_failure_detail,
        profile_failure_detail,
    })
}

/// Builds the shared workbench plan for precheck dispatch visibility and skipping.
/// 构建工作台与服务共用的 precheck 调度显示和跳过计划。
///
/// # Parameters
/// 参数
/// `effective_vmm_config` is the resolved global/project/session VMM configuration.
/// `effective_vmm_config` 是已解析的全局、项目与会话 VMM 配置。
/// `precheck_hook_snapshots` lists every registered precheck hook handler.
/// `precheck_hook_snapshots` 列出所有已注册的 precheck hook 处理器。
///
/// # Returns
/// 返回值
/// A shared precheck plan matching service behavior.
/// 返回与服务行为一致的共享 precheck 计划。
pub fn shared_prompt_precheck_plan(
    effective_vmm_config: &VmmEffectiveConfig,
    precheck_hook_snapshots: &[HookRegistrationSnapshot],
) -> SharedPromptPrecheckPlan {
    let vmm_precheck_ready =
        effective_vmm_config.diagnose_precheck_flow(true) == VmmConfigDiagnostic::Ready;
    let should_skip_precheck = !vmm_precheck_ready
        && !precheck_hook_snapshots.is_empty()
        && precheck_hook_snapshots
            .iter()
            .all(|snapshot| snapshot.registration.id == VMM_PRECHECK_HOOK_ID);
    let progress_labels = precheck_hook_snapshots
        .iter()
        .filter(|snapshot| vmm_precheck_ready || snapshot.registration.id != VMM_PRECHECK_HOOK_ID)
        .filter_map(|snapshot| snapshot.registration.progress_label.clone())
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .collect();
    SharedPromptPrecheckPlan {
        has_precheck_hooks: !precheck_hook_snapshots.is_empty(),
        should_skip_precheck,
        progress_labels,
    }
}

/// Returns the current Unix timestamp in milliseconds as text.
/// 以文本形式返回当前 Unix 毫秒时间戳。
///
/// # Returns
/// 返回值
/// Current Unix timestamp in milliseconds.
/// 当前 Unix 毫秒时间戳。
fn current_unix_millis_text() -> String {
    OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .checked_div(1_000_000)
        .unwrap_or_default()
        .to_string()
}
