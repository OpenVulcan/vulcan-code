//! Shared prompt-turn reducers used by workbench and service shells.
//! 工作台与服务外壳共用的提示词轮次归约器。

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use serde_json::Value;
use time::OffsetDateTime;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_hooks::HookRegistrationSnapshot;
use vulcan_provider_config::config::{
    AiToolKind, PersistedAiToolConfiguration, ProjectVulcanCodeConfigurationFile,
    ReasoningEffortLevel,
};
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
};
use vulcan_session_core::SessionNodeId;
use vulcan_vmm::{VmmConfigDiagnostic, VmmEffectiveConfig};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::{
    ExecutionPreparationRequest, ExecutionPromptPurpose,
    agent_runtime::{
        context_injection_runtime::{
            collect_builtin_manual_injection_ops_and_sanitized_prompt,
            core_provider_context_injection_from_state_for_turn,
        },
        conversation_hooks::{
            ConversationHookRuntime, ConversationPrecheckOutcome, ConversationPrecheckRequest,
            vmm_profile_sync_effect_from_report,
        },
        events::{RuntimeEvent, RuntimeEventKind},
        vmm_hooks::{VMM_PRECHECK_HOOK_ID, vmm_profile_placement_phase},
    },
    conversation_protocol::{
        SessionConversationPayload, SessionConversationRecord, append_record,
        append_user_text_message, merge_session_messages_with_conversation_state_records,
    },
    session_event_runtime::{
        append_runtime_event_projection_to_state, build_session_record_message_id,
    },
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

/// Prepared prompt turn that must continue after precheck completes.
/// 需要在 precheck 完成后继续的提示词回合。
#[derive(Clone)]
pub struct SharedPromptTurnStart {
    /// Session that owns this prompt turn.
    /// 拥有该提示词回合的会话。
    pub session_id: SessionId,
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
    /// Prompt text after builtin manual-injection syntax is removed.
    /// 移除内建手动注入语法后的提示词文本。
    pub prompt: String,
    /// Optional model-only guidance appended after visible-user prompt handling.
    /// 可选的仅模型可见引导，会在可见用户提示处理后追加。
    pub hidden_model_guidance: Option<String>,
    /// Structured attachments bound to the prompt.
    /// 绑定在提示词上的结构化附件。
    pub attachments: Vec<ConversationAttachment>,
    /// Execution mode requested by the shell command.
    /// 外壳命令请求的执行模式。
    pub execution_mode: String,
    /// Reasoning effort requested by the shell command.
    /// 外壳命令请求的推理强度。
    pub reasoning_effort: Option<ReasoningEffortLevel>,
    /// AI capability kinds disabled for this prompt turn.
    /// 本次提示词回合禁用的 AI 能力类型。
    pub disabled_ai_capability_kinds: BTreeSet<AiToolKind>,
    /// AI capability target selectors disabled for this prompt turn.
    /// 本次提示词回合禁用的 AI 能力目标选择器。
    pub disabled_ai_capability_target_selectors: BTreeSet<String>,
    /// AI capability target order overrides requested by this prompt turn.
    /// 本次提示词回合请求的 AI 能力目标顺序覆盖。
    pub ai_capability_target_order_overrides: BTreeMap<AiToolKind, Vec<String>>,
    /// session-boundary boundary events produced when this prompt turn is accepted.
    /// 当前提示词回合被接受时生成的 会话边界边界事件。
    pub boundary_events: Vec<SessionBoundaryEvent>,
}

impl fmt::Debug for SharedPromptTurnStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedPromptTurnStart")
            .field("session_id", &self.session_id)
            .field("session_title", &self.session_title)
            .field("workspace_path", &self.workspace_path)
            .field("source", &self.source)
            .field("prompt", &self.prompt)
            .field("attachments", &self.attachments)
            .field("execution_mode", &self.execution_mode)
            .field("reasoning_effort", &self.reasoning_effort)
            .field(
                "disabled_ai_capability_kinds",
                &self.disabled_ai_capability_kinds,
            )
            .field(
                "disabled_ai_capability_target_selectors",
                &self.disabled_ai_capability_target_selectors,
            )
            .field(
                "ai_capability_target_order_overrides",
                &self.ai_capability_target_order_overrides,
            )
            .field("boundary_events", &self.boundary_events.len())
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
    /// AI capability kinds disabled for this prompt turn.
    /// 本次提示词回合禁用的 AI 能力类型。
    pub disabled_ai_capability_kinds: BTreeSet<AiToolKind>,
    /// AI capability target selectors disabled for this prompt turn.
    /// 本次提示词回合禁用的 AI 能力目标选择器。
    pub disabled_ai_capability_target_selectors: BTreeSet<String>,
    /// AI capability target order overrides requested by this prompt turn.
    /// 本次提示词回合请求的 AI 能力目标顺序覆盖。
    pub ai_capability_target_order_overrides: BTreeMap<AiToolKind, Vec<String>>,
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
    /// AI capability kinds disabled for this prompt turn.
    /// 本次提示词回合禁用的 AI 能力类型。
    pub disabled_ai_capability_kinds: BTreeSet<AiToolKind>,
    /// AI capability target selectors disabled for this prompt turn.
    /// 本次提示词回合禁用的 AI 能力目标选择器。
    pub disabled_ai_capability_target_selectors: BTreeSet<String>,
    /// AI capability target order overrides requested by this prompt turn.
    /// 本次提示词回合请求的 AI 能力目标顺序覆盖。
    pub ai_capability_target_order_overrides: BTreeMap<AiToolKind, Vec<String>>,
    /// Owning project id used by hook routing.
    /// hook 路由使用的所属项目标识。
    pub owning_project_id: Option<String>,
    /// Persisted session messages used when runtime state has not been hydrated yet.
    /// 运行态尚未水合时用于恢复会话状态的持久化消息。
    pub prior_session_messages: Option<&'a [SessionMessageEntry]>,
}

