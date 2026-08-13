//! WebUI conversation prompt submission orchestration.
//! WebUI 会话提示词提交编排逻辑。

use std::collections::BTreeMap;

use crate::{apply_output_style_snapshot, resolve_output_style_snapshot};
use uuid::Uuid;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, SharedPromptTurnStart,
    begin_shared_prompt_turn, restore_shared_prompt_turn_from_execution_retry,
};
use vulcan_service_api::{
    ComposerAttachmentSummary, ServiceEvent, ServiceEventEmitter, SubmitConversationPromptAck,
    UpdateConversationRunPhaseDelta, WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionKnowledgeLibrarySelection,
    SessionKnowledgeLibrarySelectionMode, SessionProviderToolTargetSelection, SessionRunPhase,
    SessionState,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use super::{
    ExistingSubmissionAdmissionAuthorization, PromptSessionMaterializationKind,
    SharedServiceBridgeState, UserPromptEditSubmitRequest, active_kernel_session_ids,
    advance_session_goal_dispatch_version,
    apply_webui_prompt_begin_outer_steps_deferred_context_persist,
    begin_existing_prompt_submission_exclusion, build_conversation_messages_patched_delta,
    build_vmm_hook_config_snapshot, capture_preserved_workspace_run_state,
    composer_attachment_summary_to_core, conversation_messages_from_session,
    current_unix_millis_text, current_unix_millis_value,
    current_workspace_snapshot_from_shared_state, emit_materialized_session_message_delta,
    emit_pending_prompts_changed_delta, emit_service_event_with_trace,
    enqueue_webui_pending_prompt_from_submission, hydrate_retained_submission_session_if_needed,
    load_workspace_for_submission, mark_webui_prompt_preparation_failed,
    mark_webui_prompt_snapshot_preparing, materialize_prompt_session,
    owning_project_id_for_session, parse_prompt_session_target, parse_session_id,
    persist_initial_submission_goal, persist_workspace_session_update, prepare_submission_media,
    preserve_active_session_run_state, provider_tool_capability_key,
    replace_conversation_messages_in_workbench_window, resolve_session_continuation_eligibility,
    retain_slow_submission_diagnostic, retained_submission_snapshot_has_session_context,
    retained_submission_snapshot_is_usable, rollback_persisted_session_before_user_prompt,
    runtime_parts_from_state, session_continuation_runtime_blockers,
    session_has_blocking_system_prompt_work, session_process_public_ai_selection,
    snapshot_webui_context_before_user_prompt, spawn_service_task,
    spawn_workspace_session_persist_task, start_reserved_webui_session_kernel_user_turn,
    start_webui_precheck_or_continue, start_webui_session_kernel_user_turn,
    store_workspace_snapshot_without_workbench_rebuild,
    sync_hydrated_session_dialogue_ready_marker, validate_composer_attachment_summary,
    webui_session_allows_user_prompt_editing, webui_session_kernel_has_active_turn,
    webui_shared_prompt_precheck_plan, webui_submit_trace, webui_trace_if_slow,
    workbench_with_live_pending_question, workspace_path_for_session,
    workspace_user_prompt_message_allows_destructive_edit,
};
mod activation;
mod edit;
mod preflight;
mod queue;
mod runtime_options;

pub(crate) use activation::*;
pub(super) use edit::*;
pub(crate) use preflight::*;
pub(crate) use queue::*;
pub(crate) use runtime_options::*;

/// Defines how an existing session obtains dialogue context before prompt submission.
/// 定义已有会话在提交提示词前如何取得对话上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExistingSessionHydrationPolicy {
    /// Loads persisted dialogue when the retained runtime context is empty.
    /// 当保留运行态上下文为空时加载持久化对话。
    HydrateIfEmpty,
    /// Trusts the retained snapshot even when a destructive edit intentionally made it empty.
    /// 即使破坏性编辑有意将其清空，也信任保留快照。
    TrustRetainedSnapshot,
}

/// Selects whether a submission creates visible user input or continues retained provider state.
/// 选择本次提交是创建可见用户输入还是继续保留的 Provider 状态。
#[derive(Debug, Clone)]
pub(super) enum ConversationSubmissionMode {
    /// Normal composer submission authored by the user.
    /// 用户通过输入区发起的普通提交。
    UserPrompt,
    /// Prompt-free continuation admitted by one exact terminal ledger generation.
    /// 由精确终态账本代际准入的无提示词 Continue。
    Continuation {
        /// Exact continuation generation projected to the caller.
        /// 投影给调用方的精确 Continue 代际。
        expected_generation: u64,
        /// Exact provider-call ledger revision projected to the caller.
        /// 投影给调用方的精确 Provider 调用账本修订号。
        expected_ledger_revision: u64,
        /// Exact frozen execution snapshot loaded and digest-validated before admission.
        /// 准入前加载并完成摘要校验的精确冻结执行快照。
        retry_snapshot: Box<vulcan_workbench_model::ExecutionRetrySnapshot>,
    },
}

impl ConversationSubmissionMode {
    /// Returns the prompt source recorded in the shared turn state.
    /// 返回共享回合状态中记录的提示词来源。
    fn prompt_source(&self) -> PendingSessionPromptSource {
        match self {
            Self::UserPrompt => PendingSessionPromptSource::UserFollowup,
            Self::Continuation { .. } => PendingSessionPromptSource::ExecutionRetry,
        }
    }

    /// Returns whether this mode should append user-authored conversation records.
    /// 返回本模式是否应追加用户编写的会话记录。
    fn appends_user_records(&self) -> bool {
        matches!(self, Self::UserPrompt)
    }

    /// Returns whether this submission is a prompt-free continuation.
    /// 返回本次提交是否为无提示词 Continue。
    fn is_continuation(&self) -> bool {
        matches!(self, Self::Continuation { .. })
    }

    /// Returns the exact continuation generation and ledger revision when applicable.
    /// 在适用时返回精确 Continue 代际与账本修订号。
    fn continuation_expectation(&self) -> Option<(u64, u64)> {
        match self {
            Self::UserPrompt => None,
            Self::Continuation {
                expected_generation,
                expected_ledger_revision,
                ..
            } => Some((*expected_generation, *expected_ledger_revision)),
        }
    }

    /// Returns the exact frozen retry snapshot carried by a manual continuation.
    /// 返回手动 Continue 携带的精确冻结重试快照。
    fn retry_snapshot(&self) -> Option<&vulcan_workbench_model::ExecutionRetrySnapshot> {
        match self {
            Self::UserPrompt => None,
            Self::Continuation { retry_snapshot, .. } => Some(retry_snapshot),
        }
    }
}

/// Validates one frozen retry sidecar against the exact current session state before CAS claim.
/// 在 CAS 领取前，根据精确当前会话状态校验一份冻结的重试 sidecar。
///
/// # Parameters
/// 参数
/// `runtime_state` is the authoritative retained session state at admission.
/// `runtime_state` 是准入时的权威保留会话状态。
/// `session_id` is the exact owner selected by the Continue RPC.
/// `session_id` 是 Continue RPC 选定的精确所有者。
/// `snapshot` is the digest-validated complete sidecar loaded from storage.
/// `snapshot` 是从存储加载且已完成摘要校验的完整 sidecar。
///
/// # Returns
/// 返回值
/// Success only when route, context generation, canonical state, lifecycle, and pending action are all replay-safe.
/// 仅当路由、上下文代际、规范状态、生命周期与待执行动作全部可安全重放时返回成功。
pub(crate) fn validate_execution_retry_snapshot_for_claim(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
    session_id: SessionId,
    snapshot: &vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<(), String> {
    if snapshot.session_id != session_id.to_string() {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_OWNER_MISMATCH",
        ));
    }
    if snapshot.state != vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen
        || snapshot.claimed_attempt_id.is_some()
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN",
        ));
    }
    if !matches!(
        &snapshot.pending_action,
        vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation { .. }
            | vulcan_workbench_model::ExecutionRetryPendingAction::DispatchProviderRequest { .. }
    ) {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_ACTION_UNSUPPORTED",
        ));
    }
    if snapshot
        .error
        .as_ref()
        .is_some_and(|error| !error.retryable)
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE",
        ));
    }
    if runtime_state.provider_route_lease.as_ref() != Some(&snapshot.provider_route) {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED",
        ));
    }
    if runtime_state
        .provider_call_ledger
        .active_context_generation_id
        != snapshot.context_generation_id
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_CONTEXT_GENERATION_CHANGED",
        ));
    }
    // canonicalSnapshot uses the same lossless bridge used by provider runtime bootstrap.
    // canonicalSnapshot 使用与 Provider 运行时 bootstrap 相同的无损桥接。
    let canonical_snapshot =
        vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
            &runtime_state.conversation_state,
        )?;
    let canonical_bytes = serde_json::to_vec(&canonical_snapshot)
        .map_err(|error| format!("Failed to serialize current retry-claim Core state: {error}"))?;
    if execution_retry_sha256_hex(&canonical_bytes) != snapshot.canonical_digest {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED",
        ));
    }
    Ok(())
}

/// Invalidates an older retry snapshot when the user starts a different logical execution.
/// 当用户启动不同逻辑执行时使旧重试快照失效。
fn invalidate_execution_retry_snapshot_for_user_prompt(
    controller_runtime: &vulcan_workspace_store::data_plane::ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    let reference = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone());
    let Some(reference) = reference else {
        return Ok(());
    };
    if matches!(
        reference.state,
        vulcan_workbench_model::ExecutionRetrySnapshotState::Consumed
            | vulcan_workbench_model::ExecutionRetrySnapshotState::Invalidated
    ) {
        return Ok(());
    }
    let mut snapshot = vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
        &controller_runtime.data_root,
        session_id,
        &reference,
    )?;
    snapshot.state = vulcan_workbench_model::ExecutionRetrySnapshotState::Invalidated;
    snapshot.claimed_attempt_id = None;
    snapshot.updated_at = current_unix_millis_text();
    let invalidated_reference =
        vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
            &controller_runtime.data_root,
            session_id,
            &snapshot,
        )?;
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| {
            String::from("Execution retry runtime state disappeared while invalidating.")
        })?;
    runtime_state.execution_retry_snapshot = Some(invalidated_reference);
    Ok(())
}

