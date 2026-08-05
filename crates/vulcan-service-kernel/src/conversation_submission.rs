//! WebUI conversation prompt submission orchestration.
//! WebUI 会话提示词提交编排逻辑。

use std::collections::BTreeMap;

use crate::{apply_output_style_snapshot, resolve_output_style_snapshot};
use uuid::Uuid;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_context::SessionConversationPayload;
use vulcan_service_api::{
    ComposerAttachmentSummary, ServiceEvent, ServiceEventEmitter, SubmitConversationPromptAck,
    UpdateConversationRunPhaseDelta, WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_session_boundary::SessionBoundaryStateStore;
use vulcan_session_core::SessionNodeId;
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionKnowledgeLibrarySelection,
    SessionKnowledgeLibrarySelectionMode, SessionMessageKind, SessionMessageRecordKind,
    SessionProviderToolTargetSelection, SessionRunPhase, SessionState,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle,
    workspace_store::{WorkspaceSegmentedSessionBoundaryStore, WorkspaceSnapshot},
};

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
    is_active_session_run_phase, load_workspace_for_submission,
    mark_webui_prompt_preparation_failed, mark_webui_prompt_snapshot_preparing,
    materialize_prompt_session, owning_project_id_for_session, parse_prompt_session_target,
    parse_session_id, persist_initial_submission_goal, prepare_submission_media,
    preserve_active_session_run_state, provider_tool_capability_key,
    replace_conversation_messages_in_workbench_window, retain_slow_submission_diagnostic,
    retained_submission_snapshot_has_session_context, retained_submission_snapshot_is_usable,
    rollback_persisted_session_before_user_prompt, runtime_parts_from_state,
    session_has_blocking_system_prompt_work, snapshot_webui_context_before_user_prompt,
    spawn_service_task, spawn_workspace_session_persist_task,
    start_reserved_webui_session_kernel_user_turn, start_webui_precheck_or_continue,
    start_webui_session_kernel_user_turn, store_workspace_snapshot_without_workbench_rebuild,
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

/// Selects whether a submission creates visible user input or replays retained provider state.
/// 选择本次提交是创建可见用户输入还是重放保留的 Provider 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConversationSubmissionMode {
    /// Normal composer submission authored by the user.
    /// 用户通过输入区发起的普通提交。
    UserPrompt,
    /// Host-owned retry that must not append another canonical or visible user message.
    /// 宿主拥有的重试，不追加新的规范用户消息或可见用户记录。
    ProviderRetry,
}

impl ConversationSubmissionMode {
    /// Returns the prompt source recorded in the shared turn state.
    /// 返回共享回合状态中记录的提示词来源。
    fn prompt_source(self) -> PendingSessionPromptSource {
        match self {
            Self::UserPrompt => PendingSessionPromptSource::UserFollowup,
            Self::ProviderRetry => PendingSessionPromptSource::ProviderRetry,
        }
    }