/// Input required to continue one shared prompt turn after precheck.
/// precheck 后继续一个共享提示词回合所需的输入。
pub struct SharedPromptTurnContinueInput {
    /// Started prompt turn waiting for precheck completion.
    /// 正在等待 precheck 完成的已启动提示词回合。
    pub start: SharedPromptTurnStart,
    /// Completed precheck outcome.
    /// 已完成的 precheck 结果。
    pub precheck_outcome: ConversationPrecheckOutcome,
    /// Effective AI tool configuration for the actual provider request.
    /// 真实 Provider 请求使用的有效 AI 工具配置。
    pub ai_tool_configuration: PersistedAiToolConfiguration,
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
                attachments: input.attachments,
                append_canonical_user_message: input.append_canonical_user_message,
                append_visible_user_prompt_record: input.append_visible_user_prompt_record,
                source: input.source,
                execution_mode: input.execution_mode,
                reasoning_effort: input.reasoning_effort,
                disabled_ai_capability_kinds: input.disabled_ai_capability_kinds,
                disabled_ai_capability_target_selectors: input
                    .disabled_ai_capability_target_selectors,
                ai_capability_target_order_overrides: input.ai_capability_target_order_overrides,
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
    let now = current_unix_millis_text();
    let (injection_ops, sanitized_prompt) =
        collect_builtin_manual_injection_ops_and_sanitized_prompt(input.prompt);
    let start_attachments = input.attachments.clone();
    runtime_state.ui_state.unread = false;
    if runtime_state.conversation_state.messages.is_empty()
        && runtime_state.conversation_state.records.is_empty()
    {
        if let Some(messages) = input.prior_session_messages {
            runtime_state.conversation_state =
                crate::conversation_protocol::conversation_state_from_session_messages(messages);
        }
    }
    let append_canonical_user_message =
        input.append_canonical_user_message && input.source.persists_canonical_user_message();
    let append_visible_user_prompt_record =
        input.append_visible_user_prompt_record && input.source.starts_user_parent_turn();
    if append_canonical_user_message && append_visible_user_prompt_record {
        append_shared_user_prompt_record_with_attachments(
            runtime_state,
            input.session_id,
            &now,
            &sanitized_prompt,
            input.attachments.clone(),
        );
    } else if append_canonical_user_message {
        append_user_text_message(
            &mut runtime_state.conversation_state,
            format!("session-{}-protocol-user-{now}", input.session_id),
            sanitized_prompt.clone(),
            input.attachments.clone(),
        );
        runtime_state.updated_at = now.clone();
    }
    let system_prompt_hook_phase = vmm_profile_placement_phase(
        runtime_state.conversation_state.messages.len(),
        runtime_state.last_compaction_id.as_deref(),
        runtime_state
            .vmm_profile_sync
            .covered_compaction_id
            .as_deref(),
    );
    let hook_conversation_state = runtime_state.conversation_state.clone();
    let precheck_request = conversation_hook_runtime.prepare_user_prompt_precheck(
        input.session_id,
        &input.prompt_entry_id,
        input.owning_project_id.clone(),
        input.workspace_path.to_string(),
        input.source,
        system_prompt_hook_phase,
        &sanitized_prompt,
        &hook_conversation_state,
    );
    let precheck_turn_id = precheck_request.turn_id().to_string();
    runtime_state.active_precheck_turn_id = Some(precheck_turn_id.clone());
    if !injection_ops.is_empty() {
        runtime_state
            .context_injection_state
            .apply_inserts_for_turn(injection_ops, &now, Some(&precheck_turn_id));
        runtime_state.updated_at = now.clone();
    }
    let boundary_events = prompt_turn_boundary_events(
        &input,
        &precheck_turn_id,
        &sanitized_prompt,
        &start_attachments,
        &now,
    );
    Ok(SharedPromptTurnStart {
        session_id: input.session_id,
        session_title: input.session_title.to_string(),
        workspace_path: input.workspace_path.to_string(),
        source: input.source,
        precheck_request,
        prompt: sanitized_prompt,
        hidden_model_guidance: input
            .hidden_model_guidance
            .map(str::trim)
            .filter(|guidance| !guidance.is_empty())
            .map(str::to_string),
        attachments: start_attachments,
        execution_mode: input.execution_mode,
        reasoning_effort: input.reasoning_effort,
        disabled_ai_capability_kinds: input.disabled_ai_capability_kinds,
        disabled_ai_capability_target_selectors: input.disabled_ai_capability_target_selectors,
        ai_capability_target_order_overrides: input.ai_capability_target_order_overrides,
        boundary_events,
    })
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
    let append_visible_user_prompt_record =
        input.append_visible_user_prompt_record && input.source.starts_user_parent_turn();
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

/// Continues one prompt turn after precheck using shared workbench semantics.
/// 使用工作台与服务共用语义在 precheck 后继续一个提示词回合。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the shell-owned workspace snapshot being mutated.
/// `workspace_snapshot` 是外壳持有并将被修改的工作区快照。
/// `input` carries precheck and execution inputs.
/// `input` 携带 precheck 与执行输入。
///
/// # Returns
/// 返回值
/// A continuation containing the real provider execution request.
/// 返回包含真实 Provider 执行请求的 continuation。
pub fn continue_shared_prompt_turn_after_precheck(
    workspace_snapshot: &mut WorkspaceSnapshot,
    input: SharedPromptTurnContinueInput,
) -> Result<SharedPromptTurnContinuation, String> {
    let session_id = input.start.session_id;
    let continuation = {
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("会话 `{session_id}` 缺少运行态，无法继续提示词回合。"))?;
        continue_shared_prompt_turn_after_precheck_with_runtime_state(runtime_state, input)?
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

/// Continues one prompt turn after precheck using shared runtime-state scoped semantics.
/// 使用共享的运行态作用域语义在 precheck 后继续一个提示词回合。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state being mutated.
/// `runtime_state` 是将被修改的会话运行态。
/// `input` carries precheck and execution inputs.
/// `input` 携带 precheck 与执行输入。
///
/// # Returns
/// 返回值
/// A continuation containing the real provider execution request.
/// 返回包含真实 Provider 执行请求的 continuation。
pub fn continue_shared_prompt_turn_after_precheck_with_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    input: SharedPromptTurnContinueInput,
) -> Result<SharedPromptTurnContinuation, String> {
    let session_id = input.start.session_id;
    if input.precheck_outcome.session_id != session_id {
        return Err(format!(
            "precheck 结果会话不匹配：期望 {session_id}，实际 {}",
            input.precheck_outcome.session_id
        ));
    }
    let expected_turn_id = input.start.precheck_request.turn_id().to_string();
    if input.precheck_outcome.turn_id != expected_turn_id {
        return Err(format!(
            "precheck 结果回合不匹配：期望 {expected_turn_id}，实际 {}",
            input.precheck_outcome.turn_id
        ));
    }
    if runtime_state.active_precheck_turn_id.as_deref() != Some(expected_turn_id.as_str()) {
        return Err(format!(
            "precheck 结果已过期：当前等待 {:?}，收到 {expected_turn_id}",
            runtime_state.active_precheck_turn_id
        ));
    }
    runtime_state.active_precheck_turn_id = None;
    let mut request_prompt = input.start.prompt;
    let request_attachments = input.start.attachments.clone();
    let mut system_prompt_hook_blocks = Vec::new();
    let mut precheck_failure_detail = None;
    match input.precheck_outcome.report {
        Ok(report) => {
            if let Some(effect) = vmm_profile_sync_effect_from_report(&report) {
                runtime_state.vmm_profile_sync = effect.state;
                runtime_state.updated_at = current_unix_millis_text();
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
                    .apply_inserts_for_turn(
                        report.injections,
                        &now,
                        Some(&input.precheck_outcome.turn_id),
                    );
                runtime_state.updated_at = now;
            }
            system_prompt_hook_blocks = report.system_prompt_blocks;
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
            );
            precheck_failure_detail = Some(detail);
        }
    }
    request_prompt = append_hidden_model_guidance_to_prompt(
        request_prompt,
        input.start.hidden_model_guidance.as_deref(),
    );
    let prompt_purpose = ExecutionPromptPurpose::UserTurn;
    let runtime_policy = vulcan_session_core::ExecutionRuntimePolicy::main_session();
    let context_injection = core_provider_context_injection_from_state_for_turn(
        &runtime_state.context_injection_state,
        Some(&input.precheck_outcome.turn_id),
    );
    let request = ExecutionPreparationRequest {
        session_id,
        execution_scope: vulcan_session_core::ExecutionScope::MainSession {
            session_id: vulcan_session_core::SessionNodeId::from(session_id),
        },
        runtime_policy,
        execution_agent_id: None,
        agent_system_prompt_override: None,
        task_session_store_root: None,
        session_title: input.start.session_title,
        workspace_path: input.start.workspace_path,
        prompt: request_prompt,
        conversation_state: runtime_state.conversation_state.clone(),
        execution_mode: input.start.execution_mode,
        temperature: runtime_state.active_temperature,
        reasoning_effort: input
            .start
            .reasoning_effort
            .or(input.runtime_reasoning_effort)
            .or(input.provider_reasoning_effort)
            .unwrap_or(ReasoningEffortLevel::Medium),
        ai_tool_configuration: input.ai_tool_configuration,
        tool_exposure_override: Default::default(),
        prompt_purpose,
        project_prompt_skills: input.project_configuration.prompt_skills,
        session_prompt_skill_overrides: runtime_state.prompt_skill_overrides.clone(),
        context_injection,
        system_prompt_hook_blocks,
        project_vmm: input.project_configuration.vmm,
        session_vmm: runtime_state.vmm.clone(),
        boundary_events: input.start.boundary_events.clone(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        session_goal_context: None,
        session_plan_context: None,
    };
    Ok(SharedPromptTurnContinuation {
        request,
        precheck_failure_detail,
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
    let vmm_memory_ready =
        effective_vmm_config.diagnose_memory_flow(true) == VmmConfigDiagnostic::Ready;
    let vmm_precheck_enabled = vmm_memory_ready && effective_vmm_config.precheck_enabled;
    let should_skip_precheck = !vmm_memory_ready
        && !precheck_hook_snapshots.is_empty()
        && precheck_hook_snapshots
            .iter()
            .all(|snapshot| snapshot.registration.id == VMM_PRECHECK_HOOK_ID);
    let progress_labels = precheck_hook_snapshots
        .iter()
        .filter(|snapshot| vmm_precheck_enabled || snapshot.registration.id != VMM_PRECHECK_HOOK_ID)
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

/// Rewrites the latest visible user prompt after a precheck transform edits it.
/// 在 precheck transform 改写提示词后回写最近一条可见用户提示词。
fn patch_latest_prechecked_user_prompt_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    prompt: &str,
    attachments: &[ConversationAttachment],
    patch_visible_user_prompt_record: bool,
) {
    if let Some(message) = runtime_state
        .conversation_state
        .messages
        .iter_mut()
        .rev()
        .find(|message| matches!(message.payload, SessionConversationPayload::UserText { .. }))
        && let SessionConversationPayload::UserText { text, .. } = &mut message.payload
    {
        *text = prompt.to_string();
    }
    if !patch_visible_user_prompt_record {
        runtime_state.updated_at = current_unix_millis_text();
        return;
    }
    if let Some(record) = runtime_state
        .conversation_state
        .records
        .iter_mut()
        .rev()
        .find(|record| {
            record.visible
                && record.kind == SessionMessageKind::User
                && record.record_kind == SessionMessageRecordKind::Transcript
                && record.title == "用户指令"
        })
    {
        record.body = build_shared_user_prompt_transcript_body(prompt, attachments);
        record.metadata = build_shared_user_prompt_transcript_metadata(prompt, attachments);
        if let Some(metadata) = record.metadata.as_mut()
            && let Some(object) = metadata.as_object_mut()
        {
            object.insert(
                String::from("canonical_text"),
                Value::String(prompt.to_string()),
            );
            object.insert(
                String::from("canonical_user_text"),
                Value::String(prompt.to_string()),
            );
        }
    }
}

/// Appends one user prompt as canonical context plus shared visible transcript record.
/// 将一条用户提示词追加为规范上下文和共享可见转录记录。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state being mutated.
/// `runtime_state` 是将被修改的会话运行态。
/// `session_id` owns the prompt record.
/// `session_id` 拥有该提示词记录。
/// `now` is the stable timestamp text used for ids.
/// `now` 是用于构建标识的稳定时间戳文本。
/// `prompt` is the canonical user-authored text.
/// `prompt` 是用户编写的规范文本。
/// `attachments` are the structured prompt attachments.
/// `attachments` 是提示词携带的结构化附件。
fn append_shared_user_prompt_record_with_attachments(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    prompt: &str,
    attachments: Vec<ConversationAttachment>,
) {
    runtime_state.ui_state.unread = false;
    let message_id = format!("session-{session_id}-protocol-user-{now}");
    append_user_text_message(
        &mut runtime_state.conversation_state,
        message_id,
        prompt.to_string(),
        attachments.clone(),
    );
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id,
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: build_shared_user_prompt_transcript_body(prompt, &attachments),
            metadata: build_shared_user_prompt_transcript_metadata(prompt, &attachments),
            created_at: now.to_string(),
        },
    );
    runtime_state.updated_at = now.to_string();
}

/// Builds the shared visible transcript body for one user prompt.
/// 为一条用户提示词构建共享的可见转录正文。
///
/// # Parameters
/// 参数
/// `prompt` is the canonical user-authored text.
/// `prompt` 是用户编写的规范文本。
/// `attachments` are the structured prompt attachments.
/// `attachments` 是提示词携带的结构化附件。
///
/// # Returns
/// 返回值
/// A transcript body matching the workbench visible-prompt projection semantics.
/// 返回与工作台可见提示词投影视图一致的转录正文。
pub fn build_shared_user_prompt_transcript_body(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> String {
    let visible_attachments = attachments
        .iter()
        .filter(|attachment| attachment.kind != ConversationAttachmentKind::Image)
        .collect::<Vec<_>>();
    let mut sections = Vec::new();
    if !prompt.trim().is_empty() {
        sections.push(prompt.trim().to_string());
    }
    if !visible_attachments.is_empty() {
        sections.push(String::from("附件与上下文："));
        sections.extend(visible_attachments.iter().map(|attachment| {
            format!(
                "- {} `{}`",
                attachment.kind.display_label(),
                attachment.file_name
            )
        }));
    }
    if sections.is_empty() && !attachments.is_empty() && visible_attachments.is_empty() {
        String::new()
    } else if sections.is_empty() {
        String::from("空白消息")
    } else {
        sections.join("\n")
    }
}

/// Builds the shared visible transcript metadata for one user prompt.
/// 为一条用户提示词构建共享的可见转录元数据。
///
/// # Parameters
/// 参数
/// `prompt` is the canonical user-authored text.
/// `prompt` 是用户编写的规范文本。
/// `attachments` are the structured prompt attachments.
/// `attachments` 是提示词携带的结构化附件。
///
/// # Returns
/// 返回值
/// Reload-safe metadata for recovering canonical text and visible attachment strips.
/// 返回可在重载后恢复规范文本和可见附件条的元数据。
pub fn build_shared_user_prompt_transcript_metadata(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> Option<Value> {
    if attachments.is_empty() {
        return None;
    }
    let serialized_attachments = serde_json::to_value(attachments).ok()?;
    Some(serde_json::json!({
        "canonical_text": prompt,
        "canonical_user_text": prompt,
        "transcript_attachments": serialized_attachments,
    }))
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

#[cfg(test)]
mod tests {
    //! Tests for shared prompt-turn routing decisions.
    //! 共享提示词回合路由决策测试。

    use super::*;
    use crate::conversation_protocol::conversation_state_to_session_messages;

    /// Begins one user-authored prompt turn used by session boundary tests.
    /// 启动一条供 会话边界测试使用的用户提示词回合。
    fn begin_user_prompt(runtime_state: &mut SessionRuntimeStateEntry) -> SharedPromptTurnStart {
        let mut hook_runtime = ConversationHookRuntime::new();
        begin_shared_prompt_turn_with_runtime_state(
            runtime_state,
            &mut hook_runtime,
            SharedPromptTurnRuntimeBeginInput {
                session_id: SessionId::from_u128(902),
                session_title: "主会话",
                workspace_path: "D:/workspace",
                prompt_entry_id: String::from("user-entry"),
                prompt: "生成一张图片",
                hidden_model_guidance: None,
                attachments: Vec::new(),
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                execution_mode: String::from("Build"),
                reasoning_effort: None,
                disabled_ai_capability_kinds: BTreeSet::new(),
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
                owning_project_id: Some(String::from("project-a")),
                prior_session_messages: None,
            },
        )
        .expect("user prompt should start")
    }

    /// Verifies user prompt begins expose visible session user-input boundary events.
    /// 验证用户提示词开始时会暴露可见的 会话用户输入边界事件。
    #[test]
    fn user_prompt_begin_exports_visible_boundary_events() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let start = begin_user_prompt(&mut runtime_state);

        assert_eq!(start.boundary_events.len(), 2);
        assert_eq!(
            start.boundary_events[0].kind,
            SessionBoundaryEventKind::TurnStarted
        );
        assert_eq!(
            start.boundary_events[0].visibility,
            SessionEventVisibility::Hidden
        );
        assert_eq!(
            start.boundary_events[1].kind,
            SessionBoundaryEventKind::UserInput
        );
        assert_eq!(
            start.boundary_events[1].visibility,
            SessionEventVisibility::Visible
        );
        assert_eq!(
            start.boundary_events[1].payload["source"],
            serde_json::json!("user_followup")
        );
    }

    /// Verifies session-mailbox turns cannot accidentally expose visible user boundary or transcript records.
    /// 验证会话 mailbox 回合不会意外暴露可见用户边界事件或转录记录。
    #[test]
    fn session_mailbox_begin_keeps_user_input_hidden_even_if_visibility_flag_is_true() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let mut hook_runtime = ConversationHookRuntime::new();
        let start = begin_shared_prompt_turn_with_runtime_state(
            &mut runtime_state,
            &mut hook_runtime,
            SharedPromptTurnRuntimeBeginInput {
                session_id: SessionId::from_u128(903),
                session_title: "主会话",
                workspace_path: "D:/workspace",
                prompt_entry_id: String::from("mailbox-entry"),
                prompt: "<subagent_mailbox_event>\n{}\n</subagent_mailbox_event>",
                hidden_model_guidance: None,
                attachments: Vec::new(),
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::SessionMailbox,
                execution_mode: String::from("Build"),
                reasoning_effort: None,
                disabled_ai_capability_kinds: BTreeSet::new(),
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
                owning_project_id: Some(String::from("project-a")),
                prior_session_messages: None,
            },
        )
        .expect("mailbox prompt should start");

        assert!(runtime_state.conversation_state.messages.is_empty());
        assert!(runtime_state.conversation_state.records.is_empty());
        assert_eq!(
            start.boundary_events[1].visibility,
            SessionEventVisibility::Hidden
        );
        assert_eq!(
            start.boundary_events[1].payload["source"],
            serde_json::json!("session_mailbox")
        );
        assert_eq!(
            start.boundary_events[1].payload["canonical"],
            serde_json::json!(false)
        );
    }

    /// Verifies goal continuation turns never persist hidden prompts as reopen-visible user text.
    /// 验证目标续跑回合绝不会把隐藏提示持久化成重开后可见的用户文本。
    #[test]
    fn goal_continuation_begin_never_persists_user_message() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let mut hook_runtime = ConversationHookRuntime::new();
        let start = begin_shared_prompt_turn_with_runtime_state(
            &mut runtime_state,
            &mut hook_runtime,
            SharedPromptTurnRuntimeBeginInput {
                session_id: SessionId::from_u128(904),
                session_title: "主会话",
                workspace_path: "D:/workspace",
                prompt_entry_id: String::from("goal-entry"),
                prompt: "Continue working toward the active thread goal.",
                hidden_model_guidance: None,
                attachments: Vec::new(),
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::GoalContinuation,
                execution_mode: String::from("Build"),
                reasoning_effort: None,
                disabled_ai_capability_kinds: BTreeSet::new(),
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
                owning_project_id: Some(String::from("project-a")),
                prior_session_messages: None,
            },
        )
        .expect("goal continuation prompt should start");

        assert!(runtime_state.conversation_state.messages.is_empty());
        assert!(runtime_state.conversation_state.records.is_empty());
        assert_eq!(
            start.boundary_events[1].visibility,
            SessionEventVisibility::Hidden
        );
        assert_eq!(
            start.boundary_events[1].payload["source"],
            serde_json::json!("goal_continuation")
        );
        assert_eq!(
            start.boundary_events[1].payload["canonical"],
            serde_json::json!(false)
        );
    }

    /// Verifies hidden model guidance reaches the provider request without polluting transcript records.
    /// 验证隐藏模型引导会进入 Provider 请求，但不会污染转录记录。
    #[test]
    fn hidden_model_guidance_stays_out_of_visible_transcript() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let mut hook_runtime = ConversationHookRuntime::new();
        let start = begin_shared_prompt_turn_with_runtime_state(
            &mut runtime_state,
            &mut hook_runtime,
            SharedPromptTurnRuntimeBeginInput {
                session_id: SessionId::from_u128(905),
                session_title: "主会话",
                workspace_path: "D:/workspace",
                prompt_entry_id: String::from("plan-execute-entry"),
                prompt: "执行计划",
                hidden_model_guidance: Some("Plan file: D:/workspace/docs/plan/demo.md"),
                attachments: Vec::new(),
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                execution_mode: String::from("Build"),
                reasoning_effort: None,
                disabled_ai_capability_kinds: BTreeSet::new(),
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
                owning_project_id: Some(String::from("project-a")),
                prior_session_messages: None,
            },
        )
        .expect("plan execution prompt should start");
        let precheck_outcome = start.precheck_request.clone().skip();
        let continuation = continue_shared_prompt_turn_after_precheck_with_runtime_state(
            &mut runtime_state,
            SharedPromptTurnContinueInput {
                start,
                precheck_outcome,
                ai_tool_configuration: PersistedAiToolConfiguration::default(),
                project_configuration: ProjectVulcanCodeConfigurationFile::default(),
                runtime_reasoning_effort: None,
                provider_reasoning_effort: None,
                patch_visible_user_prompt_record: true,
            },
        )
        .expect("prompt should continue");
        let transcript_text =
            conversation_state_to_session_messages(&runtime_state.conversation_state)
                .into_iter()
                .map(|message| message.body)
                .collect::<Vec<_>>()
                .join("\n");

        assert_eq!(transcript_text, "执行计划");
        assert!(continuation.request.prompt.contains("执行计划"));
        assert!(
            continuation
                .request
                .prompt
                .contains("<hidden_host_guidance>\nPlan file: D:/workspace/docs/plan/demo.md")
        );
    }