/// Computes a lowercase SHA-256 digest for exact retry-claim comparison bytes.
/// 为精确重试领取比较字节计算小写 SHA-256 摘要。
fn execution_retry_sha256_hex(encoded: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Persists the accepted pre-provider preparation boundary before the submit acknowledgement.
/// 在提交确认返回前持久化已接纳的 Provider 调用前准备边界。
///
/// # Parameters
/// 参数
/// - `controller_runtime`: Workspace data-plane owner used for atomic sidecar persistence.
/// - `controller_runtime`：用于原子持久化 sidecar 的工作区数据面所有者。
/// - `workspace_snapshot`: Detached accepted workspace state that receives the lightweight reference.
/// - `workspace_snapshot`：接收轻量引用的分离已接纳工作区状态。
/// - `session_id`: Session that owns the accepted prompt turn.
/// - `session_id`：拥有已接纳提示词回合的会话。
/// - `physical_attempt_id`: Submission identity that owns the first physical attempt.
/// - `physical_attempt_id`：拥有首次物理尝试的提交身份。
/// - `start`: Exact started prompt-turn state captured before deferred preparation.
/// - `start`：延迟准备前捕获的精确已启动提示词回合状态。
///
/// # Returns
/// 返回值
/// Success after both sidecar persistence and runtime-reference publication complete.
/// sidecar 持久化与运行态引用发布均完成后返回成功。
fn persist_accepted_preparation_retry_checkpoint(
    controller_runtime: &vulcan_workspace_store::data_plane::ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    physical_attempt_id: &str,
    start: &SharedPromptTurnStart,
) -> Result<(), String> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("Accepted preparation retry runtime state is missing."))?;
    let provider_route = runtime_state
        .provider_route_lease
        .clone()
        .ok_or_else(|| String::from("Accepted preparation retry has no frozen provider route."))?;
    let controller_snapshot = vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
        &runtime_state.conversation_state,
    )?;
    let canonical_bytes = serde_json::to_vec(&controller_snapshot).map_err(|error| {
        format!("Failed to serialize accepted preparation retry Core state: {error}")
    })?;
    let generation = runtime_state
        .execution_retry_snapshot
        .as_ref()
        .map(|reference| reference.generation)
        .unwrap_or_default()
        .checked_add(1)
        .ok_or_else(|| String::from("Execution retry generation overflowed."))?;
    let now = current_unix_millis_text();
    let snapshot = vulcan_workbench_model::ExecutionRetrySnapshot {
        schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
        snapshot_id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        generation,
        state: vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen,
        logical_execution_id: physical_attempt_id.to_string(),
        logical_turn_id: Some(start.preparation_turn_id.clone()),
        physical_attempt_id: physical_attempt_id.to_string(),
        claimed_attempt_id: None,
        provider_route,
        provider_service_entry_id: None,
        credential_profile_id: None,
        controller_config: None,
        controller_snapshot,
        pending_action: vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation {
            input: Box::new(start.execution_retry_preparation_input()?),
        },
        error: None,
        canonical_digest: execution_retry_sha256_hex(&canonical_bytes),
        context_generation_id: runtime_state
            .provider_call_ledger
            .active_context_generation_id
            .clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let reference = vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
        &controller_runtime.data_root,
        session_id,
        &snapshot,
    )?;
    runtime_state.execution_retry_snapshot = Some(reference);
    runtime_state.updated_at = now;
    Ok(())
}

/// Freezes the exact provider, model, and credential selected when a prompt is submitted.
/// 固定提示词提交时选择的精确供应商、模型与凭据。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SubmissionModelSelection {
    /// Stable provider identifier selected in the composer.
    /// 输入区中选择的稳定供应商标识。
    pub(crate) provider_id: String,
    /// Stable model identifier selected in the composer.
    /// 输入区中选择的稳定模型标识。
    pub(crate) model_id: String,
    /// Exact credential connection selected in the composer.
    /// 输入区中选择的精确凭据连接。
    pub(crate) connection_id: String,
}

/// Captures one tri-state VMM policy pair selected before a conversation exists.
/// 捕获会话创建前选择的一组 VMM 三态策略。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionVmmPolicy {
    /// Precheck-memory injection mode.
    /// 预检记忆注入模式。
    precheck_mode: String,
    /// Post-turn memory writeback mode.
    /// 回合后记忆回写模式。
    memory_writeback_mode: String,
}

/// Captures one explicit knowledge-library selection for the materialized session.
/// 捕获用于已物化会话的一份显式知识库选择。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionKnowledgeSelection {
    /// Selection mode, either inherit or explicit.
    /// 选择模式，仅可为继承或显式。
    mode: String,
    /// Exact authorized library identifiers.
    /// 精确且已授权的知识库标识列表。
    library_ids: Vec<String>,
    /// Optional explicit retrieval policy for knowledge tools.
    /// 知识库工具可选的显式检索策略。
    #[serde(default)]
    retrieval_mode: Option<String>,
}

/// Captures one initial user goal created before the first prompt is submitted.
/// 捕获首次提示词提交前创建的初始用户目标。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionInitialGoal {
    /// Human-authored goal objective.
    /// 用户编写的目标文本。
    objective: String,
    /// Optional positive token budget.
    /// 可选正数 Token 预算。
    token_budget: Option<i64>,
}

/// Freezes one complete provider tool snapshot for a new conversation.
/// 固定新会话的一份完整供应商工具快照。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmissionProviderToolSettings {
    /// Capabilities enabled only for the materialized conversation.
    /// 仅为物化会话启用的能力。
    enabled_capabilities:
        std::collections::BTreeSet<vulcan_provider_adapters::ProviderToolCapabilityKind>,
    /// Exact model and account retained independently from activation.
    /// 与启用状态独立保留的精确模型与账号。
    selections: BTreeMap<
        vulcan_provider_adapters::ProviderToolCapabilityKind,
        vulcan_provider_adapters::ProviderToolTargetSelection,
    >,
}

/// Freezes every draft-only session setting required by the first provider turn.
/// 固定首个 Provider 回合所需的全部草稿会话设置。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionSessionSettings {
    /// Optional session output-style identifier captured before the first prompt boundary.
    /// 在首次提示词边界前捕获的可选会话输出风格标识。
    #[serde(default)]
    output_style_id: Option<String>,
    /// Optional explicit VMM policy pair; absence preserves inherited defaults.
    /// 可选显式 VMM 策略对；缺省时保留继承默认值。
    vmm_policy: Option<SubmissionVmmPolicy>,
    /// Optional complete provider-tool snapshot; absence preserves an explicit session override or inherits current global defaults.
    /// 可选完整供应商工具快照；缺省时保留会话显式覆盖，或继承当前全局默认值。
    provider_tool_settings: Option<SubmissionProviderToolSettings>,
    /// Optional knowledge selection; absence preserves project inheritance.
    /// 可选知识库选择；缺省时保留项目继承。
    knowledge_selection: Option<SubmissionKnowledgeSelection>,
    /// Optional initial goal created before the first turn starts.
    /// 首轮开始前创建的可选初始目标。
    initial_goal: Option<SubmissionInitialGoal>,
}

/// Reports whether an absent per-turn snapshot must preserve an explicit session override.
/// 判断缺失的本回合快照是否必须保留会话显式覆盖。
fn should_preserve_existing_provider_tool_settings(
    has_explicit_settings: bool,
    provider_tool_settings_initialized: bool,
    provider_tool_selections_overridden: bool,
) -> bool {
    !has_explicit_settings
        && provider_tool_settings_initialized
        && provider_tool_selections_overridden
}

/// Converts one submitted VMM tri-state mode into the persisted optional layer value.
/// 将一个已提交的 VMM 三态模式转换为持久化可选层取值。
fn submission_vmm_policy_value(mode: &str) -> Result<Option<bool>, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "inherit" => Ok(None),
        "enabled" => Ok(Some(true)),
        "disabled" => Ok(Some(false)),
        other => Err(format!("不支持的 VMM 记忆策略模式：{other}")),
    }
}

/// Resolves output style only when the submission is crossing a materialization boundary.
/// 仅当提交跨越会话物化边界时解析输出风格。
fn submission_output_style_snapshot(
    settings: Option<&SubmissionSessionSettings>,
    global_output_style_selection: vulcan_agent_runtime::provider_config::PersistedPromptOutputStyleSelection,
    allow_output_style_activation: bool,
) -> Result<Option<vulcan_prompt_composer::PromptOutputStyleSnapshot>, String> {
    if !allow_output_style_activation {
        return Ok(None);
    }
    resolve_output_style_snapshot(
        submission_output_style_id(settings).as_deref(),
        global_output_style_selection,
    )
    .map(Some)
}