    /// Returns whether this mode should append user-authored conversation records.
    /// 返回本模式是否应追加用户编写的会话记录。
    fn appends_user_records(self) -> bool {
        matches!(self, Self::UserPrompt)
    }
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
    if !configuration.model_is_enabled(selection.provider_id.as_str(), selection.model_id.as_str())
    {
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
            verified_protocols: BTreeSet::from([String::from("openai-chat-completions.v1")]),
            selected_protocol: String::from("openai-chat-completions.v1"),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        configuration
            .connection_graph
            .add_connection(runtime.providers(), connection)
            .expect("connection should register");
        configuration.set_model_enabled("openrouter", "vendor/previous", true);
        configuration.set_model_enabled("openrouter", "vendor/submitted", true);
        configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("vendor/previous"),
            connection_id: String::from("openrouter-account"),
        });
        runtime
            .replace_catalog(
                parse_openrouter_model_catalog(
                    "openrouter-account",
                    1,
                    r#"{"data":[{"id":"vendor/previous","name":"Previous","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},{"id":"vendor/submitted","name":"Submitted","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
                )
                .expect("catalog should parse"),
            )
            .expect("catalog should install");
        runtime
            .replace_configuration(configuration)
            .expect("configuration should install");
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
            model_id: String::from("vendor/submitted"),
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
            Some("vendor/submitted")
        );
        assert_eq!(
            runtime
                .configuration_snapshot()
                .expect("configuration should remain readable")
                .active
                .as_ref()
                .map(|active| active.model_id.as_str()),
            Some("vendor/previous")
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
/// Loads whether the durable session-boundary status is failed after a restart.
/// 读取进程重启后持久化会话边界状态是否为失败。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the segmented session data root.
/// `controller_runtime` 持有分段会话数据根目录。
/// `project_id` identifies the owning workspace project.
/// `project_id` 标识所属工作区项目。
/// `session_id` identifies the session boundary thread.
/// `session_id` 标识会话边界线程。
/// `session_title` supplies the store adapter's stable display title.
/// `session_title` 提供存储适配器使用的稳定显示标题。
///
/// # Returns
/// 返回值
/// Whether the persisted thread status is failed when a boundary state exists, or `None` when absent.
/// 存在边界状态时返回其是否为失败；没有边界状态的会话返回 `None`。
fn load_persisted_session_boundary_failed(
    controller_runtime: &ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
    session_title: &str,
) -> Result<Option<bool>, String> {
    let store = WorkspaceSegmentedSessionBoundaryStore::new(
        controller_runtime.data_root.clone(),
        project_id.to_string(),
        session_id,
        session_title.to_string(),
    );
    let Some(value) = store.load_state(SessionNodeId::from(session_id))? else {
        return Ok(None);
    };
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| String::from("ERR_CONVERSATION_RETRY_BOUNDARY_STATE_INVALID"))?;
    match status {
        "idle" | "running" | "interrupted" => Ok(Some(false)),
        "failed" => Ok(Some(true)),
        _ => Err(String::from(
            "ERR_CONVERSATION_RETRY_BOUNDARY_STATE_INVALID",
        )),
    }
}