    /// Verifies a loaded dialogue message window is not overwritten by display-only fallback rows.
    /// 验证已加载的对话层消息窗口不会被仅显示层的回退行覆盖。
    #[test]
    fn begin_prompt_keeps_runtime_messages_when_records_are_empty() {
        // runtime_state simulates a restarted session whose API context restored messages but not display records.
        // runtime_state 模拟重启后恢复了 API 消息上下文但没有显示记录的会话。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_user_text_message(
            &mut runtime_state.conversation_state,
            "dialogue-history",
            "对话层历史",
            Vec::new(),
        );
        // display_rows simulate a visible turn window that must not become the API source of truth.
        // display_rows 模拟不应变成 API 真相来源的可见 turn 窗口。
        let display_rows = vec![SessionMessageEntry::new(
            SessionMessageKind::User,
            "用户",
            "显示层历史",
        )];
        let mut hook_runtime = ConversationHookRuntime::new();

        begin_shared_prompt_turn_with_runtime_state(
            &mut runtime_state,
            &mut hook_runtime,
            SharedPromptTurnRuntimeBeginInput {
                session_id: SessionId::from_u128(906),
                session_title: "主会话",
                workspace_path: "D:/workspace",
                prompt_entry_id: String::from("followup-entry"),
                prompt: "继续",
                hidden_model_guidance: None,
                attachments: Vec::new(),
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                execution_mode: String::from("Build"),
                reasoning_effort: None,
                disabled_ai_capability_kinds: BTreeSet::new(),
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
                owning_project_id: Some(String::from("project-a")),
                prior_session_messages: Some(&display_rows),
            },
        )
        .expect("prompt should start");

        let bodies = runtime_state
            .conversation_state
            .messages
            .iter()
            .map(|message| match &message.payload {
                SessionConversationPayload::UserText { text, .. } => text.as_str(),
                _ => "",
            })
            .collect::<Vec<_>>();
        assert_eq!(bodies, vec!["对话层历史", "继续"]);
    }
}