/// Normalizes the explicit style override captured by a new-session submission.
/// 规范化新会话提交中捕获的显式输出风格覆盖。
fn submission_output_style_id(settings: Option<&SubmissionSessionSettings>) -> Option<String> {
    settings
        .and_then(|settings| settings.output_style_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Validates and applies draft-only settings before the first precheck or provider request.
/// 在首次预检或 Provider 请求前校验并应用草稿专属设置。
async fn apply_submission_session_settings(
    state: &SharedServiceBridgeState,
    controller_runtime: &vulcan_workspace_store::data_plane::ControllerRuntimeHandle,
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    execution_mode: Option<&str>,
    settings: Option<&SubmissionSessionSettings>,
    allow_output_style_activation: bool,
) -> Result<(), String> {
    if settings
        .and_then(|settings| settings.initial_goal.as_ref())
        .is_some()
        && execution_mode.is_some_and(|mode| mode.eq_ignore_ascii_case("plan"))
    {
        return Err(String::from(
            "目标模式与计划模式互斥，不能在同一首次提交中同时启用。",
        ));
    }
    let configuration = runtime.configuration_snapshot()?;
    let global_output_style_selection = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
        .provider_configuration
        .prompt
        .output_style;
    let output_style_snapshot = submission_output_style_snapshot(
        settings,
        global_output_style_selection,
        allow_output_style_activation,
    )?;
    let provider_tool_settings =
        settings.and_then(|settings| settings.provider_tool_settings.as_ref());
    let has_explicit_provider_tool_settings = provider_tool_settings.is_some();
    let preserve_existing_provider_tools = should_preserve_existing_provider_tool_settings(
        has_explicit_provider_tool_settings,
        workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .is_some_and(|runtime_state| runtime_state.provider_tool_settings_initialized),
        workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .is_some_and(|runtime_state| runtime_state.provider_tool_selections_overridden),
    );
    if !preserve_existing_provider_tools {
        let enabled_capabilities = provider_tool_settings
            .map(|settings| &settings.enabled_capabilities)
            .unwrap_or(&configuration.provider_tool_enabled_capabilities);
        let provider_tool_selections = provider_tool_settings
            .map(|settings| &settings.selections)
            .unwrap_or(&configuration.provider_tool_selections);
        crate::public_ai_settings::validate_provider_tool_session_settings(
            runtime,
            &configuration,
            enabled_capabilities,
            provider_tool_selections,
        )?;
    }
    let knowledge_selection = if let Some(selection) =
        settings.and_then(|settings| settings.knowledge_selection.as_ref())
    {
        let mode = match selection.mode.as_str() {
            "inherit" => SessionKnowledgeLibrarySelectionMode::Inherit,
            "explicit" => SessionKnowledgeLibrarySelectionMode::Explicit,
            other => return Err(format!("未知知识库选择模式：{other}")),
        };
        let knowledge_runtime = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .knowledge_runtime
            .clone()
            .ok_or_else(|| {
                String::from("service is still starting; knowledge runtime is not ready")
            })?;
        let project_id = owning_project_id_for_session(workspace_snapshot, session_id)
            .ok_or_else(|| String::from("当前会话不属于可用项目。"))?;
        let authorized = knowledge_runtime
            .authorized_library_ids(&project_id, false)
            .await?
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(denied) = selection
            .library_ids
            .iter()
            .find(|library_id| !authorized.contains(*library_id))
        {
            return Err(format!("知识库未授权给当前项目：{denied}"));
        }
        if let Some(retrieval_mode) = selection.retrieval_mode.as_deref()
            && !matches!(
                retrieval_mode,
                "raw_bm25" | "display_bm25" | "display_hybrid"
            )
        {
            return Err(format!("未知知识库检索策略：{retrieval_mode}"));
        }
        Some(
            SessionKnowledgeLibrarySelection {
                mode,
                library_ids: selection.library_ids.clone(),
                retrieval_mode: selection.retrieval_mode.clone(),
            }
            .normalized(),
        )
    } else {
        None
    };
    let updated_at = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    if let Some(policy) = settings.and_then(|settings| settings.vmm_policy.as_ref()) {
        runtime_state.vmm.precheck_enabled = submission_vmm_policy_value(&policy.precheck_mode)?;
        runtime_state.vmm.memory_writeback_enabled =
            submission_vmm_policy_value(&policy.memory_writeback_mode)?;
    }
    if !preserve_existing_provider_tools {
        let enabled_capabilities = provider_tool_settings
            .map(|settings| &settings.enabled_capabilities)
            .unwrap_or(&configuration.provider_tool_enabled_capabilities);
        let provider_tool_selections = provider_tool_settings
            .map(|settings| &settings.selections)
            .unwrap_or(&configuration.provider_tool_selections);
        runtime_state.provider_tool_selections = provider_tool_selections
            .iter()
            .map(|(capability, selection)| {
                (
                    provider_tool_capability_key(*capability),
                    SessionProviderToolTargetSelection {
                        connection_id: selection.connection_id.clone(),
                        target_id: selection.target_id.clone(),
                    },
                )
            })
            .collect();
        runtime_state.provider_tool_enabled_capabilities = enabled_capabilities
            .iter()
            .map(|capability| provider_tool_capability_key(*capability))
            .collect();
        runtime_state.provider_tool_settings_initialized = true;
        runtime_state.provider_tool_selections_overridden = has_explicit_provider_tool_settings;
    }
    if let Some(selection) = knowledge_selection {
        runtime_state.knowledge_library_selection = selection;
    }
    if allow_output_style_activation {
        // An explicit draft choice must remain session-owned after creation; null/empty inherits global.
        // 新会话草稿的显式选择必须在创建后归属会话；空值继续继承全局。
        runtime_state.output_style.override_style_id = submission_output_style_id(settings);
    }
    if let Some(output_style_snapshot) = output_style_snapshot {
        apply_output_style_snapshot(
            runtime_state,
            &output_style_snapshot,
            "new-session",
            None,
            updated_at.clone(),
        );
    }
    runtime_state.updated_at = updated_at;
    if let Some(goal) = settings.and_then(|settings| settings.initial_goal.as_ref()) {
        persist_initial_submission_goal(
            controller_runtime,
            session_id,
            &goal.objective,
            goal.token_budget,
        )
        .await?;
        advance_session_goal_dispatch_version(state, session_id)?;
    }
    Ok(())
}

/// Validates and applies one submission-scoped model selection to session runtime state.
/// 校验并将单次提交范围的模型选择写入会话运行态。
///
/// # Parameters
/// 参数
/// `runtime` owns the authoritative provider configuration and discovered catalogs.
/// `runtime` 持有权威供应商配置与已发现目录。
/// `workspace_snapshot` contains the materialized session runtime state to update.
/// `workspace_snapshot` 包含待更新的已物化会话运行态。
/// `session_id` identifies the exact submitted session.
/// `session_id` 标识本次提交对应的精确会话。
/// `selection` is absent only when the composer has no available model.
/// `selection` 仅在输入区没有可用模型时缺失。
///
/// # Returns
/// 返回值
/// Returns success after an exact configured selection is applied, or a validation error.
/// 在精确已配置选择写入后返回成功，否则返回校验错误。
fn apply_submission_model_selection(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    selection: Option<&SubmissionModelSelection>,
) -> Result<(), String> {
    let Some(selection) = selection else {
        return Ok(());
    };
    let configuration = runtime.configuration_snapshot()?;
    let configured_connection = configuration
        .connection_graph
        .connections
        .get(selection.connection_id.as_str())
        .ok_or_else(|| {
            format!(
                "Connection `{}` is not configured.",
                selection.connection_id
            )
        })?;
    if configured_connection.provider_id != selection.provider_id {
        return Err(format!(
            "Connection `{}` does not belong to provider `{}`.",
            selection.connection_id, selection.provider_id
        ));
    }
    // leasedSelection is evaluated before current-catalog membership because an established
    // session route is intentionally allowed to outlive a hot Manifest model removal.
    // leasedSelection 在当前目录成员关系之前求值，因为已建立会话路由被明确允许在热更新
    // Manifest 移除模型后继续存在。
    let leased_selection = session_process_public_ai_selection(
        runtime,
        session_id,
        workspace_snapshot.session_runtime_state.get(&session_id),
    )?
    .filter(|leased| {
        leased.route.provider.id == selection.provider_id
            && leased.route.connection.id == selection.connection_id
            && leased.model.id == selection.model_id
    });
    if leased_selection.is_some() {
        // Explicit provider/account access revocation still terminates the lease. Manifest-owned
        // catalog/default changes are not treated as user revocation.
        // 显式供应商或账号访问撤销仍会终止租约；Manifest 持有的目录或默认值变化不视为用户撤销。
        let explicitly_disabled = configuration
            .model_enabled_override(selection.provider_id.as_str(), selection.model_id.as_str())
            == Some(false)
            || !configuration.connection_model_is_enabled(
                selection.connection_id.as_str(),
                selection.model_id.as_str(),
            );
        if explicitly_disabled {
            return Err(format!(
                "Provider `{}` model `{}` is disabled.",
                selection.provider_id, selection.model_id
            ));
        }
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        runtime_state.updated_at = current_unix_millis_text();
        return Ok(());
    }
    if !runtime.model_is_enabled_for_connection(
        &configuration,
        selection.connection_id.as_str(),
        selection.model_id.as_str(),
    )? {
        return Err(format!(
            "Provider `{}` model `{}` is disabled.",
            selection.provider_id, selection.model_id
        ));
    }
    // Validate against the same reconstructed catalog used by execution so a just-saved
    // capability override cannot be hidden by a stale process-local catalog copy.
    // 使用与执行相同的重建目录校验，避免刚保存的能力覆盖被旧进程内目录副本遮蔽。
    let resolved_catalogs = runtime.resolved_model_catalog_snapshot(&configuration)?;
    let source_catalogs = runtime.source_catalog_snapshot()?;
    let effective_catalogs = if resolved_catalogs.contains_key(selection.connection_id.as_str())
        || source_catalogs.contains_key(selection.connection_id.as_str())
    {
        resolved_catalogs
    } else {
        // Manual/legacy catalogs without immutable source tracking still use their process-local
        // snapshot; this fallback does not bypass overlays for source-backed connections.
        // 没有不可变源跟踪的手工/旧版目录仍使用进程内快照；源目录连接不会绕过覆盖链路。
        runtime.catalog_snapshot()?
    };
    let connection_exposes_model = effective_catalogs
        .get(selection.connection_id.as_str())
        .is_some_and(|catalog| {
            catalog
                .models
                .iter()
                .any(|model| model.id == selection.model_id)
        });
    if !connection_exposes_model {
        return Err(format!(
            "Connection `{}` does not expose model `{}`.",
            selection.connection_id, selection.model_id
        ));
    }
    runtime.resolve_connection_model_selection(
        &configuration,
        selection.connection_id.as_str(),
        selection.provider_id.as_str(),
        selection.model_id.as_str(),
    )?;
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    // targetChanged distinguishes the composer's per-submit snapshot from an explicit model
    // change. The composer sends its current selection on every turn, so clearing an unchanged
    // target here would silently adopt a newly activated Manifest route on the next turn.
    // targetChanged 区分输入区逐次提交快照与显式模型变更。输入区每轮都会发送当前选择，
    // 因此若在目标未变化时清除租约，下一轮会静默采用新激活的 Manifest 路由。
    let target_changed = runtime_state.active_provider_id.as_deref()
        != Some(selection.provider_id.as_str())
        || runtime_state.active_connection_id.as_deref() != Some(selection.connection_id.as_str())
        || runtime_state.active_model_id.as_deref() != Some(selection.model_id.as_str());
    if target_changed {
        runtime.clear_session_selection_leases(session_id.to_string().as_str())?;
        runtime_state.provider_route_lease = None;
    }
    runtime_state.active_provider_id = Some(selection.provider_id.clone());
    runtime_state.active_connection_id = Some(selection.connection_id.clone());
    runtime_state.active_model_id = Some(selection.model_id.clone());
    runtime_state.updated_at = current_unix_millis_text();
    Ok(())
}

#[cfg(test)]
mod submission_model_selection_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use vulcan_provider_adapters::{
        CredentialConnection, CredentialMode, PersistedAiConfiguration, PublicAiRuntime,
        UnavailableCredentialSecretResolver, parse_openrouter_model_catalog,
    };
    use vulcan_workbench_model::SessionSummary;

    use super::*;
    use crate::ensure_webui_session_runtime_state;

    /// Builds one configured runtime whose global active model differs from the submitted model.
    /// 构建一个全局激活模型与本次提交模型不同的已配置运行时。
    /// Adds one protocol variant for each protocol id to every model of one catalog.
    /// 给一个目录的每个模型按协议 ID 各添加一个协议变体。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_profile_ids` are the exact protocols the models bind to; every model
    /// receives a variant for each id so system-manifest bindings can prove their exact
    /// variant before execution (plan §R5.1).
    /// `protocol_profile_ids` 是模型绑定的精确协议；每个模型为每个 ID 获得一个变体，
    /// 使系统清单绑定能够在执行前证明其精确变体（方案 §R5.1）。
    fn with_protocol_variants(
        mut catalog: vulcan_provider_adapters::ConnectionModelCatalog,
        protocol_profile_ids: &[&str],
    ) -> vulcan_provider_adapters::ConnectionModelCatalog {
        for model in &mut catalog.models {
            for protocol_profile_id in protocol_profile_ids {
                model.protocol_variants.insert(
                    (*protocol_profile_id).to_string(),
                    vulcan_provider_adapters::ProtocolModelVariant {
                        capabilities: model.capabilities.clone(),
                        capability_evidence: Default::default(),
                        discovery_signature: String::from("test-signature"),
                        catalog_generation: catalog.generation,
                    },
                );
            }
        }
        catalog
    }

    fn configured_runtime() -> PublicAiRuntime {
        let runtime =
            PublicAiRuntime::first_batch(std::sync::Arc::new(UnavailableCredentialSecretResolver))
                .expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let connection = CredentialConnection {
            id: String::from("openrouter-account"),
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential-openrouter-account"),
            display_name: String::from("OpenRouter account"),
            secret_reference: String::from("secret-openrouter-account"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([
                String::from("openai-chat-completions.v1"),
                String::from("openai-responses.v1"),
            ]),
            selected_protocol: String::from("openai-chat-completions.v1"),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection,
            )
            .expect("connection should register");
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-terra", true);
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
        configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-terra"),
            connection_id: String::from("openrouter-account"),
        });
        runtime
            .stage_source_catalog_for_testing(with_protocol_variants(
                parse_openrouter_model_catalog(
                    "openrouter-account",
                    1,
                    r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Previous","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},{"id":"openai/gpt-5.6-sol","name":"Submitted","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
                )
                .expect("catalog should parse"),
                &[vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID],
            ))
            .expect("source catalog should stage");
        runtime
            .replace_configuration(configuration.clone())
            .expect("configuration should install");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("configured test generation should publish");
        runtime
    }

    /// Verifies a new session uses the model snapshot captured by submit instead of the previous global model.
    /// 验证新会话使用提交时捕获的模型快照，而不是上一次全局模型。
    #[test]
    fn submitted_model_selection_overrides_previous_global_selection() {
        let runtime = configured_runtime();
        let session_id = Uuid::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "New conversation",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)
            .expect("session runtime state should initialize");
        let selection = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("openrouter-account"),
        };

        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("submitted selection should apply");

        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("session runtime state should exist");
        assert_eq!(
            runtime_state.active_provider_id.as_deref(),
            Some("openrouter")
        );
        assert_eq!(
            runtime_state.active_connection_id.as_deref(),
            Some("openrouter-account")
        );
        assert_eq!(
            runtime_state.active_model_id.as_deref(),
            Some("openai/gpt-5.6-sol")
        );
        assert_eq!(
            runtime
                .configuration_snapshot()
                .expect("configuration should remain readable")
                .active
                .as_ref()
                .map(|active| active.model_id.as_str()),
            Some("openai/gpt-5.6-terra")
        );
    }

    /// Verifies an unchanged per-submit model snapshot preserves the session route lease.
    /// 验证逐次提交中未变化的模型快照会保留会话路由租约。
    #[test]
    fn repeated_submission_selection_preserves_lease_until_target_changes() {
        let runtime = configured_runtime();
        let session_id = Uuid::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "Existing conversation",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)
            .expect("session runtime state should initialize");
        let selection = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("openrouter-account"),
        };
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("initial selection should apply");

        // leasedSelection simulates the immutable route installed by the first prepared turn.
        // leasedSelection 模拟首个已准备回合安装的不可变路由。
        let configuration = runtime
            .configuration_snapshot()
            .expect("configuration should remain readable");
        let leased_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                selection.connection_id.as_str(),
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            )
            .expect("selected route should resolve");
        let lease_identity =
            vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
                &leased_selection,
            );
        runtime
            .lease_session_selection(session_id.to_string(), leased_selection.clone())
            .expect("session route should lease");
        workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .expect("session runtime state should exist")
            .provider_route_lease = Some(lease_identity.clone());

        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("unchanged submission snapshot should apply");
        assert_eq!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("session lease should remain readable")
                .as_ref()
                .map(|selection| selection.route_revision.as_str()),
            Some(leased_selection.route_revision.as_str()),
            "an unchanged per-submit snapshot must not clear the process-local lease"
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.provider_route_lease.as_ref()),
            Some(&lease_identity),
            "an unchanged per-submit snapshot must not clear the persisted lease identity"
        );

        // replacementCatalog simulates a hot catalog generation that removes the leased model.
        // replacementCatalog 模拟移除已租用模型的热更新目录代际。
        let replacement_catalog = with_protocol_variants(
            parse_openrouter_model_catalog(
                "openrouter-account",
                2,
                r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Replacement","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
            )
            .expect("replacement catalog should parse"),
            &[vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID],
        );
        runtime
            .stage_source_catalog_for_testing(replacement_catalog)
            .expect("replacement source catalog should stage");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("replacement generation should publish");
        let replacement_catalogs = runtime
            .catalog_snapshot()
            .expect("replacement catalogs should remain readable");
        assert!(
            replacement_catalogs
                .get("openrouter-account")
                .is_some_and(|catalog| catalog
                    .models
                    .iter()
                    .all(|model| model.id != selection.model_id)),
            "the current catalog must actually remove the leased model"
        );

        let selected_runtime_state = workspace_snapshot.session_runtime_state.get(&session_id);
        let workbench_selection = crate::public_ai_workbench_selection(
            &configuration,
            &runtime,
            Some(session_id),
            selected_runtime_state,
        )
        .expect("lease-aware workbench selection should resolve")
        .expect("the process-local lease must remain the effective selection");
        assert_eq!(
            workbench_selection.route_revision, leased_selection.route_revision,
            "a hot catalog removal must not replace the selected session route"
        );
        let rows = crate::public_ai_provider_rows_for_selection(
            &configuration,
            &runtime,
            Some(&workbench_selection),
        )
        .expect("lease-aware provider rows should project");
        assert!(
            rows.iter()
                .any(|row| row.catalog_models.iter().any(|model| {
                    model.id == selection.model_id && model.active && model.status.is_some()
                })),
            "the leased model must remain visible only as the selected session row"
        );
        let changed_sessions =
            crate::public_ai_settings::reconcile_missing_session_model_selections(
                &mut workspace_snapshot,
                &runtime,
                &configuration,
                &replacement_catalogs,
            )
            .expect("lease-aware session reconciliation should succeed");
        assert!(
            changed_sessions.is_empty(),
            "hot catalog removal must not reconcile an established session lease"
        );
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&selection),
        )
        .expect("the unchanged leased selection must survive current catalog removal");
        assert!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("session lease should remain readable")
                .is_some(),
            "per-submit validation must not clear a model removed only by hot catalog refresh"
        );

        // Clearing only the process snapshot simulates a restart while persisted lease identity
        // remains authoritative for drift detection.
        // 只清除进程快照用于模拟重启，而持久化租约身份仍对漂移检测保持权威。
        runtime
            .clear_session_selection_leases(session_id.to_string().as_str())
            .expect("process-local lease should clear for restart simulation");
        let changed_after_restart =
            crate::public_ai_settings::reconcile_missing_session_model_selections(
                &mut workspace_snapshot,
                &runtime,
                &configuration,
                &replacement_catalogs,
            )
            .expect("restart reconciliation should preserve persisted lease evidence");
        assert!(
            changed_after_restart.is_empty(),
            "restart reconciliation must not silently replace a drifted persisted lease"
        );
        let restarted_runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("session runtime state should remain present");
        assert_eq!(
            restarted_runtime_state.active_model_id.as_deref(),
            Some(selection.model_id.as_str()),
            "the persisted target must remain available to exact recovery validation"
        );
        assert_eq!(
            restarted_runtime_state.provider_route_lease.as_ref(),
            Some(&lease_identity),
            "the persisted lease identity must survive restart catalog reconciliation"
        );

        let replacement = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-terra"),
            connection_id: String::from("openrouter-account"),
        };
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            Some(&replacement),
        )
        .expect("changed submission target should apply");
        assert!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("session lease should remain readable")
                .is_none(),
            "an explicit target change must clear the process-local lease"
        );
        assert!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.provider_route_lease.as_ref())
                .is_none(),
            "an explicit target change must clear the persisted lease identity"
        );
    }
}