/// Retries the latest failed provider turn from the retained canonical session state.
/// 从保留的规范会话状态重试最近一次失败的 Provider 回合。
///
/// # Parameters
/// 参数
/// `app` emits the accepted retry state and subsequent runtime deltas.
/// `app` 发出已接受的重试状态与后续运行时增量。
/// `state` owns the retained workspace and canonical conversation state.
/// `state` 持有保留的工作区与规范会话状态。
/// `conversation_id` identifies the failed WebUI conversation.
/// `conversation_id` 标识失败的 WebUI 会话。
///
/// # Returns
/// 返回值
/// A lightweight acknowledgement after the retained provider state is admitted for retry.
/// 保留的 Provider 状态获准重试后的轻量确认信息。
pub(super) async fn service_retry_conversation_request(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<SubmitConversationPromptAck, String> {
    let session_id = parse_session_id(&conversation_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_RETRY_INVALID_CONVERSATION"))?;
    let (prompt, run_phase, latest_visible_transcript_is_error, boundary_context) = {
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
            .ok_or_else(|| String::from("ERR_CONVERSATION_RETRY_RUNTIME_STATE_MISSING"))?;
        // The last visible transcript record is the durable failure fact after a process restart.
        // 进程重启后，最后一条可见转录记录是可持久化的失败事实。
        let latest_visible_transcript_is_error = runtime_state
            .conversation_state
            .records
            .iter()
            .rev()
            .find(|record| {
                record.visible && record.record_kind == SessionMessageRecordKind::Transcript
            })
            .is_some_and(|record| record.kind == SessionMessageKind::Error);
        let run_phase = workspace
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle);
        let boundary_context = if !is_active_session_run_phase(run_phase)
            && !matches!(
                run_phase,
                SessionRunPhase::Failed | SessionRunPhase::Stopped
            ) {
            let controller_runtime = guard
                .controller_runtime
                .clone()
                .ok_or_else(|| String::from("ERR_CONVERSATION_RETRY_CONTROLLER_NOT_READY"))?;
            let project_id = owning_project_id_for_session(workspace, session_id)
                .ok_or_else(|| String::from("ERR_CONVERSATION_RETRY_PROJECT_NOT_FOUND"))?;
            let session_title = workspace
                .sessions
                .iter()
                .find(|session| session.id == session_id)
                .map(|session| session.title.clone())
                .ok_or_else(|| String::from("ERR_CONVERSATION_RETRY_SESSION_NOT_FOUND"))?;
            Some((controller_runtime, project_id, session_title))
        } else {
            None
        };
        let prompt = runtime_state
            .conversation_state
            .messages
            .iter()
            .rev()
            .find_map(|message| match &message.payload {
                SessionConversationPayload::UserText { text, .. } => {
                    let trimmed = text.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                }
                _ => None,
            })
            .ok_or_else(|| String::from("ERR_CONVERSATION_RETRY_PROMPT_NOT_FOUND"))?;
        (
            prompt,
            run_phase,
            latest_visible_transcript_is_error,
            boundary_context,
        )
    };
    let persisted_boundary_status = boundary_context
        .as_ref()
        .map(|(controller_runtime, project_id, session_title)| {
            load_persisted_session_boundary_failed(
                controller_runtime,
                project_id,
                session_id,
                session_title,
            )
        })
        .transpose()?;
    let durable_failure_observed =
        latest_visible_transcript_is_error || persisted_boundary_status == Some(Some(true));
    // Active phases always reject retry; after restart, Idle or Ready may use the durable boundary failure.
    // 活跃阶段始终拒绝重试；进程重启后，Idle 或 Ready 可依据持久化会话边界失败状态重试。
    let phase_allows_retry = matches!(
        run_phase,
        SessionRunPhase::Failed | SessionRunPhase::Stopped
    ) || (!is_active_session_run_phase(run_phase)
        && durable_failure_observed);
    if !phase_allows_retry {
        return Err(String::from("ERR_CONVERSATION_RETRY_SESSION_NOT_FAILED"));
    }
    if !durable_failure_observed {
        return Err(String::from("ERR_CONVERSATION_RETRY_FAILURE_NOT_FOUND"));
    }

    service_submit_conversation_prompt(
        app,
        state,
        Uuid::new_v4().to_string(),
        None,
        Some(conversation_id),
        prompt,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
        ConversationSubmissionMode::ProviderRetry,
    )
    .await
}

/// Submits one ordinary user prompt or one host-owned retry through the shared conversation pipeline.
/// 通过共享会话流水线提交普通用户提示词或宿主拥有的重试请求。
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
/// `submission_mode` selects ordinary user input or a host-owned provider-state retry.
/// `submission_mode` 选择普通用户输入或宿主拥有的 Provider 状态重试。
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
    // Measures the whole submit RPC body from service entry to final response.
    // 衡量提交 RPC 主体从服务入口到最终返回的整体耗时。
    let submit_total_started_at = current_unix_millis_value();
    let client_submission_id = client_submission_id.trim().to_string();
    if client_submission_id.is_empty() {
        return Err(String::from("clientSubmissionId must not be empty."));
    }
    let prompt = prompt.trim().to_string();
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
    if prompt.is_empty() && attachments.is_empty() {
        return Err(String::from("请输入任务指令。"));
    }

    // Measures runtime handle acquisition before workspace hydration begins.
    // 衡量工作区水合开始前获取运行时句柄的耗时。
    let runtime_parts_started_at = current_unix_millis_value();
    let (
        controller_runtime,
        agent_runtime,
        _luaskills_catalog,
        _token_budget_runtime,
        _mcp_catalog,
        _logs,
    ) = runtime_parts_from_state(&state)?;
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
        if submission_mode == ConversationSubmissionMode::ProviderRetry {
            return Err(String::from("ERR_CONVERSATION_RETRY_SESSION_BUSY"));
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
    prepare_submission_media(
        &app,
        &state,
        &agent_runtime,
        client_submission_id.as_str(),
        conversation_id.as_deref(),
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
    let precheck_start = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
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
        )
    }?;
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
    let turn_id = Some(precheck_start.preparation_turn_id.clone());
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
    {
        let kernel_turn_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(kernel_reservation_id) = kernel_reservation_id {
            start_reserved_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
                kernel_reservation_id,
            )?;
        } else {
            start_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
            )?;
        }
        webui_submit_trace(
            "submit_prompt_kernel_turn_started",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(kernel_turn_started_at)
            ),
        );
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
    // Acceptance persistence is scheduled from the bounded new-session slice before ACK; it never waits for VMM or Codebase preparation.
    // 接纳持久化在 ACK 前从有界新会话切片调度；它绝不等待 VMM 或 Codebase 准备。
    spawn_workspace_session_persist_task(
        state.clone(),
        controller_runtime.clone(),
        workspace_snapshot.clone(),
        session_id,
        Some(String::from("submit_acceptance")),
    );
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
                prompt_source,
            )?;
            webui_submit_trace(
                "submit_prompt_context_snapshot_ready",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(context_snapshot_started_at)
                ),
            );
            let precheck_plan_started_at = current_unix_millis_value();
            let precheck_plan = if prompt_source.runs_user_prompt_precheck() {
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
            ) {
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