/// Determines whether an existing submission must hydrate missing dialogue from persistence.
/// 判断已有会话提交是否必须从持久化层水合缺失对话。
///
/// # Parameters
/// 参数
/// `session_id` is present only for submissions targeting an existing session.
/// `session_id` 仅在提交目标为已有会话时存在。
/// `policy` describes whether the retained snapshot is authoritative when empty.
/// `policy` 描述保留快照为空时是否仍具权威性。
///
/// # Returns
/// 返回值
/// Returns true only when an existing ordinary submission requires hydration.
/// 仅当已有会话的普通提交需要水合时返回 true。
fn existing_submission_requires_hydration(
    session_id: Option<SessionId>,
    policy: ExistingSessionHydrationPolicy,
) -> bool {
    session_id.is_some() && policy == ExistingSessionHydrationPolicy::HydrateIfEmpty
}

/// Inserts one client submission identifier into a visible user record and its canonical runtime record.
/// 将一个客户端提交标识写入可见用户记录及其规范运行时记录。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the newly appended transcript and runtime records.
/// `workspace_snapshot` 持有刚追加的转录记录与运行时记录。
/// `session_id` selects the exact accepted conversation.
/// `session_id` 选择精确的已接纳会话。
/// `message_id` is the visible user record identifier returned by the shared turn reducer.
/// `message_id` 是共享回合 reducer 返回的可见用户记录标识。
/// `client_submission_id` is the stable browser-generated correlation identifier.
/// `client_submission_id` 是浏览器生成的稳定关联标识。
/// `submission_target` identifies a new or existing conversation submission.
/// `submission_target` 标识新会话或已有会话提交。
/// `accepted_at_ms` captures the service acceptance boundary for first-semantic latency.
/// `accepted_at_ms` 捕获用于首个语义输出延迟的服务接纳边界。
///
/// # Returns
/// 返回值
/// Success after both projections own the identifier, or an exact record-shape diagnostic.
/// 两个投影都拥有标识后返回成功，否则返回精确的记录结构诊断。
pub(crate) fn attach_client_submission_id_to_visible_user_record(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    message_id: Option<&str>,
    client_submission_id: &str,
    submission_target: &str,
    accepted_at_ms: i128,
) -> Result<(), String> {
    let Some(message_id) = message_id else {
        return Ok(());
    };
    // insert_identifier preserves the established object metadata contract and rejects a corrupted non-object value.
    // insert_identifier 保留既有对象元数据契约，并拒绝损坏的非对象值。
    let insert_identifier = |metadata: &mut Option<serde_json::Value>| -> Result<(), String> {
        if metadata.is_none() {
            *metadata = Some(serde_json::Value::Object(serde_json::Map::new()));
        }
        let object = metadata
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| {
                format!("Visible user record `{message_id}` has non-object metadata.")
            })?;
        object.insert(
            String::from("client_submission_id"),
            serde_json::Value::String(client_submission_id.to_string()),
        );
        object.insert(
            String::from("client_submission_target"),
            serde_json::Value::String(submission_target.to_string()),
        );
        object.insert(
            String::from("client_submission_accepted_at_ms"),
            serde_json::Value::String(accepted_at_ms.to_string()),
        );
        Ok(())
    };
    let visible_message = workspace_snapshot
        .session_messages
        .get_mut(&session_id)
        .and_then(|messages| messages.iter_mut().find(|message| message.id == message_id))
        .ok_or_else(|| format!("Visible user record `{message_id}` was not projected."))?;
    insert_identifier(&mut visible_message.metadata)?;
    let runtime_record = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .and_then(|runtime_state| {
            runtime_state
                .conversation_state
                .records
                .iter_mut()
                .find(|record| record.id == message_id)
        })
        .ok_or_else(|| format!("Canonical user record `{message_id}` was not retained."))?;
    insert_identifier(&mut runtime_record.metadata)
}

/// Submits a WebUI user prompt into a new or existing conversation.
/// 将 WebUI 用户提示词提交到新会话或已有会话。
///
/// # Parameters
/// 参数
/// `app` emits workbench updates during submission preparation and precheck dispatch.
/// `app` 在提交准备与 precheck 分发期间发送工作台更新。
/// `state` owns service runtime handles and the retained workspace snapshot.
/// `state` 持有服务运行时句柄与保留工作区快照。
/// `project_id` selects the target project when creating a new conversation.
/// `project_id` 在创建新会话时选择目标项目。
/// `conversation_id` selects an existing conversation when present.
/// `conversation_id` 在存在时选择已有会话。
/// `prompt` is the trimmed user prompt body.
/// `prompt` 是裁剪后的用户提示词正文。
/// `hidden_model_guidance` optionally adds hidden runtime guidance.
/// `hidden_model_guidance` 可选追加隐藏运行时引导。
/// `execution_mode` chooses build or plan execution mode.
/// `execution_mode` 选择 build 或 plan 执行模式。
/// `reasoning_effort` optionally overrides model reasoning effort for this turn.
/// `reasoning_effort` 可选覆盖本回合模型推理强度。
/// `model_selection` freezes the exact provider, model, and credential shown at submission time.
/// `model_selection` 固定提交时界面显示的精确供应商、模型与凭据。
/// `attachments` contains composer attachment summaries to stage into the turn.
/// `attachments` 包含要加入本回合的输入区附件摘要。
/// `preauthorized_turn_admission` carries exact ownership from an outer destructive-edit transaction.
/// `preauthorized_turn_admission` 携带外层破坏性编辑事务的精确所有权。
/// Continues one terminated conversation from retained canonical provider state without appending user input.
/// 从保留的规范 Provider 状态继续已终止会话，且不追加用户输入。
///
/// # Parameters
/// 参数
/// `app` emits the accepted continuation state and subsequent runtime deltas.
/// `app` 发出已接受的 Continue 状态与后续运行时增量。
/// `state` owns the retained workspace and canonical conversation state.
/// `state` 持有保留的工作区与规范会话状态。
/// `conversation_id` identifies the terminated WebUI conversation.
/// `conversation_id` 标识已终止的 WebUI 会话。
/// `expected_generation` is the exact continuation generation projected to the WebUI.
/// `expected_generation` 是投影给 WebUI 的精确 Continue 代际。
/// `expected_ledger_revision` is the exact provider-call ledger revision projected to the WebUI.
/// `expected_ledger_revision` 是投影给 WebUI 的精确 Provider 调用账本修订号。
///
/// # Returns
/// 返回值
/// A lightweight acknowledgement after the retained provider state is admitted for continuation.
/// 保留的 Provider 状态获准继续后的轻量确认信息。
pub(super) async fn service_continue_conversation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    expected_generation: u64,
    expected_ledger_revision: u64,
) -> Result<SubmitConversationPromptAck, String> {
    let session_id = parse_session_id(&conversation_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION"))?;
    // retryReference is read with eligibility so Continue cannot race a changed runtime snapshot.
    // retryReference 与资格信息在同一锁内读取，避免 Continue 与变化后的运行态快照竞争。
    let (controller_runtime, retry_reference) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        let runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
        let run_phase = workspace
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle);
        let blockers = session_continuation_runtime_blockers(&guard, workspace, session_id)?;
        resolve_session_continuation_eligibility(
            runtime_state,
            run_phase,
            blockers,
            Some((expected_generation, expected_ledger_revision)),
        )
        .map_err(String::from)?;
        let retry_reference = runtime_state
            .execution_retry_snapshot
            .clone()
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
        if retry_reference.state != vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen {
            return Err(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN",
            ));
        }
        let controller_runtime = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?;
        (controller_runtime, retry_reference)
    };
    // retrySnapshot is the exact sidecar payload; missing, damaged, or unknown versions fail closed.
    // retrySnapshot 是精确 sidecar 载荷；缺失、损坏或未知版本均失败关闭。
    let retry_snapshot = vulcan_workspace_store::workspace_store::load_execution_retry_snapshot(
        &controller_runtime.data_root,
        session_id,
        &retry_reference,
    )?;

    service_retry_frozen_execution(
        app,
        state,
        conversation_id,
        expected_generation,
        expected_ledger_revision,
        retry_snapshot,
    )
    .await
}

/// Starts a claimed frozen execution without creating or accepting prompt text.
/// 在不创建或接收提示词文本的情况下启动一份已领取冻结执行。
async fn service_retry_frozen_execution(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    expected_generation: u64,
    expected_ledger_revision: u64,
    retry_snapshot: vulcan_workbench_model::ExecutionRetrySnapshot,
) -> Result<SubmitConversationPromptAck, String> {
    service_start_conversation_execution(
        app,
        state,
        Uuid::new_v4().to_string(),
        None,
        Some(conversation_id),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
        ConversationSubmissionMode::Continuation {
            expected_generation,
            expected_ledger_revision,
            retry_snapshot: Box::new(retry_snapshot),
        },
    )
    .await
}

/// Starts an ordinary user-authored conversation submission through the shared execution admission path.
/// 通过共享执行准入路径启动普通用户编写的会话提交。
#[allow(clippy::too_many_arguments)]
pub(super) async fn service_submit_conversation_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    client_submission_id: String,
    project_id: Option<String>,
    conversation_id: Option<String>,
    prompt: String,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    model_selection: Option<SubmissionModelSelection>,
    session_settings: Option<SubmissionSessionSettings>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    preauthorized_turn_admission: Option<ExistingSubmissionAdmissionAuthorization>,
    existing_session_hydration_policy: ExistingSessionHydrationPolicy,
    submission_mode: ConversationSubmissionMode,
) -> Result<SubmitConversationPromptAck, String> {
    service_start_conversation_execution(
        app,
        state,
        client_submission_id,
        project_id,
        conversation_id,
        Some(prompt),
        hidden_model_guidance,
        execution_mode,
        reasoning_effort,
        model_selection,
        session_settings,
        attachments,
        preauthorized_turn_admission,
        existing_session_hydration_policy,
        submission_mode,
    )
    .await
}

/// Submits one ordinary user prompt or one prompt-free continuation through the shared conversation pipeline.
/// 通过共享会话流水线提交普通用户提示词或无提示词 Continue 请求。
///
/// # Parameters
/// `app` emits accepted workbench updates and runtime events.
/// `app` 发出已接受的工作台更新与运行时事件。
/// `state` owns service handles and the retained workspace snapshot.
/// `state` 持有服务句柄与保留的工作区快照。
/// The remaining arguments preserve the existing prompt, session, model, attachment, admission, hydration, and submission-mode contracts.
/// 其余参数继续承载现有的提示词、会话、模型、附件、准入、恢复策略与提交模式契约。
///
/// # Returns
/// A lightweight acknowledgement after the request is queued or started.
/// 请求排队或启动后的轻量确认信息。
///
/// `existing_session_hydration_policy` controls whether an empty retained session is rehydrated.
/// `existing_session_hydration_policy` 控制是否重新水合空的保留会话。
///
/// # Returns
/// 返回值
/// Returns a lightweight acknowledgement after queuing or starting the submitted prompt.
/// 返回排队或启动提交提示词后的轻量确认信息。
///
/// `submission_mode` selects ordinary user input or a revision-bound prompt-free continuation.
/// `submission_mode` 选择普通用户输入或绑定修订号的无提示词 Continue。
async fn service_start_conversation_execution(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    client_submission_id: String,
    project_id: Option<String>,
    conversation_id: Option<String>,
    prompt: Option<String>,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    model_selection: Option<SubmissionModelSelection>,
    session_settings: Option<SubmissionSessionSettings>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    preauthorized_turn_admission: Option<ExistingSubmissionAdmissionAuthorization>,
    existing_session_hydration_policy: ExistingSessionHydrationPolicy,
    submission_mode: ConversationSubmissionMode,
) -> Result<SubmitConversationPromptAck, String> {
    // Measures the whole submit RPC body from service entry to final response.
    // 衡量提交 RPC 主体从服务入口到最终返回的整体耗时。
    let submit_total_started_at = current_unix_millis_value();
    let client_submission_id = client_submission_id.trim().to_string();
    if client_submission_id.is_empty() {
        return Err(String::from("clientSubmissionId must not be empty."));
    }
    let prompt = match (&submission_mode, prompt) {
        (ConversationSubmissionMode::UserPrompt, Some(prompt)) => prompt.trim().to_string(),
        (ConversationSubmissionMode::UserPrompt, None) => {
            return Err(String::from("A user submission requires prompt text."));
        }
        (ConversationSubmissionMode::Continuation { .. }, None) => String::new(),
        (ConversationSubmissionMode::Continuation { .. }, Some(_)) => {
            return Err(String::from(
                "An execution retry must not carry prompt text.",
            ));
        }
    };
    // claimedRetrySnapshot carries the exact sidecar through admission into deferred runtime preparation.
    // claimedRetrySnapshot 将精确 sidecar 从准入阶段传递到延迟运行时准备阶段。
    let mut claimed_retry_snapshot = submission_mode.retry_snapshot().cloned();
    let prompt_source = submission_mode.prompt_source();
    let append_user_records = submission_mode.appends_user_records();
    webui_submit_trace(
        "submit_prompt_received",
        format!(
            "conversation_id={} project_id={} prompt_chars={} attachments={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            project_id.as_deref().unwrap_or("<none>"),
            prompt.chars().count(),
            attachments.as_ref().map(Vec::len).unwrap_or(0)
        ),
    );
    let attachments = attachments
        .unwrap_or_default()
        .into_iter()
        .map(composer_attachment_summary_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    if prompt.is_empty() && attachments.is_empty() && !submission_mode.is_continuation() {
        return Err(String::from("请输入任务指令。"));
    }
    if submission_mode.is_continuation() && !attachments.is_empty() {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_ATTACHMENTS_NOT_ALLOWED",
        ));
    }

    // Measures runtime handle acquisition before workspace hydration begins.
    // 衡量工作区水合开始前获取运行时句柄的耗时。
    let runtime_parts_started_at = current_unix_millis_value();
    let (controller_runtime, agent_runtime, _luaskills_catalog, _mcp_catalog, _logs) =
        runtime_parts_from_state(&state)?;
    webui_submit_trace(
        "submit_prompt_runtime_parts_ready",
        format!(
            "conversation_id={} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(runtime_parts_started_at)
        ),
    );
    // submission_target fixes create-or-continue intent before admission, loading, or hydration starts.
    // submission_target 在准入、加载或水合开始前固定新建或继续意图。
    let submission_target = parse_prompt_session_target(conversation_id.as_deref())?;
    // submission_session_id is the existing identity used by retained-state and hydration lookups.
    // submission_session_id 是保留状态与水合查找使用的已有会话身份。
    let submission_session_id = submission_target.existing_session_id();
    // Measures retained workspace reuse without cloning the full workbench snapshot on the hot path.
    // 衡量保留工作区复用，并避免在热路径克隆完整工作台快照。
    let retained_snapshot_started_at = current_unix_millis_value();
    // RetainedSubmissionPreflight keeps admission ownership aligned with the captured workspace revision.
    // RetainedSubmissionPreflight 让准入所有权与捕获的工作区版本保持一致。
    let RetainedSubmissionPreflight {
        turn_admission,
        project_admission,
        preserved_workspace_run_state,
        retained_dialogue_context_available,
        retained_workspace_snapshot,
    } = prepare_retained_submission_preflight(
        &state,
        project_id.as_deref(),
        submission_target,
        preauthorized_turn_admission,
    )?;
    webui_trace_if_slow(
        "submit_prompt_retained_snapshot_slow",
        retained_snapshot_started_at,
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} preserved_sessions={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            preserved_workspace_run_state
                .as_ref()
                .map(|state| state.session_count())
                .unwrap_or_default()
        ),
    );
    let workspace_load_started_at = current_unix_millis_value();
    let (mut workspace_snapshot, workspace_source) =
        if let Some(workspace_snapshot) = retained_workspace_snapshot {
            (
                workspace_snapshot,
                if submission_session_id.is_some() {
                    "retained-existing"
                } else {
                    "retained-new-slice"
                },
            )
        } else if submission_session_id.is_none() {
            return Err(String::from(
                "工作区仍在初始化，新会话不会回退加载历史工作区；请稍后重试。",
            ));
        } else {
            (
                load_workspace_for_submission(controller_runtime.clone(), submission_session_id)
                    .await?,
                "disk",
            )
        };
    if existing_submission_requires_hydration(
        submission_session_id,
        existing_session_hydration_policy,
    ) && let Some(session_id) = submission_session_id
    {
        hydrate_retained_submission_session_if_needed(
            controller_runtime.clone(),
            &mut workspace_snapshot,
            session_id,
        )
        .await?;
    }
    webui_submit_trace(
        "submit_prompt_workspace_ready",
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} source={workspace_source} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(workspace_load_started_at)
        ),
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut workspace_snapshot,
    );
    if let ExistingSubmissionQueueDecision::Queue { session_id } =
        evaluate_existing_submission_queue(&state, &workspace_snapshot, submission_session_id)?
    {
        if submission_mode.is_continuation() {
            return Err(String::from("ERR_CONVERSATION_CONTINUE_SESSION_BUSY"));
        }
        return queue_existing_submission(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            ExistingSubmissionQueueInput {
                client_submission_id,
                workspace_snapshot,
                session_id,
                prompt,
                hidden_model_guidance,
                attachments,
                reasoning_effort,
                model_selection,
                submit_total_started_at,
            },
        );
    }
    // Media preparation starts only after queue admission so a queued prompt uploads at its real dispatch boundary.
    // 媒体准备仅在队列准入判断后开始，确保排队提示词在实际分发边界上传媒体。
    // submissionMediaRuntimeState carries the exact persisted lease identity from the loaded
    // workspace snapshot without re-reading mutable service state during upload.
    // submissionMediaRuntimeState 从已加载工作区快照携带精确持久化租约身份，上传期间不再读取
    // 可变服务状态。
    let submission_media_runtime_state = submission_session_id
        .and_then(|session_id| workspace_snapshot.session_runtime_state.get(&session_id));
    prepare_submission_media(
        &app,
        &state,
        &agent_runtime,
        client_submission_id.as_str(),
        submission_session_id,
        submission_media_runtime_state,
        model_selection.as_ref(),
        attachments.as_slice(),
    )
    .await?;
    let materialize_started_at = current_unix_millis_value();
    // materialization_project_id pins a new session to the exact project selected under preflight ownership.
    // materialization_project_id 将新会话固定到预检所有权下选择的精确项目。
    let materialization_project_id = if submission_session_id.is_none() {
        project_admission
            .exact_project_id()
            .or(project_id.as_deref())
    } else {
        project_id.as_deref()
    };
    let materialized_session = materialize_prompt_session(
        &mut workspace_snapshot,
        materialization_project_id,
        submission_target,
        if prompt.is_empty() {
            "附件会话"
        } else {
            &prompt
        },
    )?;
    project_admission.validate_materialized_session(&materialized_session)?;
    webui_submit_trace(
        "submit_prompt_materialize_ready",
        format!(
            "session={} title_chars={} workspace_path_chars={} elapsed_ms={}",
            materialized_session.session_id,
            materialized_session.session_title.chars().count(),
            materialized_session.workspace_path.chars().count(),
            current_unix_millis_value().saturating_sub(materialize_started_at)
        ),
    );
    // kernel_reservation_id binds an existing materialization to the exact admission owner captured before loading.
    // kernel_reservation_id 将已有会话物化结果绑定到加载前捕获的精确准入所有者。
    let kernel_reservation_id = turn_admission.kernel_reservation_id(&materialized_session)?;
    prepare_materialized_submission_session(&mut workspace_snapshot, &materialized_session)?;
    apply_submission_model_selection(
        agent_runtime.public_ai_runtime.as_ref(),
        &mut workspace_snapshot,
        materialized_session.session_id,
        model_selection.as_ref(),
    )?;
    if session_settings.is_some()
        && materialized_session.kind != PromptSessionMaterializationKind::Created
    {
        return Err(String::from("草稿会话设置只能随新会话的首次提交应用。"));
    }
    apply_submission_session_settings(
        &state,
        &controller_runtime,
        agent_runtime.public_ai_runtime.as_ref(),
        &mut workspace_snapshot,
        materialized_session.session_id,
        execution_mode.as_deref(),
        session_settings.as_ref(),
        materialized_session.kind == PromptSessionMaterializationKind::Created,
    )
    .await?;
    // session_id is copied from the authoritative materialization result after activation consumes its classification.
    // session_id 在激活阶段消费物化分类后，从权威物化结果中复制。
    let session_id = materialized_session.session_id;
    if matches!(submission_mode, ConversationSubmissionMode::UserPrompt)
        && let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
    {
        let _ = runtime_state
            .provider_call_ledger
            .invalidate_continuation_for_submission(
                client_submission_id.clone(),
                i64::try_from(current_unix_millis_value()).unwrap_or(i64::MAX),
            );
    }
    if matches!(submission_mode, ConversationSubmissionMode::UserPrompt) {
        invalidate_execution_retry_snapshot_for_user_prompt(
            &controller_runtime,
            &mut workspace_snapshot,
            session_id,
        )?;
    }
    // session_title borrows the authoritative title through the final activation commit.
    // session_title 借用权威标题直到最终激活提交完成。
    let session_title = materialized_session.session_title.as_str();
    // workspace_path borrows the owning project path through precheck dispatch preparation.
    // workspace_path 借用所属项目路径直到 precheck 分发准备完成。
    let workspace_path = materialized_session.workspace_path.as_str();
    webui_submit_trace(
        "submit_prompt_starting_run",
        format!(
            "session={session_id} title_chars={}",
            session_title.chars().count()
        ),
    );
    let submit_run_prepare_started_at = current_unix_millis_value();
    let runtime_options = prepare_submission_runtime_options(
        &controller_runtime,
        &mut workspace_snapshot,
        session_id,
        SubmissionRuntimeOptionsInput {
            execution_mode,
            reasoning_effort,
        },
    )
    .await?;
    webui_submit_trace(
        "submit_prompt_runtime_options_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(submit_run_prepare_started_at)
        ),
    );
    let turn_begin_started_at = current_unix_millis_value();
    let resume_preparation_input =
        claimed_retry_snapshot
            .as_ref()
            .and_then(|snapshot| match &snapshot.pending_action {
                vulcan_workbench_model::ExecutionRetryPendingAction::ResumePreparation {
                    input,
                } => Some(input.as_ref().clone()),
                _ => None,
            });
    let mut precheck_start = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        if let Some(input) = resume_preparation_input {
            let restored_turn_id = input.preparation_turn_id.clone();
            let restored = restore_shared_prompt_turn_from_execution_retry(
                session_id,
                &mut hook_runtime,
                input,
            )?;
            let runtime_state = workspace_snapshot
                .session_runtime_state
                .get_mut(&session_id)
                .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
            runtime_state.active_precheck_turn_id = Some(restored_turn_id);
            restored
        } else {
            begin_shared_prompt_turn(
                &mut workspace_snapshot,
                &mut hook_runtime,
                SharedPromptTurnBeginInput {
                    session_id,
                    session_title,
                    workspace_path,
                    prompt_entry_id: format!(
                        "webui-prompt-{session_id}-{}",
                        current_unix_millis_text()
                    ),
                    prompt: &prompt,
                    hidden_model_guidance: hidden_model_guidance.as_deref(),
                    provider_visible_agent_messages: Vec::new(),
                    attachments,
                    append_canonical_user_message: append_user_records,
                    append_visible_user_prompt_record: append_user_records,
                    source: prompt_source,
                    execution_mode: runtime_options.execution_mode,
                    reasoning_effort: runtime_options.reasoning_effort,
                },
            )?
        }
    };
    let accepted_message_id = precheck_start.visible_user_prompt_record_id.clone();
    let submission_target_label = if submission_session_id.is_some() {
        "existing"
    } else {
        "new"
    };
    attach_client_submission_id_to_visible_user_record(
        &mut workspace_snapshot,
        session_id,
        accepted_message_id.as_deref(),
        &client_submission_id,
        submission_target_label,
        current_unix_millis_value(),
    )?;
    if !submission_mode.is_continuation() {
        // This synchronous boundary closes the accepted-but-not-yet-prepared Stop race.
        // 该同步边界关闭“已接纳但尚未准备”阶段的 Stop 竞态窗口。
        persist_accepted_preparation_retry_checkpoint(
            &controller_runtime,
            &mut workspace_snapshot,
            session_id,
            &client_submission_id,
            &precheck_start,
        )?;
    }
    if let Some((expected_generation, expected_ledger_revision)) =
        submission_mode.continuation_expectation()
    {
        let blockers = {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            let retained_workspace = guard
                .workspace_snapshot
                .as_ref()
                .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
            session_continuation_runtime_blockers(&guard, retained_workspace, session_id)?
        };
        let run_phase = workspace_snapshot
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle);
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
        resolve_session_continuation_eligibility(
            runtime_state,
            run_phase,
            blockers,
            Some((expected_generation, expected_ledger_revision)),
        )
        .map_err(String::from)?;
        let claimed_at_ms = i64::try_from(current_unix_millis_value())
            .map_err(|_| String::from("ERR_CONVERSATION_CONTINUE_TIMESTAMP_OUT_OF_RANGE"))?;
        runtime_state.provider_call_ledger.claim_continuation(
            expected_generation,
            expected_ledger_revision,
            client_submission_id.clone(),
            claimed_at_ms,
        )?;
        let retry_snapshot = claimed_retry_snapshot
            .as_mut()
            .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING"))?;
        validate_execution_retry_snapshot_for_claim(runtime_state, session_id, retry_snapshot)?;
        retry_snapshot.state = vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed;
        retry_snapshot.claimed_attempt_id = Some(client_submission_id.clone());
        retry_snapshot.updated_at = current_unix_millis_text();
        let retry_reference =
            vulcan_workspace_store::workspace_store::persist_execution_retry_snapshot(
                &controller_runtime.data_root,
                session_id,
                retry_snapshot,
            )?;
        runtime_state.execution_retry_snapshot = Some(retry_reference);
        precheck_start.execution_retry_snapshot = claimed_retry_snapshot.clone().map(Box::new);
        store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot.clone())?;
        if let Err(error) = persist_workspace_session_update(
            Some(&state),
            controller_runtime.clone(),
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            let cleanup_error = mark_webui_prompt_preparation_failed(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                &error,
            )
            .await
            .err();
            return Err(match cleanup_error {
                Some(cleanup_error) => format!("{error}; cleanup failed: {cleanup_error}"),
                None => error,
            });
        }
    }
    let turn_id = Some(precheck_start.preparation_turn_id.clone());
    let preparation_prompt_source = precheck_start.source;
    webui_submit_trace(
        "submit_prompt_turn_begin_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(turn_begin_started_at)
        ),
    );
    // Every accepted turn starts in Preparing; post-ACK precheck moves it to MemoryRecall only when required.
    // 每个已接纳回合都从 Preparing 开始；仅在需要时由 ACK 后预检推进到 MemoryRecall。
    let submit_visible_run_phase = SessionRunPhase::Preparing;
    let kernel_start_result = {
        let kernel_turn_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let result = if let Some(kernel_reservation_id) = kernel_reservation_id {
            start_reserved_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
                kernel_reservation_id,
            )
        } else {
            start_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
            )
        };
        webui_submit_trace(
            "submit_prompt_kernel_turn_started",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(kernel_turn_started_at)
            ),
        );
        result
    };
    if let Err(error) = kernel_start_result {
        let cleanup_error = mark_webui_prompt_preparation_failed(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            &error,
        )
        .await
        .err();
        return Err(match cleanup_error {
            Some(cleanup_error) => format!("{error}; cleanup failed: {cleanup_error}"),
            None => error,
        });
    }
    mark_webui_prompt_snapshot_preparing(&mut workspace_snapshot, session_id);
    let begin_commit_started_at = current_unix_millis_value();
    if materialized_session.kind == PromptSessionMaterializationKind::Created {
        commit_created_submission_workspace_slice(
            &state,
            &workspace_snapshot,
            &materialized_session,
        )?;
    } else {
        store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot.clone())?;
    }
    webui_submit_trace(
        "submit_prompt_begin_workspace_committed",
        format!(
            "session={session_id} elapsed_ms={} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(begin_commit_started_at),
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    commit_materialized_submission_activation(
        &app,
        &state,
        &workspace_snapshot,
        &materialized_session,
    )?;
    if let Some(message_id) = precheck_start.visible_user_prompt_record_id.as_deref()
        && let Some(message) = workspace_snapshot
            .session_messages
            .get(&session_id)
            .and_then(|messages| messages.iter().find(|message| message.id == message_id))
    {
        // delta_started_at measures only the post-commit visible projection and event emission.
        // delta_started_at 仅衡量提交后的可见投影与事件发送。
        let delta_started_at = current_unix_millis_value();
        emit_materialized_session_message_delta(
            &app,
            &state,
            session_id,
            message,
            submit_visible_run_phase,
        )?;
        webui_submit_trace(
            "submit_prompt_user_message_delta_ready",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms={} elapsed_ms_since_received={}",
                current_unix_millis_value().saturating_sub(delta_started_at),
                current_unix_millis_value().saturating_sub(submit_total_started_at)
            ),
        );
        tokio::task::yield_now().await;
        webui_submit_trace(
            "submit_prompt_user_message_delta_yielded",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms_since_received={}",
                current_unix_millis_value().saturating_sub(submit_total_started_at)
            ),
        );
    }
    webui_submit_trace(
        "submit_prompt_visible_user_message_emitted",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    // A continuation claim is a synchronous durable admission boundary; ordinary user submissions retain the bounded asynchronous ACK path.
    // Continue 领取是同步持久化准入边界；普通用户提交继续使用有界异步 ACK 路径。
    if submission_mode.is_continuation() {
        if let Err(persist_error) = persist_workspace_session_update(
            Some(&state),
            controller_runtime.clone(),
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            let cleanup_error = mark_webui_prompt_preparation_failed(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                &persist_error,
            )
            .await
            .err();
            return Err(cleanup_error.map_or_else(
                || format!("Continue 领取持久化失败：{persist_error}"),
                |cleanup_error| {
                    format!(
                        "Continue 领取持久化失败：{persist_error}；失败清理也未完成：{cleanup_error}"
                    )
                },
            ));
        }
    } else {
        spawn_workspace_session_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot.clone(),
            session_id,
            Some(String::from("submit_acceptance")),
        );
    }
    let revision = current_workbench_revision_from_state(&state)?;
    let acknowledgement_elapsed_ms =
        current_unix_millis_value().saturating_sub(submit_total_started_at);
    retain_slow_submission_diagnostic(
        &state,
        &client_submission_id,
        submission_target_label,
        "acceptance",
        acknowledgement_elapsed_ms,
    )?;
    webui_submit_trace(
        "submit_prompt_returning",
        format!(
            "session={session_id} client_submission_id={client_submission_id} elapsed_ms_since_received={acknowledgement_elapsed_ms}"
        ),
    );
    let accepted_client_submission_id = client_submission_id.clone();
    let acknowledgement = SubmitConversationPromptAck {
        client_submission_id,
        conversation_id: session_id.to_string(),
        accepted_message_id,
        turn_id,
        run_phase: submit_visible_run_phase.to_string(),
        queued: false,
        revision,
    };
    // Deferred preparation begins only after the visible message and acknowledgement facts are committed.
    // 延迟准备仅在可见消息与确认事实提交后开始。
    let preparation_turn_id = precheck_start.preparation_turn_id.clone();
    let session_title = session_title.to_string();
    let workspace_path = workspace_path.to_string();
    let app_for_preparation = app.clone();
    let state_for_preparation = state.clone();
    let controller_runtime_for_preparation = controller_runtime.clone();
    let agent_runtime_for_preparation = agent_runtime.clone();
    spawn_service_task(async move {
        let preparation_started_at = current_unix_millis_value();
        let preparation_result = (|| -> Result<(), String> {
            // workspace_snapshot is cloned only after acknowledgement, so new-session acceptance never copies history.
            // workspace_snapshot 只在确认后克隆，因此新会话接纳绝不会复制历史数据。
            let mut workspace_snapshot = current_workspace_snapshot_from_shared_state(
                &state_for_preparation,
            )?
            .ok_or_else(|| String::from("accepted submission workspace snapshot is unavailable"))?;
            let context_snapshot_started_at = current_unix_millis_value();
            snapshot_webui_context_before_user_prompt(
                &controller_runtime_for_preparation,
                &workspace_snapshot,
                session_id,
                preparation_prompt_source,
            )?;
            webui_submit_trace(
                "submit_prompt_context_snapshot_ready",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(context_snapshot_started_at)
                ),
            );
            let precheck_plan_started_at = current_unix_millis_value();
            let precheck_plan = if preparation_prompt_source.runs_user_prompt_precheck() {
                webui_shared_prompt_precheck_plan(
                    &agent_runtime_for_preparation,
                    &workspace_snapshot,
                    session_id,
                    &workspace_path,
                )?
            } else {
                SharedPromptPrecheckPlan {
                    has_precheck_hooks: false,
                    should_skip_precheck: true,
                    progress_labels: Vec::new(),
                }
            };
            webui_submit_trace(
                "submit_prompt_precheck_plan_ready",
                format!(
                    "session={session_id} has_hooks={} skip={} labels={} elapsed_ms={}",
                    precheck_plan.has_precheck_hooks,
                    precheck_plan.should_skip_precheck,
                    precheck_plan.progress_labels.len(),
                    current_unix_millis_value().saturating_sub(precheck_plan_started_at)
                ),
            );
            let outer_steps_started_at = current_unix_millis_value();
            apply_webui_prompt_begin_outer_steps_deferred_context_persist(
                &controller_runtime_for_preparation,
                &mut workspace_snapshot,
                session_id,
                &session_title,
            );
            webui_submit_trace(
                "submit_prompt_outer_steps_ready",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(outer_steps_started_at)
                ),
            );
            let hook_snapshot_started_at = current_unix_millis_value();
            agent_runtime_for_preparation
                .vmm_hook_runtime
                .replace_snapshot(build_vmm_hook_config_snapshot(
                    &agent_runtime_for_preparation.provider_configuration,
                    &workspace_snapshot,
                ))?;
            webui_submit_trace(
                "submit_prompt_hook_snapshot_ready",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
                ),
            );
            store_workspace_snapshot_without_workbench_rebuild(
                &state_for_preparation,
                workspace_snapshot,
            )?;
            let snapshot = current_workspace_snapshot_from_shared_state(&state_for_preparation)?
                .ok_or_else(|| {
                    String::from("accepted submission workspace snapshot is unavailable")
                })?;
            spawn_workspace_session_persist_task(
                state_for_preparation.clone(),
                controller_runtime_for_preparation.clone(),
                snapshot,
                session_id,
                Some(String::from("submit_begin")),
            );
            let precheck_started = start_webui_precheck_or_continue(
                &app_for_preparation,
                &state_for_preparation,
                &controller_runtime_for_preparation,
                &agent_runtime_for_preparation,
                precheck_start,
                precheck_plan,
            )?;
            webui_submit_trace(
                "submit_prompt_precheck_dispatch_returned",
                format!("session={session_id} started={precheck_started}"),
            );
            Ok(())
        })();
        let preparation_elapsed_ms =
            current_unix_millis_value().saturating_sub(preparation_started_at);
        if let Err(error) = retain_slow_submission_diagnostic(
            &state_for_preparation,
            &accepted_client_submission_id,
            submission_target_label,
            "post_ack_preparation",
            preparation_elapsed_ms,
        ) {
            eprintln!("[webui-service] failed to retain slow submission diagnostic: {error}");
        }
        if let Err(error) = preparation_result {
            eprintln!("[webui-service] accepted submission preparation failed: {error}");
            if let Err(cleanup_error) = mark_webui_prompt_preparation_failed(
                &app_for_preparation,
                &state_for_preparation,
                &controller_runtime_for_preparation,
                &agent_runtime_for_preparation,
                session_id,
                &preparation_turn_id,
                &error,
            )
            .await
            {
                eprintln!(
                    "[webui-service] accepted submission preparation cleanup failed: {cleanup_error}"
                );
            }
        }
    });
    Ok(acknowledgement)
}

/// Reads the latest accepted workbench revision from shared service state.
/// 从共享服务状态读取最新已接受的工作台版本号。
fn current_workbench_revision_from_state(state: &SharedServiceBridgeState) -> Result<u64, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workbench_generation)
}

/// Emits the latest queued-prompt projection as an ordered workbench delta.
/// 将最新排队提示词投影作为有序工作台增量发出。
///
/// # Parameters
/// # 参数
/// - `app`: Event emitter that publishes the ordered projection.
/// - `app`：发布有序投影的事件发送器。
/// - `state`: Shared service state containing the authoritative workspace and revision.
/// - `state`：包含权威工作区与版本号的共享服务状态。
/// - `session_id`: Conversation whose current run phase must be projected.
/// - `session_id`：需要投影当前运行阶段的会话。
///
/// # Returns
/// # 返回值
/// The committed workbench revision and authoritative current run phase, or an exact state error.
/// 已提交的工作台版本与权威当前运行阶段，或准确的状态错误。
pub(crate) fn emit_queued_prompt_run_phase_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: Uuid,
) -> Result<(u64, SessionRunPhase), String> {
    let (event, run_phase) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // workspace is reread under the generation lock so later turn advancement always wins.
        // workspace 在版本锁内重新读取，使之后发生的回合推进始终胜出。
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("cannot project queued prompt without a retained workspace snapshot")
        })?;
        // run_phase is the current authoritative phase, never the value captured by queue commit.
        // run_phase 是当前权威阶段，绝不使用排队提交时捕获的旧值。
        let run_phase = workspace
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let revision = guard.workbench_generation;
        guard.workbench.revision = revision;
        let conversation_id = session_id.to_string();
        let run_phase_text = run_phase.to_string();
        let mut status = None;
        let mut last_updated_label = None;
        if let Some(conversation) = guard
            .workbench
            .conversations
            .iter_mut()
            .find(|candidate| candidate.id == conversation_id)
        {
            conversation.run_phase = run_phase_text.clone();
            status = Some(conversation.status.clone());
            last_updated_label = Some(conversation.last_updated_label.clone());
        }
        (
            WorkbenchDeltaEnvelope {
                generation: revision,
                delta: WorkbenchDelta::UpdateConversationRunPhase(
                    UpdateConversationRunPhaseDelta {
                        conversation_id,
                        run_phase: run_phase_text,
                        status,
                        last_updated_label,
                    },
                ),
            },
            run_phase,
        )
    };
    let generation = event.generation;
    emit_service_event_with_trace(
        app,
        "workbench_queued_prompt_run_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok((generation, run_phase))
}

#[cfg(test)]
mod submission_hydration_policy_tests {
    use super::*;

    /// Verifies a first-prompt edit treats its intentionally empty retained history as authoritative.
    /// 验证编辑第一条提示词时会将有意清空的保留历史视为权威状态。
    #[test]
    fn first_prompt_edit_does_not_rehydrate_the_removed_original_prompt() {
        let session_id = SessionId::from_u128(901);

        assert!(!existing_submission_requires_hydration(
            Some(session_id),
            ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
        ));
        assert!(existing_submission_requires_hydration(
            Some(session_id),
            ExistingSessionHydrationPolicy::HydrateIfEmpty,
        ));
    }

    /// Verifies new-session submissions never enter the existing-session hydration path.
    /// 验证新会话提交永远不会进入已有会话水合路径。
    #[test]
    fn new_session_submission_never_requires_existing_session_hydration() {
        assert!(!existing_submission_requires_hydration(
            None,
            ExistingSessionHydrationPolicy::HydrateIfEmpty,
        ));
    }
}

#[cfg(test)]
mod submission_session_settings_tests {
    use super::{
        SubmissionSessionSettings, should_preserve_existing_provider_tool_settings,
        submission_output_style_id, submission_output_style_snapshot,
    };
    use vulcan_agent_runtime::provider_config::PersistedPromptOutputStyleSelection;

    /// Verifies only an explicitly overridden session is preserved when a submission omits settings.
    /// 验证提交缺少设置时仅保留显式覆盖的会话。
    #[test]
    fn omitted_settings_preserve_explicit_session_only() {
        assert!(should_preserve_existing_provider_tool_settings(
            false, true, true
        ));
        assert!(!should_preserve_existing_provider_tool_settings(
            true, true, true
        ));
        assert!(!should_preserve_existing_provider_tool_settings(
            false, true, false
        ));
        assert!(!should_preserve_existing_provider_tool_settings(
            false, false, false
        ));
    }

    /// Verifies an ordinary existing-session submission cannot activate a newly selected style.
    /// 验证已有会话的普通提交不能激活新选择的输出风格。
    #[test]
    fn ordinary_submission_keeps_output_style_snapshot_unchanged() {
        let settings = SubmissionSessionSettings {
            output_style_id: Some(String::from("rigorous")),
            vmm_policy: None,
            provider_tool_settings: None,
            knowledge_selection: None,
            initial_goal: None,
        };
        assert!(
            submission_output_style_snapshot(
                Some(&settings),
                PersistedPromptOutputStyleSelection::Balanced,
                false,
            )
            .expect("ordinary submission should be accepted")
            .is_none()
        );
        assert_eq!(
            submission_output_style_snapshot(
                Some(&settings),
                PersistedPromptOutputStyleSelection::Balanced,
                true,
            )
            .expect("new-session boundary should resolve the style")
            .expect("new-session boundary should capture a style")
            .id,
            "rigorous"
        );
        assert_eq!(
            submission_output_style_id(Some(&settings)).as_deref(),
            Some("rigorous")
        );
    }
}
