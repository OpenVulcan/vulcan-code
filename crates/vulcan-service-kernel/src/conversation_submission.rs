//! WebUI conversation prompt submission orchestration.
//! WebUI 会话提示词提交编排逻辑。

use std::{collections::BTreeMap, sync::Arc};

use crate::{apply_output_style_snapshot, resolve_output_style_snapshot};
use uuid::Uuid;
use vulcan_agent_runtime::attachment_asset_index::promote_staged_attachment_to_message_ref;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_service_api::{
    ComposerAttachmentSummary, ServiceEvent, ServiceEventEmitter, SubmitConversationPromptAck,
    UpdateConversationRunPhaseDelta, WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_session_core::v1::config::{
    ActiveRequestConfig, KnowledgeLibrarySelectionConfig as KnowledgeLibrarySelectionConfigV1,
    KnowledgeLibrarySelectionMode as KnowledgeLibrarySelectionModeV1, VmmSessionPolicyConfig,
};
use vulcan_session_core::v1::document::{
    ContinuationTerminalProjection, SessionSelectionBundleInput,
};
use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission, ContinuationRequest};
#[cfg(test)]
use vulcan_workbench_model::SessionState;
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionKnowledgeLibrarySelection,
    SessionKnowledgeLibrarySelectionMode, SessionProviderToolTargetSelection, SessionRunPhase,
};
use vulcan_workspace_store::workspace_store::{WorkspaceSnapshot, locate_session_v1_storage};

use crate::conversation_resources_v1::{
    composer_attachment_from_project_resource_v1, validate_project_resource_attachment_reference,
};
use crate::session_runtime_registry_v1::{
    ContinuationDispatchGuard, SessionHandle, SessionRuntimeError,
};
use crate::session_v1_lifecycle::{
    CreateRootSessionV1Request, PromptAttachmentOrigin,
    create_and_register_root_session_v1_with_context_window, get_or_load_session_v1,
    register_project_prompt_attachments_v1,
};
use crate::webui_prompt_lifecycle::user_root_dispatch_authority_from_v1_handle;

use super::{
    ExistingSubmissionAdmissionAuthorization, HostRuntimeHandle, MaterializedPromptSession,
    PromptSessionMaterializationKind, SharedServiceBridgeState, SubmissionActivation,
    active_kernel_session_ids, capture_preserved_workspace_run_state,
    capture_webui_prompt_preparation_generation, capture_webui_prompt_preparation_workspace,
    composer_attachment_draft_scope, composer_attachment_summary_to_core,
    composer_attachment_summary_to_provider_core, composer_attachment_workspace_path_from_snapshot,
    current_unix_millis_text, current_unix_millis_value, defer_attachment_delivery_to_v1,
    emit_pending_prompts_changed_delta, emit_service_event_with_trace,
    enqueue_webui_pending_prompt_from_submission, ensure_session_terminal_settlement_settled,
    hydrate_retained_submission_session_if_needed, load_workspace_for_submission,
    mark_webui_prompt_preparation_failed, mark_webui_prompt_snapshot_preparing,
    materialize_prompt_session, owning_project_id_for_session, parse_prompt_session_target,
    parse_session_id, persist_initial_submission_goal, preserve_active_session_run_state,
    provider_tool_capability_key, queued_dispatch_authority_for_client_action,
    refresh_exact_vmm_hook_session_projection, resolve_composer_attachment_summary_for_workspace,
    retain_slow_submission_diagnostic, retained_submission_snapshot_has_session_context,
    retained_submission_snapshot_is_usable, runtime_parts_from_state,
    session_has_blocking_system_prompt_work, session_process_public_ai_selection,
    spawn_service_task, start_exact_v1_queued_turn, start_reserved_webui_session_kernel_user_turn,
    start_webui_precheck_or_continue, start_webui_session_kernel_user_turn,
    webui_session_kernel_has_active_turn, webui_shared_prompt_precheck_plan, webui_submit_trace,
    webui_trace_if_slow, webui_vmm_runtime_readiness,
};
mod activation;
pub(crate) mod idle_root_input;
mod preflight;
mod queue;
mod runtime_options;
mod submission_failure;

pub(crate) use activation::*;
pub(crate) use preflight::*;
pub(crate) use queue::*;
pub(crate) use runtime_options::*;
pub(crate) use submission_failure::settle_admitted_submission_failure;

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
    /// V1 continuation admitted by one durable ClientActionId in the current open Turn.
    /// 由当前开放回合中的可靠 ClientActionId 准入的 V1 Continue。
    Continuation,
}

impl ConversationSubmissionMode {
    /// Returns the prompt source recorded in the shared turn state.
    /// 返回共享回合状态中记录的提示词来源。
    fn prompt_source(&self) -> PendingSessionPromptSource {
        match self {
            Self::UserPrompt => PendingSessionPromptSource::UserFollowup,
            Self::Continuation => PendingSessionPromptSource::ExecutionRetry,
        }
    }

    /// Returns whether this mode should append user-authored conversation records.
    /// 返回本模式是否应追加用户编写的会话记录。
    fn appends_user_records(&self) -> bool {
        matches!(self, Self::UserPrompt)
    }

    /// Returns whether this submission resumes one retained interrupted execution.
    /// 返回本次提交是否恢复一项已保留的中断执行。
    fn is_continuation(&self) -> bool {
        matches!(self, Self::Continuation)
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
    /// Native model-extension target sets selected per generative capability.
    /// 按生成类能力选择的原生模型扩展目标集合。
    extension_target_sets: BTreeMap<
        vulcan_provider_adapters::ProviderToolCapabilityKind,
        std::collections::BTreeSet<vulcan_provider_adapters::ProviderToolTargetSelection>,
    >,
    /// Extension modes chosen in the draft; a missing capability is single mode without a model.
    /// 草稿中选择的扩展模式；缺失的能力表示未选模型的单模型模式。
    extension_modes: BTreeMap<
        vulcan_provider_adapters::ProviderToolCapabilityKind,
        vulcan_provider_adapters::ProviderExtensionModeSelection,
    >,
    /// Exact session-owned provider-tool or replacement-model route per media capability.
    /// 当前会话为每项媒体能力持有的精确供应商工具或替代模型路由。
    #[serde(default)]
    media_understanding_selections: BTreeMap<
        vulcan_provider_adapters::ProviderToolCapabilityKind,
        vulcan_provider_adapters::MediaUnderstandingBackendSelection,
    >,
}

/// Freezes every draft-only session setting required by the first provider turn.
/// 固定首个 Provider 回合所需的全部草稿会话设置。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionSessionSettings {
    /// Explicit answer mode captured by the draft; absent copies the global default.
    /// 草稿捕获的显式回答模式；缺省时复制全局默认值。
    #[serde(default)]
    answer_mode: Option<vulcan_session_core::v1::config::AnswerMode>,
    /// Explicit permission mode captured by the draft; absent keeps the session default.
    /// 草稿捕获的显式权限模式；缺省时保留会话默认值。
    #[serde(default)]
    permission_mode: Option<vulcan_session_core::v1::config::PermissionMode>,
    /// Optional session output-style identifier captured before the first prompt boundary.
    /// 在首次提示词边界前捕获的可选会话输出风格标识。
    #[serde(default)]
    output_style_id: Option<String>,
    /// Optional explicit VMM policy pair; absence preserves inherited defaults.
    /// 可选显式 VMM 策略对；缺省时保留继承默认值。
    vmm_policy: Option<SubmissionVmmPolicy>,
    /// Optional complete provider-tool snapshot; absence preserves initialized session state or materializes defaults for a new session.
    /// 可选完整供应商工具快照；缺省时保留已初始化会话状态，或为新会话物化默认值。
    provider_tool_settings: Option<SubmissionProviderToolSettings>,
    /// Optional knowledge selection; absence preserves project inheritance.
    /// 可选知识库选择；缺省时保留项目继承。
    knowledge_selection: Option<SubmissionKnowledgeSelection>,
    /// Optional initial goal created before the first turn starts.
    /// 首轮开始前创建的可选初始目标。
    initial_goal: Option<SubmissionInitialGoal>,
    /// Dedicated-session choices; present only when the draft creates a dedicated session.
    /// 专职会话选择；仅当草稿创建专职会话时存在。
    #[serde(default)]
    dedicated: Option<SubmissionDedicatedSettings>,
    /// Session skill switches, keyed by skill name; only bundled on-demand skills are accepted.
    /// 会话技能开关，按技能名称索引；只接受随包的按需技能。
    #[serde(default)]
    prompt_skill_overrides: std::collections::BTreeMap<String, SubmissionPromptSkillState>,
}

/// One session skill switch a new conversation carries.
/// 新会话携带的一个会话技能开关。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SubmissionPromptSkillState {
    Enabled,
    Disabled,
}

/// Resolves the session skill switches of a first submission. Only bundled skills declared
/// `activation: on_demand` may be switched here, because a dedicated flow such as on-device fixing
/// starts them; every ordinary skill keeps the switches of the skills settings.
/// 解析首次提交的会话技能开关。这里只能切换声明 `activation: on_demand` 的随包技能，因为它们由
/// 本机修复等专门流程启动；普通技能一律沿用技能设置中的开关。
///
/// # Parameters
/// 参数
/// `requested` is the draft's map; `prompt_skills` is the persisted global skill configuration.
/// `requested` 是草稿给出的映射；`prompt_skills` 是持久化的全局技能配置。
///
/// # Returns
/// 返回值
/// Overrides ordered by skill name, or an error naming the first skill that is not an on-demand
/// bundled skill.
/// 按技能名称排序的覆盖；遇到第一个不是随包按需技能的名称时返回错误。
fn submission_prompt_skill_overrides(
    requested: &std::collections::BTreeMap<String, SubmissionPromptSkillState>,
    prompt_skills: &vulcan_provider_config::config::PersistedPromptSkillsConfiguration,
) -> Result<Vec<vulcan_session_core::v1::config::PromptSkillOverride>, String> {
    if requested.is_empty() {
        return Ok(Vec::new());
    }
    let catalog = vulcan_prompt_skills::load_prompt_skills(
        &vulcan_agent_runtime::skills::prompt::global_config_from_persisted(prompt_skills),
    );
    requested
        .iter()
        .map(|(name, state)| {
            let on_demand = catalog.skills.iter().any(|skill| {
                skill.name == *name
                    && skill.scope == vulcan_prompt_skills::PromptSkillScope::System
                    && skill.activation == vulcan_prompt_skills::PromptSkillActivation::OnDemand
            });
            if !on_demand {
                return Err(format!(
                    "Prompt skill `{name}` is not a bundled on-demand skill and cannot be switched by a new conversation."
                ));
            }
            Ok(vulcan_session_core::v1::config::PromptSkillOverride {
                skill_id: name.clone(),
                enabled: *state == SubmissionPromptSkillState::Enabled,
            })
        })
        .collect()
}

/// Converts persisted session skill switches into the runtime map every turn reads.
/// 将持久化的会话技能开关转换为每个回合读取的运行态映射。
pub(crate) fn runtime_prompt_skill_overrides(
    overrides: &[vulcan_session_core::v1::config::PromptSkillOverride],
) -> std::collections::BTreeMap<String, vulcan_provider_config::config::PersistedPromptSkillState> {
    overrides
        .iter()
        .map(|entry| {
            let state = if entry.enabled {
                vulcan_provider_config::config::PersistedPromptSkillState::Enabled
            } else {
                vulcan_provider_config::config::PersistedPromptSkillState::Disabled
            };
            (entry.skill_id.clone(), state)
        })
        .collect()
}

/// Dedicated-session choices captured by the creation wizard.
/// 创建向导捕获的专职会话选择。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubmissionDedicatedSettings {
    /// General tool access fixed at creation.
    /// 创建时固定的通用工具开放方式。
    tool_access: vulcan_session_core::v1::DedicatedToolAccess,
    /// User-confirmed base prompt sent as the session's system instruction.
    /// 用户确认、作为会话系统指令发送的基础提示词。
    base_prompt: String,
    /// Whether memory is locked before the first user input.
    /// 是否在首条用户输入之前锁定记忆。
    lock_immediately: bool,
}

/// Validates dedicated choices and returns the creation request, or `None` for ordinary drafts.
/// 校验专职选择并返回创建请求；普通草稿返回 `None`。
fn submission_dedicated_creation(
    settings: Option<&SubmissionSessionSettings>,
    execution_mode: Option<&str>,
) -> Result<Option<crate::session_v1_lifecycle::DedicatedSessionCreation>, String> {
    let Some(settings) = settings else {
        return Ok(None);
    };
    let Some(dedicated) = settings.dedicated.as_ref() else {
        return Ok(None);
    };
    if settings.initial_goal.is_some()
        || execution_mode.is_some_and(|mode| mode.eq_ignore_ascii_case("plan"))
    {
        return Err(String::from("ERR_DEDICATED_SESSION_MODE_UNSUPPORTED"));
    }
    let profile = vulcan_session_core::v1::DedicatedSessionProfile::new(
        dedicated.tool_access,
        dedicated.base_prompt.trim(),
    )
    .map_err(|error| format!("ERR_DEDICATED_SESSION_INVALID: {error}"))?;
    Ok(Some(
        crate::session_v1_lifecycle::DedicatedSessionCreation {
            profile,
            lock_immediately: dedicated.lock_immediately,
        },
    ))
}

/// Reports whether an absent per-turn snapshot must preserve initialized session-owned settings.
/// 判断缺失的本回合快照是否必须保留已初始化的会话自有设置。
///
/// # Parameters
/// 参数
/// `has_explicit_settings` reports whether this submission carries a complete replacement snapshot;
/// `provider_tool_settings_initialized` reports whether defaults were already materialized for the session.
/// `has_explicit_settings` 表示本次提交是否携带完整替换快照；
/// `provider_tool_settings_initialized` 表示默认值是否已经为当前会话物化。
///
/// # Returns
/// 返回值
/// Returns `true` only when the submission omits settings and the session already owns a materialized snapshot.
/// 仅当提交未携带设置且会话已持有物化快照时返回 `true`。
fn should_preserve_existing_provider_tool_settings(
    has_explicit_settings: bool,
    provider_tool_settings_initialized: bool,
) -> bool {
    !has_explicit_settings && provider_tool_settings_initialized
}

/// Converts one submitted VMM tri-state mode into the persisted optional layer value.
/// 将一个已提交的 VMM 三态模式转换为持久化可选层取值。
fn submission_vmm_policy_value(mode: &str) -> Result<Option<bool>, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "inherit" => Ok(None),
        "enabled" => Ok(Some(true)),
        "disabled" => Ok(Some(false)),
        other => Err(format!("Unsupported VMM memory policy mode: {other}")),
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
            "Goal mode and plan mode are mutually exclusive and cannot both be enabled in the same first submission.",
        ));
    }
    let configuration = runtime.configuration_snapshot()?;
    // New-session defaults come from the same immutable settings snapshot.
    // 新会话默认值来自同一份不可变设置快照。
    let (
        global_output_style_selection,
        global_answer_mode,
        global_permission_mode,
        global_prompt_skills,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let provider_configuration = &guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
            .provider_configuration;
        (
            provider_configuration.prompt.output_style,
            crate::answer_mode::global_answer_mode(provider_configuration),
            crate::permission_mode::effective_default_permission_mode(provider_configuration),
            provider_configuration.prompt_skills.clone(),
        )
    };
    let prompt_skill_overrides_v1 = match settings {
        Some(settings) => submission_prompt_skill_overrides(
            &settings.prompt_skill_overrides,
            &global_prompt_skills,
        )?,
        None => Vec::new(),
    };
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
    );
    let enabled_capabilities = provider_tool_settings
        .map(|settings| &settings.enabled_capabilities)
        .unwrap_or(&configuration.provider_tool_enabled_capabilities);
    let provider_tool_selections = provider_tool_settings
        .map(|settings| &settings.selections)
        .unwrap_or(&configuration.provider_tool_selections);
    let provider_extension_target_sets = provider_tool_settings
        .map(|settings| &settings.extension_target_sets)
        .unwrap_or(&configuration.provider_extension_target_sets);
    let media_understanding_selections = provider_tool_settings
        .map(|settings| &settings.media_understanding_selections)
        .unwrap_or(&configuration.media_understanding_selections);
    let provider_extension_modes = provider_tool_settings
        .map(|settings| &settings.extension_modes)
        .unwrap_or(&configuration.provider_extension_modes);
    if !preserve_existing_provider_tools {
        crate::public_ai_settings::validate_provider_tool_session_settings(
            runtime,
            &configuration,
            enabled_capabilities,
            provider_tool_selections,
            media_understanding_selections,
            provider_extension_target_sets,
            provider_extension_modes,
        )?;
    }
    let knowledge_selection_v1 = if let Some(selection) =
        settings.and_then(|settings| settings.knowledge_selection.as_ref())
    {
        let mode = match selection.mode.as_str() {
            "inherit" => KnowledgeLibrarySelectionModeV1::Inherit,
            "explicit" => KnowledgeLibrarySelectionModeV1::Explicit,
            other => return Err(format!("Unknown knowledge library selection mode: {other}")),
        };
        let knowledge_runtime = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .knowledge_runtime
            .clone()
            .ok_or_else(|| {
                String::from("service is still starting; knowledge runtime is not ready")
            })?;
        let project_id =
            owning_project_id_for_session(workspace_snapshot, session_id).ok_or_else(|| {
                String::from("The current session does not belong to an available project.")
            })?;
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
            return Err(format!(
                "Knowledge libraries not authorized for the current project: {denied}"
            ));
        }
        let mut library_ids = selection.library_ids.clone();
        library_ids.sort();
        library_ids.dedup();
        if mode == KnowledgeLibrarySelectionModeV1::Inherit {
            library_ids.clear();
        }
        KnowledgeLibrarySelectionConfigV1 { mode, library_ids }
    } else {
        KnowledgeLibrarySelectionConfigV1::default()
    };
    let vmm_selection_v1 = if settings.is_some_and(|settings| settings.dedicated.is_some()) {
        // Dedicated sessions never use VMM memory; the choice is frozen with the session.
        // 专职会话从不使用 VMM 记忆；该选择随会话冻结。
        VmmSessionPolicyConfig {
            precheck_enabled: Some(false),
            memory_writeback_enabled: Some(false),
        }
    } else if let Some(policy) = settings.and_then(|settings| settings.vmm_policy.as_ref()) {
        VmmSessionPolicyConfig {
            precheck_enabled: submission_vmm_policy_value(&policy.precheck_mode)?,
            memory_writeback_enabled: submission_vmm_policy_value(&policy.memory_writeback_mode)?,
        }
    } else {
        VmmSessionPolicyConfig::default()
    };
    let updated_at_ms = i64::try_from(current_unix_millis_value()).map_err(|_| {
        String::from("Current Unix millisecond time exceeds the V1 persistence range.")
    })?;
    let updated_at = updated_at_ms.to_string();
    let persisted_selection_v1 = if allow_output_style_activation {
        let handle =
            crate::session_goal_lifecycle::session_v1_handle_from_state(state, session_id).await?;
        let provider_tools = if preserve_existing_provider_tools {
            handle
                .read(|document| document.metadata.config.selection.provider_tools.clone())
                .map_err(|error| format!("Failed to read V1 Provider tool selection: {error}"))?
        } else {
            crate::public_ai_settings::provider_tool_selection_config_from_parts_v1(
                enabled_capabilities,
                provider_tool_selections,
                media_understanding_selections,
                provider_extension_target_sets,
                provider_extension_modes,
                has_explicit_provider_tool_settings,
            )
        };
        crate::session_request_config_v1::set_session_selection_bundle_v1(
            handle.as_ref(),
            SessionSelectionBundleInput {
                output_style: submission_output_style_id(settings),
                provider_tools,
                knowledge_libraries: knowledge_selection_v1,
                vmm: vmm_selection_v1,
                prompt_skill_overrides: prompt_skill_overrides_v1,
            },
            settings
                .and_then(|settings| settings.answer_mode)
                .unwrap_or(global_answer_mode),
            updated_at_ms,
        )?;
        crate::permission_mode::set_session_permission_mode_v1(
            handle.as_ref(),
            settings
                .and_then(|settings| settings.permission_mode)
                .unwrap_or(global_permission_mode),
            updated_at_ms,
        )?;
        Some(
            handle
                .read(|document| document.metadata.config.selection.clone())
                .map_err(|error| format!("Failed to read V1 session selection config: {error}"))?,
        )
    } else {
        None
    };
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    if let Some(selection) = persisted_selection_v1.as_ref() {
        runtime_state.vmm.precheck_enabled = selection.vmm.precheck_enabled;
        runtime_state.vmm.memory_writeback_enabled = selection.vmm.memory_writeback_enabled;
        runtime_state.prompt_skill_overrides =
            runtime_prompt_skill_overrides(&selection.prompt_skill_overrides);
    }
    if !preserve_existing_provider_tools {
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
        runtime_state.media_understanding_selections =
            crate::public_ai_settings::session_media_understanding_selections_to_runtime(
                media_understanding_selections,
            );
        runtime_state.provider_extension_target_sets = provider_extension_target_sets
            .iter()
            .map(|(capability, selections)| {
                (
                    provider_tool_capability_key(*capability),
                    selections
                        .iter()
                        .map(|selection| SessionProviderToolTargetSelection {
                            connection_id: selection.connection_id.clone(),
                            target_id: selection.target_id.clone(),
                        })
                        .collect(),
                )
            })
            .collect();
        runtime_state.provider_extension_modes =
            crate::public_ai_settings::session_provider_extension_modes_to_runtime(
                provider_extension_modes,
            );
        runtime_state.provider_tool_settings_initialized = true;
        runtime_state.provider_tool_selections_overridden = has_explicit_provider_tool_settings;
    }
    if let Some(selection) = persisted_selection_v1.as_ref() {
        runtime_state.knowledge_library_selection = SessionKnowledgeLibrarySelection {
            mode: match selection.knowledge_libraries.mode {
                KnowledgeLibrarySelectionModeV1::Inherit => {
                    SessionKnowledgeLibrarySelectionMode::Inherit
                }
                KnowledgeLibrarySelectionModeV1::Explicit => {
                    SessionKnowledgeLibrarySelectionMode::Explicit
                }
            },
            library_ids: selection.knowledge_libraries.library_ids.clone(),
        };
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
        persist_initial_submission_goal(state, session_id, &goal.objective, goal.token_budget)
            .await?;
    }
    Ok(())
}

/// Reports whether a persisted restart lease targets the same route owner but an older execution revision.
/// 判断持久化的重启租约是否仍指向同一路由所有者、但执行修订已经过期。
///
/// # Parameters
/// 参数
/// `expected` is the non-secret lease identity restored with the session runtime state.
/// `expected` 是随会话运行态恢复的非敏感租约身份。
/// `actual` is the identity resolved from the currently installed provider manifest.
/// `actual` 是从当前已安装供应商清单解析出的身份。
///
/// # Returns
/// 返回值
/// Returns true only when schema, provider, connection, model, and protocol are identical while
/// the execution route revision changed.
/// 仅当 Schema、供应商、连接、模型与协议完全相同且执行路由修订发生变化时返回 true。
fn provider_route_lease_revision_changed_for_same_target(
    expected: &vulcan_workbench_model::SessionProviderRouteLeaseIdentity,
    actual: &vulcan_workbench_model::SessionProviderRouteLeaseIdentity,
) -> bool {
    expected.schema_version == actual.schema_version
        && expected.provider_id == actual.provider_id
        && expected.connection_id == actual.connection_id
        && expected.model_id == actual.model_id
        && expected.protocol_profile_id == actual.protocol_profile_id
        && expected.route_revision != actual.route_revision
}

/// Loads the current Provider configuration after validating one exact connection ownership tuple.
/// 校验一个精确连接归属三元组后加载当前 Provider 配置。
///
/// # Parameters
/// 参数
/// `runtime` owns the current immutable Provider generation and `selection` is the exact requested tuple.
/// `runtime` 持有当前不可变 Provider 代际，`selection` 是精确请求三元组。
///
/// # Returns
/// 返回值
/// The authoritative configuration when the connection exists and belongs to the requested Provider.
/// 当连接存在且属于请求的 Provider 时返回权威配置。
fn validated_submission_model_selection_configuration(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    selection: &SubmissionModelSelection,
) -> Result<vulcan_provider_adapters::PersistedAiConfiguration, String> {
    // configuration is captured once so connection ownership and later route resolution use one generation.
    // configuration 只捕获一次，使连接归属与后续路由解析使用同一代际。
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
    Ok(configuration)
}

/// Resolves one enabled exact model tuple against the same effective catalog used by execution.
/// 根据执行使用的同一有效目录解析一个已启用的精确模型三元组。
///
/// # Parameters
/// 参数
/// `runtime` owns catalog resolution, `configuration` is one captured generation, and `selection` is exact.
/// `runtime` 持有目录解析，`configuration` 是一次捕获的代际，`selection` 是精确选择。
///
/// # Returns
/// 返回值
/// The fully resolved immutable route, or an explicit unavailable/disabled error without fallback.
/// 返回完整解析的不可变路由；若不可用或被禁用，则返回明确错误且不降级。
pub(crate) fn resolve_enabled_submission_model_selection(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    selection: &SubmissionModelSelection,
) -> Result<vulcan_provider_adapters::ResolvedPublicAiSelection, String> {
    if !runtime.model_is_enabled_for_connection(
        configuration,
        selection.connection_id.as_str(),
        selection.model_id.as_str(),
    )? {
        return Err(format!(
            "Provider `{}` model `{}` is disabled.",
            selection.provider_id, selection.model_id
        ));
    }
    // resolvedCatalogs reconstruct the same overlay-aware membership used by execution.
    // resolvedCatalogs 重建执行使用的同一份覆盖感知成员关系。
    let resolved_catalogs = runtime.resolved_model_catalog_snapshot(configuration)?;
    let source_catalogs = runtime.source_catalog_snapshot()?;
    let effective_catalogs = if resolved_catalogs.contains_key(selection.connection_id.as_str())
        || source_catalogs.contains_key(selection.connection_id.as_str())
    {
        resolved_catalogs
    } else {
        // Manual catalogs without immutable source tracking retain their process-local authority.
        // 没有不可变来源跟踪的手工目录继续使用其进程内权威。
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
        configuration,
        selection.connection_id.as_str(),
        selection.provider_id.as_str(),
        selection.model_id.as_str(),
    )
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
/// `materialization_kind` distinguishes draft selection from existing V1-owned request configuration.
/// `materialization_kind` 区分草稿选择与既有 V1 持有的请求配置。
/// `selection` is the draft picker echo; existing sessions intentionally omit it.
/// `selection` 是草稿选择器回显；已有会话会有意省略它。
///
/// # Returns
/// 返回值
/// Returns success after an exact configured selection is applied, or a validation error.
/// 在精确已配置选择写入后返回成功，否则返回校验错误。
pub(crate) fn apply_submission_model_selection(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    materialization_kind: PromptSessionMaterializationKind,
    selection: Option<&SubmissionModelSelection>,
) -> Result<(), String> {
    // Existing roots have already activated their immutable V1-bound request. Their intentionally
    // absent picker echo must not be validated against the stale process-local pending mirror.
    // 既有根输入已经激活不可变的 V1 绑定请求，不能用有意省略的选择器回显校验旧进程待生效镜像。
    // synchronize_v1_dispatch_model_selection later projects that admitted request before dispatch.
    // 后续 synchronize_v1_dispatch_model_selection 会在分发前投影该已接纳请求。
    if materialization_kind == PromptSessionMaterializationKind::Existing {
        return Ok(());
    }
    // pendingSelection is the exact user choice accepted while the previous turn retained its
    // immutable route. Submission may promote only this tuple and must never silently replace it.
    // pendingSelection 是上一回合保持不可变路由时接受的用户精确选择。提交只能提升该三元组，绝不能静默替换。
    let pending_selection = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.pending_model_selection.clone());
    if pending_selection.is_some() && selection.is_none() {
        return Err(String::from(
            "The pending session model is unavailable. Refresh the model list and explicitly reselect a model before submitting.",
        ));
    }
    let Some(selection) = selection else {
        return Ok(());
    };
    if let Some(pending_selection) = pending_selection.as_ref()
        && (pending_selection.provider_id != selection.provider_id
            || pending_selection.connection_id != selection.connection_id
            || pending_selection.model_id != selection.model_id)
    {
        return Err(String::from(
            "The submitted model does not match the session's pending exact selection. Refresh the model list and reselect explicitly.",
        ));
    }
    let configuration = validated_submission_model_selection_configuration(runtime, selection)?;
    // sessionLeaseScope is the stable owner key shared by process-local and persisted leases.
    // sessionLeaseScope 是进程内租约与持久化租约共享的稳定所有者键。
    let session_lease_scope = session_id.to_string();
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
        runtime_state.pending_model_selection = None;
        runtime_state.updated_at = current_unix_millis_text();
        return Ok(());
    }
    // resolvedSelection freezes the exact currently installed route used to validate restart drift.
    // resolvedSelection 冻结用于校验重启漂移的当前已安装精确路由。
    let resolved_selection =
        resolve_enabled_submission_model_selection(runtime, &configuration, selection)?;
    // processRouteLeaseAbsent distinguishes restart restoration from an in-process hot replacement.
    // processRouteLeaseAbsent 区分重启恢复与进程内热替换。
    let process_route_lease_absent = runtime
        .session_selection_lease(session_lease_scope.as_str())?
        .is_none();
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
    // resolvedRouteIdentity is comparable with the persisted non-secret identity without retaining credentials.
    // resolvedRouteIdentity 可与持久化的非敏感身份比较，且不会保留凭据。
    let resolved_route_identity =
        vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
            &resolved_selection,
        );
    // restartedRouteChanged permits an ordinary new submission after an application upgrade to
    // adopt the installed route only when its owner and protocol are unchanged. An established
    // process-local lease still wins, preserving in-flight and hot-reload route immutability.
    // restartedRouteChanged 仅允许应用升级后的普通新提交在所有者与协议未变化时采用已安装路由；
    // 已建立的进程内租约仍然优先，从而保持执行中请求与热更新路由不可变。
    let restarted_route_changed = !target_changed
        && process_route_lease_absent
        && runtime_state
            .provider_route_lease
            .as_ref()
            .is_some_and(|expected| {
                provider_route_lease_revision_changed_for_same_target(
                    expected,
                    &resolved_route_identity,
                )
            });
    if target_changed || restarted_route_changed {
        runtime.clear_session_selection_leases(session_lease_scope.as_str())?;
        runtime_state.provider_route_lease = None;
    }
    runtime_state.active_provider_id = Some(selection.provider_id.clone());
    runtime_state.active_connection_id = Some(selection.connection_id.clone());
    runtime_state.active_model_id = Some(selection.model_id.clone());
    runtime_state.pending_model_selection = None;
    runtime_state.updated_at = current_unix_millis_text();
    Ok(())
}

/// Validates a recovery submission's exact future model without mutating the frozen active route.
/// 校验恢复提交的精确未来模型，同时不修改已冻结的当前活动路由。
///
/// # Parameters
/// 参数
/// `runtime` owns the authoritative catalog and process-local route lease.
/// `runtime` 持有权威目录与进程内路由租约。
/// `workspace_snapshot` contains the retained active and pending session selections.
/// `workspace_snapshot` 包含保留的活动与待生效会话选择。
/// `session_id` identifies the interrupted conversation and `selection` is the composer snapshot.
/// `session_id` 标识被中断会话，`selection` 是输入区快照。
///
/// # Returns
/// 返回值
/// Success only when the submitted tuple exactly matches the already-authoritative future selection.
/// 仅当提交三元组精确匹配已有权威未来选择时返回成功。
fn validate_recovery_submission_model_selection(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    selection: Option<&SubmissionModelSelection>,
) -> Result<(), String> {
    let Some(selection) = selection else {
        return Ok(());
    };
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING"))?;
    let expected_provider_id = runtime_state
        .pending_model_selection
        .as_ref()
        .map(|pending| pending.provider_id.as_str())
        .or(runtime_state.active_provider_id.as_deref());
    let expected_connection_id = runtime_state
        .pending_model_selection
        .as_ref()
        .map(|pending| pending.connection_id.as_str())
        .or(runtime_state.active_connection_id.as_deref());
    let expected_model_id = runtime_state
        .pending_model_selection
        .as_ref()
        .map(|pending| pending.model_id.as_str())
        .or(runtime_state.active_model_id.as_deref());
    if expected_provider_id != Some(selection.provider_id.as_str())
        || expected_connection_id != Some(selection.connection_id.as_str())
        || expected_model_id != Some(selection.model_id.as_str())
    {
        return Err(String::from(
            "ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH",
        ));
    }

    let configuration = validated_submission_model_selection_configuration(runtime, selection)?;
    let leased_selection =
        session_process_public_ai_selection(runtime, session_id, Some(runtime_state))?.filter(
            |leased| {
                leased.route.provider.id == selection.provider_id
                    && leased.route.connection.id == selection.connection_id
                    && leased.model.id == selection.model_id
            },
        );
    if leased_selection.is_some() {
        let explicitly_disabled = configuration
            .model_enabled_override(selection.provider_id.as_str(), selection.model_id.as_str())
            == Some(false)
            || !configuration.connection_model_is_enabled(
                selection.connection_id.as_str(),
                selection.model_id.as_str(),
            );
        if explicitly_disabled {
            return Err(String::from(
                "ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH",
            ));
        }
        return Ok(());
    }
    resolve_enabled_submission_model_selection(runtime, &configuration, selection)
        .map(|_| ())
        .map_err(|_| String::from("ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH"))
}

#[cfg(test)]
mod submission_model_selection_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use vulcan_provider_adapters::{
        CredentialConnection, CredentialMode, CustomProviderAuthentication,
        CustomProviderDefinition, CustomProviderModelDiscoveryMode,
        CustomProviderModelDiscoveryProtocol, CustomProviderProtocol, PersistedAiConfiguration,
        PublicAiRuntime, UnavailableCredentialSecretResolver, parse_openai_model_catalog,
        parse_openrouter_model_catalog,
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
                &[vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID],
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

    /// Returns a custom account catalog containing exactly `model_ids` at the supplied `generation`.
    /// 返回自定义账号在指定 `generation` 代际恰好包含 `model_ids` 的目录。
    /// The fixture has no declared models, so removing an ID exercises actual discovery removal.
    /// 夹具没有静态声明模型，因此移除 ID 会覆盖真实的发现目录移除行为。
    fn lease_discovery_catalog(
        generation: u64,
        model_ids: &[&str],
    ) -> vulcan_provider_adapters::ConnectionModelCatalog {
        // Standard discovery bytes keep the production OpenAI-compatible decoder in the fixture.
        // 标准发现报文让夹具继续覆盖生产 OpenAI 兼容解码器。
        let body = serde_json::json!({
            "object": "list",
            "data": model_ids
                .iter()
                .map(|id| serde_json::json!({"id": id, "object": "model", "owned_by": "fixture"}))
                .collect::<Vec<_>>(),
        });
        with_protocol_variants(
            parse_openai_model_catalog(LEASE_DISCOVERY_ACCOUNT, generation, &body.to_string())
                .expect("custom discovery catalog should parse"),
            &[vulcan_llm_core::OPENAI_CHAT_PROFILE_ID],
        )
    }

    /// Provider identity for the discovery-only lease fixture.
    /// 仅通过发现构建目录的租约夹具供应商身份。
    const LEASE_DISCOVERY_PROVIDER: &str = "custom:lease-discovery";

    /// Connection identity for the discovery-only lease fixture.
    /// 仅通过发现构建目录的租约夹具连接身份。
    const LEASE_DISCOVERY_ACCOUNT: &str = "lease-discovery-account";

    /// Returns an isolated custom-provider runtime with two discovered models and no network access.
    /// 返回具有两个发现模型且不访问网络的隔离自定义供应商运行时。
    fn lease_discovery_runtime() -> PublicAiRuntime {
        // The unavailable resolver prevents the fixture from reading real credentials.
        // 不可用的凭据解析器防止夹具读取真实密钥。
        let runtime =
            PublicAiRuntime::first_batch(std::sync::Arc::new(UnavailableCredentialSecretResolver))
                .expect("runtime should initialize");
        // An explicit custom discovery policy avoids coupling lease behavior to a vendor's fixed inventory.
        // 显式自定义发现策略避免将租约行为绑定到供应商的固定模型清单。
        let descriptor = CustomProviderDefinition {
            id: String::from(LEASE_DISCOVERY_PROVIDER),
            display_name: String::from("Lease Discovery Fixture"),
            base_url: String::from("https://lease-discovery.example.test/v1"),
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_protocol: CustomProviderModelDiscoveryProtocol::OpenAi,
            model_discovery_path: None,
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect("custom discovery provider should compile");
        // The standalone manifest owns the frozen binding revision, as it does in saved custom settings.
        // 独立清单与保存后的自定义设置一样，持有冻结绑定的修订身份。
        let manifest =
            vulcan_provider_adapters::provider_manifest::project_custom_descriptor_manifest(
                &descriptor,
            )
            .expect("custom discovery manifest should project");
        // Configuration owns the exact descriptor, manifest and single credential connection.
        // 配置持有精确描述符、清单及唯一凭据连接。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(String::from(LEASE_DISCOVERY_PROVIDER), descriptor);
        configuration
            .custom_provider_manifests
            .insert(String::from(LEASE_DISCOVERY_PROVIDER), manifest);
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .effective_providers(&configuration)
                    .expect("provider registry should resolve"),
                CredentialConnection {
                    id: String::from(LEASE_DISCOVERY_ACCOUNT),
                    provider_id: String::from(LEASE_DISCOVERY_PROVIDER),
                    service_entry_id: String::from("default"),
                    credential_profile_id: String::from("api-key"),
                    credential_mode: CredentialMode::Token,
                    credential_id: String::from("credential-lease-discovery"),
                    display_name: String::from("Lease Discovery Account"),
                    secret_reference: String::from("secret-lease-discovery"),
                    management_secret_reference: None,
                    service_origin_override: None,
                },
            )
            .expect("connection should register");
        configuration.set_model_enabled(LEASE_DISCOVERY_PROVIDER, "discovered-first", true);
        configuration.set_model_enabled(LEASE_DISCOVERY_PROVIDER, "discovered-second", true);
        configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
            provider_id: String::from(LEASE_DISCOVERY_PROVIDER),
            model_id: String::from("discovered-first"),
            connection_id: String::from(LEASE_DISCOVERY_ACCOUNT),
        });
        runtime
            .stage_source_catalog_for_testing(lease_discovery_catalog(
                1,
                &["discovered-first", "discovered-second"],
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
            PromptSessionMaterializationKind::Created,
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

    /// Verifies only the exact pending tuple can be promoted for the next real Provider request.
    /// 验证只有精确的待生效三元组才能为下一次真实 Provider 请求提升。
    #[test]
    fn next_provider_request_promotes_only_the_exact_pending_model_selection() {
        // runtime owns both available model identities used by the stale and accepted submissions.
        // runtime 持有过期提交与获准提交使用的两个可用模型身份。
        let runtime = configured_runtime();
        // sessionId identifies the existing session whose active route must not drift before promotion.
        // sessionId 标识在提升前活动路由不得漂移的既有会话。
        let session_id = Uuid::new_v4();
        // workspaceSnapshot contains the materialized session runtime row under test.
        // workspaceSnapshot 包含本测试使用的已物化会话运行态行。
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
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)
            .expect("session runtime state should initialize");
        // pendingSelection is the exact picker choice accepted for the next Provider request.
        // pendingSelection 是选择器为下一次 Provider 请求接受的精确选择。
        let pending_selection = vulcan_workbench_model::SessionPendingModelSelection {
            provider_id: String::from("openrouter"),
            connection_id: String::from("openrouter-account"),
            model_id: String::from("openai/gpt-5.6-sol"),
        };
        {
            // runtimeState starts on Terra while Sol remains pending.
            // runtimeState 当前使用 Terra，而 Sol 保持待生效。
            let runtime_state = workspace_snapshot
                .session_runtime_state
                .get_mut(&session_id)
                .expect("session runtime state should exist");
            runtime_state.active_provider_id = Some(String::from("openrouter"));
            runtime_state.active_connection_id = Some(String::from("openrouter-account"));
            runtime_state.active_model_id = Some(String::from("openai/gpt-5.6-terra"));
            runtime_state.pending_model_selection = Some(pending_selection.clone());
        }
        // staleSelection models an old composer snapshot that still names the active model.
        // staleSelection 模拟仍指向活动模型的旧输入区快照。
        let stale_selection = SubmissionModelSelection {
            provider_id: String::from("openrouter"),
            connection_id: String::from("openrouter-account"),
            model_id: String::from("openai/gpt-5.6-terra"),
        };
        let stale_error = apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            PromptSessionMaterializationKind::Created,
            Some(&stale_selection),
        )
        .expect_err("stale composer selection must not replace pending authority");
        assert!(stale_error.contains("pending exact selection"));
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|state| state.active_model_id.as_deref()),
            Some("openai/gpt-5.6-terra")
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|state| state.pending_model_selection.as_ref()),
            Some(&pending_selection)
        );

        // The admitted submitted snapshot follows the picker; V1 promotion is tested through the real permit separately.
        // 已准入提交快照跟随选择器；V1 提升通过真实许可边界单独验证。
        let selection = SubmissionModelSelection {
            provider_id: pending_selection.provider_id.clone(),
            connection_id: pending_selection.connection_id.clone(),
            model_id: pending_selection.model_id,
        };
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            PromptSessionMaterializationKind::Created,
            Some(&selection),
        )
        .expect("the selected new model is accepted");
        assert_eq!(
            workspace_snapshot.session_runtime_state[&session_id]
                .active_model_id
                .as_deref(),
            Some("openai/gpt-5.6-sol")
        );
    }

    /// Verifies a new session rebinds an invalid global account to the preferred account for the same model.
    /// 验证新会话会把失效的全局账号重绑到同一模型的首选账号。
    #[test]
    fn new_session_rebinds_invalid_global_account_for_same_model() {
        // runtime starts with one valid account exposing both test models.
        // runtime 从一个公开两个测试模型的有效账号开始。
        let runtime = configured_runtime();
        let mut configuration = runtime
            .configuration_snapshot()
            .expect("configuration should remain readable");
        // staleConnection represents another grouped credential that no longer exposes the active model.
        // staleConnection 表示同一分组中已不再公开活动模型的另一份凭据。
        let stale_connection = CredentialConnection {
            id: String::from("openrouter-stale-account"),
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential-openrouter-stale-account"),
            display_name: String::from("OpenRouter stale account"),
            secret_reference: String::from("secret-openrouter-stale-account"),
            management_secret_reference: None,
            service_origin_override: None,
        };
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                stale_connection.clone(),
            )
            .expect("stale connection should register");
        configuration
            .connection_graph
            .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "openrouter-account")
            .expect("valid model account preference should persist");
        configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: stale_connection.id.clone(),
        });
        // Signed OpenRouter declarations always enter account catalogs, so the account access policy
        // is the authority that withdraws the globally selected model from the stale credential.
        // 受签名的 OpenRouter 声明总会进入账号目录，因此由账号访问策略从失效凭据撤回全局所选模型。
        configuration.set_connection_model_enabled(
            stale_connection.id.as_str(),
            "openai/gpt-5.6-sol",
            false,
        );
        runtime
            .replace_configuration(configuration.clone())
            .expect("invalid global account fixture should install");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("account-scoped catalogs should publish");

        let selection = crate::public_ai_workbench_selection(&configuration, &runtime, None, None)
            .expect("new-session selection should resolve")
            .expect("same model should remain available");

        assert_eq!(selection.route.connection.provider_id, "openrouter");
        assert_eq!(selection.model.id, "openai/gpt-5.6-sol");
        assert_eq!(selection.route.connection.id, "openrouter-account");
    }

    /// Verifies an unchanged per-submit model snapshot preserves the session route lease.
    /// 验证逐次提交中未变化的模型快照会保留会话路由租约。
    #[test]
    fn repeated_submission_selection_preserves_lease_until_target_changes() {
        let runtime = lease_discovery_runtime();
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
            provider_id: String::from(LEASE_DISCOVERY_PROVIDER),
            model_id: String::from("discovered-second"),
            connection_id: String::from(LEASE_DISCOVERY_ACCOUNT),
        };
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            PromptSessionMaterializationKind::Created,
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
            PromptSessionMaterializationKind::Created,
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
        runtime
            .stage_source_catalog_for_testing(lease_discovery_catalog(2, &["discovered-first"]))
            .expect("replacement source catalog should stage");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("replacement generation should publish");
        let replacement_catalogs = runtime
            .catalog_snapshot()
            .expect("replacement catalogs should remain readable");
        assert!(
            replacement_catalogs
                .get(LEASE_DISCOVERY_ACCOUNT)
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
            None,
            &runtime,
            Some(&workbench_selection),
        )
        .expect("lease-aware provider rows should project");
        assert!(
            rows.iter()
                .any(|row| row.catalog_models.iter().any(|model| {
                    model.id == selection.model_id
                        && model.active
                        && !model.preferred_connection
                        && model.status.is_some()
                })),
            "the leased model must remain visible only as a selected, non-preferred session row"
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
            PromptSessionMaterializationKind::Created,
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
            provider_id: String::from(LEASE_DISCOVERY_PROVIDER),
            model_id: String::from("discovered-first"),
            connection_id: String::from(LEASE_DISCOVERY_ACCOUNT),
        };
        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            PromptSessionMaterializationKind::Created,
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

    /// Verifies an ordinary submission adopts a revised installed route after restart without weakening hot leases.
    /// 验证普通提交在重启后采用已修订的安装路由，同时不削弱进程内热租约。
    #[test]
    fn unchanged_submission_adopts_revised_route_after_restart() {
        let runtime = configured_runtime();
        let session_id = Uuid::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "Restarted conversation",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: Vec::new(),
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
            PromptSessionMaterializationKind::Created,
            Some(&selection),
        )
        .expect("initial selection should apply");
        let configuration = runtime
            .configuration_snapshot()
            .expect("configuration should remain readable");
        let previous_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                selection.connection_id.as_str(),
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            )
            .expect("previous route should resolve");
        let previous_identity =
            vulcan_agent_runtime::agent_runtime::provider::provider_route_lease_identity(
                &previous_selection,
            );
        workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .expect("session runtime state should exist")
            .provider_route_lease = Some(previous_identity);

        // revisedCatalog keeps the exact model owner and protocol while changing an execution capability.
        // revisedCatalog 保持完全相同的模型所有者与协议，同时修改一项执行能力。
        let revised_catalog = with_protocol_variants(
            parse_openrouter_model_catalog(
                "openrouter-account",
                2,
                r#"{"data":[{"id":"openai/gpt-5.6-terra","name":"Previous","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},{"id":"openai/gpt-5.6-sol","name":"Submitted","context_length":16384,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
            )
            .expect("revised catalog should parse"),
            &[vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID],
        );
        runtime
            .stage_source_catalog_for_testing(revised_catalog)
            .expect("revised source catalog should stage");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("revised generation should publish");
        let revised_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                selection.connection_id.as_str(),
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            )
            .expect("revised route should resolve");
        assert_ne!(
            previous_selection.route_revision, revised_selection.route_revision,
            "the fixture must change only the installed execution revision"
        );
        assert!(
            runtime
                .session_selection_lease(session_id.to_string().as_str())
                .expect("process-local lease should remain readable")
                .is_none(),
            "restart simulation must not retain a process-local route snapshot"
        );

        apply_submission_model_selection(
            &runtime,
            &mut workspace_snapshot,
            session_id,
            PromptSessionMaterializationKind::Created,
            Some(&selection),
        )
        .expect("ordinary submission should adopt the revised installed route");
        assert!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.provider_route_lease.as_ref())
                .is_none(),
            "the obsolete persisted identity must clear before execution preparation"
        );
        let blueprint =
            vulcan_agent_runtime::agent_runtime::provider::resolve_public_ai_provider_blueprint(
                &runtime,
                session_id.to_string().as_str(),
                None,
                vulcan_agent_runtime::provider_config::ReasoningEffortLevel::Off,
                Some(selection.provider_id.as_str()),
                Some(selection.model_id.as_str()),
                Some(selection.connection_id.as_str()),
            )
            .expect("execution preparation should lease the revised route without reselection");
        assert_eq!(
            blueprint.route_revision, revised_selection.route_revision,
            "the first post-restart ordinary submission must use the installed route revision"
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

/// User-authored composer fields carried by a recovery submission on a problem turn.
/// 问题回合恢复提交所携带的用户输入区字段。
#[derive(Debug, Clone)]
struct ContinuationUserSubmission {
    /// User-visible supplemental prompt.
    /// 用户可见的补充提示词。
    prompt: String,
    /// Optional model-only host guidance appended only to Provider-visible text.
    /// 仅追加到 Provider 可见文本的可选模型专用宿主引导。
    hidden_model_guidance: Option<String>,
    /// Optional execution mode selected for the recovered run.
    /// 为恢复执行选择的可选执行模式。
    execution_mode: Option<String>,
    /// Optional reasoning effort selected for the recovered run.
    /// 为恢复执行选择的可选推理强度。
    reasoning_effort: Option<String>,
    /// Exact composer model snapshot, validated without changing the frozen route.
    /// 不修改冻结路由即可校验的精确输入区模型快照。
    model_selection: Option<SubmissionModelSelection>,
    /// Composer attachments retained for explicit unsupported-input rejection.
    /// 为显式拒绝不支持输入而保留的输入区附件。
    attachments: Option<Vec<ComposerAttachmentSummary>>,
}

/// Returns whether one settled Continue terminal may be acknowledged without an RPC error.
/// 判断一个已结算 Continue 终态是否可以在不返回 RPC 错误的情况下确认。
///
/// # Parameters
/// 参数
/// `error` is the exact V1 dispatch settlement returned after admission.
/// `error` 是准入后返回的精确 V1 分派结算结果。
///
/// # Returns
/// 返回值
/// `true` only for a durable inline notice or an expected terminal that requires no notice.
/// 仅当已存在可靠行内提示，或该预期终态无需提示时返回 `true`。
fn continuation_terminal_allows_rpc_ack(error: &SessionRuntimeError) -> bool {
    // projection is carried by the exact durable terminal resolution; transport state alone is insufficient.
    // projection 由精确可靠终态结算携带；仅有传输状态不足以作出判断。
    let projection = match error {
        SessionRuntimeError::ContinuationTerminatedBeforeApi { projection, .. }
        | SessionRuntimeError::ContinuationTerminatedBeforeTransport { projection, .. } => {
            projection
        }
        _ => return false,
    };
    !matches!(projection, ContinuationTerminalProjection::MissingNotice)
}

#[cfg(test)]
mod continuation_terminal_projection_tests {
    use super::*;
    use vulcan_session_core::v1::{ApiCallId, Seq};

    /// A failed terminal without a durable inline notice remains an RPC error.
    /// 缺少可靠行内提示的失败终态仍然作为 RPC 错误返回。
    #[test]
    fn missing_terminal_notice_does_not_allow_rpc_ack() {
        // error models a historical failed API terminal whose inline DisplayNote never became durable.
        // error 模拟行内 DisplayNote 从未可靠落盘的历史 API 失败终态。
        let error = SessionRuntimeError::ContinuationTerminatedBeforeTransport {
            client_action_id: ClientActionId::try_new("continue-missing-notice")
                .expect("valid action id"),
            api_call_id: ApiCallId::try_new(1).expect("valid API id"),
            projection: ContinuationTerminalProjection::MissingNotice,
        };

        assert!(!continuation_terminal_allows_rpc_ack(&error));
    }

    /// A durable inline terminal notice suppresses the duplicate RPC error.
    /// 可靠的行内终态提示会抑制重复 RPC 错误。
    #[test]
    fn inline_terminal_notice_allows_rpc_ack() {
        // error carries the exact durable DisplayNote sequence owned by the pre-API terminal.
        // error 携带 API 前终态所拥有的精确可靠 DisplayNote 序号。
        let error = SessionRuntimeError::ContinuationTerminatedBeforeApi {
            client_action_id: ClientActionId::try_new("continue-inline-notice")
                .expect("valid action id"),
            projection: ContinuationTerminalProjection::InlineNotice {
                notice_seq: Seq::try_new(3).expect("valid notice seq"),
            },
        };

        assert!(continuation_terminal_allows_rpc_ack(&error));
    }

    /// An expected silent terminal remains an acknowledgement without inventing an error notice.
    /// 预期静默终态继续返回确认，并且不会虚构错误提示。
    #[test]
    fn expected_silent_terminal_allows_rpc_ack() {
        // error models an expected cancellation or interruption that requires no error notice.
        // error 模拟无需错误提示的预期取消或中断。
        let error = SessionRuntimeError::ContinuationTerminatedBeforeTransport {
            client_action_id: ClientActionId::try_new("continue-silent-terminal")
                .expect("valid action id"),
            api_call_id: ApiCallId::try_new(2).expect("valid API id"),
            projection: ContinuationTerminalProjection::NoNoticeRequired,
        };

        assert!(continuation_terminal_allows_rpc_ack(&error));
    }
}

/// Claims one frozen continuation and starts the exact retained execution without user input.
/// 领取一份冻结 Continue，并在不追加用户输入的情况下启动精确保留执行。
// Submission admission, media ownership and execution options remain explicit across durable materialization.
// 持久化提交边界显式保留准入、媒体所有权与执行选项。
#[expect(
    clippy::too_many_arguments,
    reason = "Submission admission, media ownership and execution options remain explicit across durable materialization."
)]
pub(super) async fn service_continue_conversation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    client_continuation_id: String,
    prompt: Option<String>,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    model_selection: Option<SubmissionModelSelection>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
) -> Result<SubmitConversationPromptAck, String> {
    let has_supplement = prompt.is_some()
        || hidden_model_guidance.is_some()
        || execution_mode.is_some()
        || reasoning_effort.is_some()
        || model_selection.is_some()
        || attachments.is_some();
    let user_submission = has_supplement.then_some(ContinuationUserSubmission {
        prompt: prompt.unwrap_or_default(),
        hidden_model_guidance,
        execution_mode,
        reasoning_effort,
        model_selection,
        attachments,
    });
    service_continue_conversation_with_submission(
        app,
        state,
        conversation_id,
        client_continuation_id,
        user_submission,
    )
    .await
}

/// Claims one frozen continuation and starts its retained execution with an optional durable user follow-up.
/// 领取一份冻结 Continue，并使用可选可靠用户补充消息启动其保留执行。
///
/// # Parameters
/// 参数
/// `app` emits the accepted continuation state and subsequent runtime deltas.
/// `app` 发出已接受的 Continue 状态与后续运行时增量。
/// `state` owns the retained workspace and canonical conversation state.
/// `state` 持有保留的工作区与规范会话状态。
/// `conversation_id` identifies the terminated WebUI conversation.
/// `conversation_id` 标识已终止的 WebUI 会话。
/// `client_continuation_id` is the stable idempotency identity for one UI recovery intent.
/// `client_continuation_id` 是一次 UI 恢复意图使用的稳定幂等标识。
/// `user_submission` optionally supplies composer text and selections that must be reconciled into the recovery.
/// `user_submission` 可选提供必须协调进恢复流程的输入区文本与选择项。
///
/// # Returns
/// 返回值
/// A lightweight acknowledgement after the retained provider state is admitted for continuation.
/// 保留的 Provider 状态获准继续后的轻量确认信息。
async fn service_continue_conversation_with_submission(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    client_continuation_id: String,
    user_submission: Option<ContinuationUserSubmission>,
) -> Result<SubmitConversationPromptAck, String> {
    let client_continuation_id = client_continuation_id.trim().to_string();
    let continuation_action_id = ClientActionId::try_new(client_continuation_id.clone())
        .map_err(|error| format!("invalid Continue client action id: {error}"))?;
    let session_id = parse_session_id(&conversation_id)
        .ok_or_else(|| String::from("ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION"))?;
    // handle is loaded before process admission so every concurrent replay shares one exact
    // session-local dispatch gate and one durable V1 authority.
    // handle 在进程准入前加载，使每个并发重放共享同一个精确会话级分派门与同一个可靠 V1 权威。
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, session_id).await?;
    let dispatch_guard = handle.acquire_continuation_dispatch().await;
    match handle.reconcile_continuation_dispatch_for_action(&continuation_action_id) {
        Ok(Some(_)) => {
            return continuation_replay_ack_from_state(&state, session_id, client_continuation_id);
        }
        Ok(None) => {}
        Err(error) if continuation_terminal_allows_rpc_ack(&error) => {
            // The accepted terminal either owns a durable inline notice or explicitly requires no error notice.
            // 已接纳终态要么拥有可靠行内提示，要么明确无需错误提示。
            return continuation_replay_ack_from_state(&state, session_id, client_continuation_id);
        }
        Err(error) => return Err(error.to_string()),
    }
    let (
        prompt,
        hidden_model_guidance,
        execution_mode,
        reasoning_effort,
        model_selection,
        attachments,
    ) = match user_submission {
        Some(submission) => (
            Some(submission.prompt),
            submission.hidden_model_guidance,
            submission.execution_mode,
            submission.reasoning_effort,
            submission.model_selection,
            submission.attachments,
        ),
        None => (None, None, None, None, None, None),
    };
    let acknowledgement = service_start_conversation_execution(
        app,
        state,
        client_continuation_id,
        None,
        Some(conversation_id),
        None,
        prompt,
        hidden_model_guidance,
        execution_mode,
        reasoning_effort,
        model_selection,
        None,
        attachments,
        None,
        ExistingSessionHydrationPolicy::TrustRetainedSnapshot,
        ConversationSubmissionMode::Continuation,
        Some(dispatch_guard),
        SubmissionActivation::Foreground,
    )
    .await?;
    if let Err(error) = handle
        .wait_for_continuation_transport_observed(
            &continuation_action_id,
            std::time::Duration::from_secs(120),
        )
        .await
        && !continuation_terminal_allows_rpc_ack(&error)
    {
        return Err(error.to_string());
    }
    Ok(acknowledgement)
}

/// Routes one composer submission through ordinary admission or exact problem-turn recovery.
/// 将一条输入区提交路由到普通准入或精确的问题回合恢复流程。
///
/// # Parameters
/// 参数
/// `app` emits accepted workbench changes and `state` owns authoritative runtime state.
/// `app` 发出已接受的工作台变更，`state` 持有权威运行态。
/// `client_submission_id`, project/conversation ids, and `prompt` identify the authored submission.
/// `client_submission_id`、项目/会话标识与 `prompt` 标识用户提交。
/// `hidden_model_guidance`, execution options, model selection, session settings, and attachments carry the exact composer snapshot.
/// `hidden_model_guidance`、执行选项、模型选择、会话设置与附件携带精确输入区快照。
/// `continuation_expectation` binds the submission to the selected problem turn when recovery is required.
/// `continuation_expectation` 在需要恢复时将提交绑定到选中的问题回合。
///
/// # Returns
/// 返回值
/// A lightweight acknowledgement after ordinary or recovery admission, or an exact validation error.
/// 普通或恢复准入后返回轻量确认，或返回精确校验错误。
#[allow(clippy::too_many_arguments)]
pub(super) async fn service_submit_conversation_prompt_rpc(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    client_submission_id: String,
    project_id: Option<String>,
    conversation_id: Option<String>,
    draft_slot_key: String,
    prompt: String,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    model_selection: Option<SubmissionModelSelection>,
    session_settings: Option<SubmissionSessionSettings>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
) -> Result<SubmitConversationPromptAck, String> {
    service_submit_conversation_prompt(
        app,
        state,
        client_submission_id,
        project_id,
        conversation_id,
        Some(draft_slot_key),
        prompt,
        hidden_model_guidance,
        execution_mode,
        reasoning_effort,
        model_selection,
        session_settings,
        attachments,
        None,
        ExistingSessionHydrationPolicy::HydrateIfEmpty,
        ConversationSubmissionMode::UserPrompt,
        SubmissionActivation::Foreground,
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
    draft_slot_key: Option<String>,
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
    activation: SubmissionActivation,
) -> Result<SubmitConversationPromptAck, String> {
    service_start_conversation_execution(
        app,
        state,
        client_submission_id,
        project_id,
        conversation_id,
        draft_slot_key,
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
        None,
        activation,
    )
    .await
}

/// Converts a post-Hook-start submission failure into one terminal, persisted cleanup result.
/// 将 Hook 启动后的提交失败转换为一个终态化且已持久化的清理结果。
///
/// # Parameters
/// # 参数
/// `app`, `state`, `controller_runtime`, and `agent_runtime` provide the exact projection, ownership, persistence, and Hook boundaries.
/// `app`、`state`、`controller_runtime` 与 `agent_runtime` 提供精确投影、所有权、持久化与 Hook 边界。
/// `session_id` and `preparation_turn_id` identify the started logical prompt turn, while `failure` is the original controlled diagnostic.
/// `session_id` 与 `preparation_turn_id` 标识已启动的逻辑提示词回合，`failure` 是原始受控诊断。
///
/// # Returns
/// # 返回值
/// The original failure after successful cleanup, or a combined diagnostic when cleanup also fails.
/// 清理成功时返回原始失败；清理也失败时返回组合诊断。
async fn cleanup_submission_after_prompt_turn_start(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &super::AgentRuntimeBridge,
    session_id: SessionId,
    preparation_turn_id: &str,
    failure: String,
) -> String {
    // cleanup_error is retained only when terminal projection or persistence could not finish.
    // cleanup_error 仅在终态投影或持久化无法完成时保留。
    let cleanup_error = mark_webui_prompt_preparation_failed(
        app,
        state,
        agent_runtime,
        session_id,
        preparation_turn_id,
        &failure,
    )
    .await
    .err();
    cleanup_error.map_or(failure.clone(), |cleanup_error| {
        format!("{failure}; post-Hook-start cleanup failed: {cleanup_error}")
    })
}

/// Exact V1 authority required before routing an existing-session submission.
/// 路由已有会话提交前必须取得的精确 V1 权威。
struct ExistingSubmissionV1Authority {
    /// Unique process-local owner loaded through the SQLite locator.
    /// 通过 SQLite 定位器加载的唯一进程内 owner。
    handle: Arc<SessionHandle>,
    /// Exact project identity read from the same indexed locator.
    /// 从同一索引定位器读取的精确项目身份。
    project_id: String,
}

/// Loads one existing V1 session and its project identity through the exact SQLite locator.
/// 通过精确 SQLite 定位器加载一份已有 V1 会话及其项目身份。
///
/// # Parameters
/// 参数
/// `state` owns the unique process registry, `controller_runtime` owns the indexed data root,
/// and `session_id` selects the exact existing session.
/// `state` 持有唯一进程注册表；`controller_runtime` 持有索引数据根；`session_id`
/// 选择精确已有会话。
///
/// # Returns
/// 返回值
/// Returns the unique handle plus the SQLite project id after verifying that the loaded
/// metadata belongs to the same project, or an explicit locator/load mismatch.
/// 在核验已加载 metadata 属于同一项目后返回唯一句柄与 SQLite 项目 ID，否则返回明确的
/// 定位或加载不一致错误。
async fn load_existing_submission_v1_authority(
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    session_id: SessionId,
) -> Result<ExistingSubmissionV1Authority, String> {
    // locator is queried explicitly because the queue must bind resource registration to SQLite's
    // project identity rather than infer it from a workspace path or legacy snapshot.
    // 显式查询 locator，是因为队列必须把资源注册绑定到 SQLite 项目身份，而不是从工作区路径
    // 或旧快照推断。
    let locator = locate_session_v1_storage(controller_runtime, &session_id.to_string())
        .await
        .map_err(|error| format!("failed to locate existing V1 session `{session_id}`: {error}"))?;
    let project_id = locator.project_id().to_string();
    let session_registry = {
        let guard = state.lock().map_err(|error| {
            format!("failed to lock service state for existing V1 session load: {error}")
        })?;
        Arc::clone(&guard.session_runtime_registry)
    };
    let handle = get_or_load_session_v1(controller_runtime, session_registry.as_ref(), session_id)
        .await
        .map_err(|error| error.to_string())?;
    let metadata_project_id = handle
        .read(|document| document.metadata.project_tag.clone())
        .map_err(|error| format!("failed to read existing V1 session metadata: {error}"))?;
    if metadata_project_id != project_id {
        return Err(format!(
            "existing V1 session `{session_id}` project mismatch: locator=`{project_id}` metadata=`{metadata_project_id}`"
        ));
    }
    Ok(ExistingSubmissionV1Authority { handle, project_id })
}

/// Publishes or exactly loads the V1 session owner at the sole prompt-materialization boundary.
/// 在唯一提示词会话物化边界发布或精确加载 V1 会话 owner。
///
/// # Parameters
/// 参数
/// `state` owns the process-local session and project-resource registries, `controller_runtime`
/// owns the exact data root and SQLite locator, `materialized_session` contains the authoritative
/// session/project/title classification, `client_submission_id` is the browser-owned idempotency
/// identity, `root_text` is the accepted first user text, `attachments` are the already resolved
/// host-managed resources, `bound_request_config` is the existing session's accepted next-node
/// route, and `now_ms` is the root publication time.
/// `state` 持有进程内会话与项目资源注册表，`controller_runtime` 持有精确数据根与 SQLite
/// 定位器，`materialized_session` 包含权威会话/项目/标题分类，`client_submission_id` 是浏览器
/// 持有的幂等身份，`root_text` 是已接纳的首条用户正文，`attachments` 是已经解析的宿主管理资源，
/// `bound_request_config` 是已有会话接纳的下一节点路由，`now_ms` 是根发布时刻。
///
/// # Returns
/// 返回值
/// Returns only after a created FreshUninitialized root is fully durable and registered, or after
/// an existing session has been located through SQLite and loaded into its unique registry slot.
/// 仅在新建的 FreshUninitialized 根完整落盘并注册后，或已有会话通过 SQLite 定位并加载到
/// 唯一注册槽后返回。
/// Explicit Continue returns its durable receipt; other submission modes return no continuation receipt.
/// 显式继续返回其可靠回执；其他提交模式不返回继续回执。
// Submission admission, media ownership and execution options remain explicit across durable materialization.
// 持久化提交边界显式保留准入、媒体所有权与执行选项。
#[expect(
    clippy::too_many_arguments,
    reason = "Submission admission, media ownership and execution options remain explicit across durable materialization."
)]
async fn materialize_submission_session_v1(
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    materialized_session: &MaterializedPromptSession,
    client_submission_id: &str,
    root_text: &str,
    attachments: &[vulcan_core::types::ConversationAttachment],
    media_origins: &[PromptAttachmentOrigin],
    accepted_input: Option<&ClientInputSubmission>,
    hidden_model_guidance: Option<&str>,
    submission_mode: &ConversationSubmissionMode,
    existing_authority: Option<&ExistingSubmissionV1Authority>,
    turn_admission: &SubmissionTurnAdmission,
    bound_request_config: Option<&ActiveRequestConfig>,
    new_session_context_window_node_id: Option<vulcan_context::ContextWindowSelection>,
    dedicated: Option<crate::session_v1_lifecycle::DedicatedSessionCreation>,
    now_ms: i64,
) -> Result<Option<vulcan_session_core::v1::document::ContinueTurnReceipt>, String> {
    // clientActionId preserves the browser-owned durable idempotency identity
    // without deriving a replacement from the session or current time.
    // clientActionId 原样保留浏览器持有的可靠幂等身份，不从会话或当前时间派生替代值。
    let client_action_id = ClientActionId::try_new(client_submission_id.to_string())
        .map_err(|error| format!("invalid client submission id for V1 materialization: {error}"))?;
    // registries are cloned under the service lock and then used without
    // retaining that coarse lock across file or SQLite I/O.
    // registries 在服务锁内克隆，随后执行文件或 SQLite I/O 时不保留该粗粒度锁。
    let (session_registry, resource_registry) = {
        let guard = state.lock().map_err(|error| {
            format!("failed to lock service state for V1 materialization: {error}")
        })?;
        (
            Arc::clone(&guard.session_runtime_registry),
            Arc::clone(&guard.project_resource_catalog_registry),
        )
    };
    match materialized_session.kind {
        PromptSessionMaterializationKind::Created => {
            if submission_mode.is_continuation() {
                return Err(String::from(
                    "ERR_CONVERSATION_CONTINUE_CANNOT_CREATE_SESSION",
                ));
            }
            // project_id is mandatory because both the resource catalog and
            // session locator use the exact materialized project partition.
            // project_id 是必填项，因为资源目录与会话定位器都使用精确的已物化项目分区。
            let project_id = materialized_session.project_id.as_deref().ok_or_else(|| {
                format!(
                    "new V1 session {} has no authoritative project id",
                    materialized_session.session_id
                )
            })?;
            // resource_ids are resolved before root publication; permanent
            // project resources may survive a later root failure by contract.
            // resource_ids 在根发布前解析；按契约，永久项目资源可在后续根发布失败后保留。
            let (resource_ids, parameters) = register_project_prompt_attachments_v1(
                controller_runtime,
                resource_registry.as_ref(),
                project_id,
                attachments,
                media_origins,
                accepted_input,
            )?;
            create_and_register_root_session_v1_with_context_window(
                controller_runtime,
                session_registry.as_ref(),
                CreateRootSessionV1Request {
                    dedicated,
                    session_id: materialized_session.session_id,
                    project_id: project_id.to_string(),
                    title: materialized_session.session_title.clone(),
                    root_submission: ClientInputSubmission {
                        parameters,
                        bound_request_config: None,
                        client_action_id,
                        text: root_text.to_string(),
                        resource_ids,
                    },
                    now_ms,
                },
                new_session_context_window_node_id,
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        PromptSessionMaterializationKind::Existing => {
            let authority = existing_authority.ok_or_else(|| {
                format!(
                    "existing V1 session `{}` has no preloaded authority",
                    materialized_session.session_id
                )
            })?;
            if authority.handle.session_id() != materialized_session.session_id {
                return Err(format!(
                    "existing V1 authority mismatch: expected `{}`, actual `{}`",
                    materialized_session.session_id,
                    authority.handle.session_id()
                ));
            }
            let (resource_ids, parameters) = register_project_prompt_attachments_v1(
                controller_runtime,
                resource_registry.as_ref(),
                authority.project_id.as_str(),
                attachments,
                media_origins,
                accepted_input,
            )?;
            match submission_mode {
                ConversationSubmissionMode::UserPrompt => {
                    idle_root_input::submit_root_input_when_idle(
                        state,
                        &authority.handle,
                        turn_admission
                            .kernel_reservation_id(materialized_session)?
                            .ok_or_else(|| {
                                String::from("Existing root input requires submission ownership.")
                            })?,
                        ClientInputSubmission {
                            parameters,
                            bound_request_config: bound_request_config.cloned(),
                            client_action_id,
                            text: root_text.to_string(),
                            resource_ids,
                        },
                        now_ms,
                    )
                    .await?;
                    record_existing_session_request_v1(
                        controller_runtime,
                        materialized_session.session_id,
                        now_ms,
                    )
                    .await;
                }
                ConversationSubmissionMode::Continuation => {
                    // Continuation keeps the same admitted preparation instead of recreating origin from the current model.
                    // 继续输入保留同一准入准备，而非根据当前模型重新创建原身份。
                    let mut continuation =
                        ContinuationRequest::new(client_action_id, root_text, resource_ids)
                            .with_hidden_model_guidance(hidden_model_guidance.map(str::to_string));
                    continuation.submission.parameters = parameters;
                    continuation.submission.bound_request_config = bound_request_config.cloned();
                    let receipt = authority
                        .handle
                        .continue_turn(continuation, now_ms)
                        .map_err(|error| error.to_string())?;
                    record_existing_session_request_v1(
                        controller_runtime,
                        materialized_session.session_id,
                        now_ms,
                    )
                    .await;
                    return Ok(Some(receipt.value));
                }
            }
        }
    }
    Ok(None)
}

/// Records an accepted request on an existing session for last-request ordering.
/// 为按最近请求排序记录既有会话上一次已接受的请求。
///
/// The input is already durable when this runs, so a failed write is logged instead of failing a
/// committed submission; the sidebar order then lags until the next request.
/// 运行到这里时输入已可靠落盘，因此写入失败只记录日志而不让已提交的输入失败；侧栏顺序会滞后到下一次请求。
async fn record_existing_session_request_v1(
    controller_runtime: &HostRuntimeHandle,
    session_id: SessionId,
    now_ms: i64,
) {
    if let Err(error) = vulcan_workspace_store::workspace_store::record_session_request_v1(
        controller_runtime,
        &session_id.to_string(),
        now_ms,
    )
    .await
    {
        tracing::warn!(%session_id, %error, "failed to record the session request time");
    }
}

/// Submits one ordinary user prompt or one revision-bound recovery through the shared conversation pipeline.
/// 通过共享会话流水线提交普通用户提示词或绑定修订号的恢复请求。
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
/// `submission_mode` selects ordinary user input or a revision-bound recovery with an optional durable follow-up.
/// `submission_mode` 选择普通用户输入，或带可选持久补充消息且绑定修订号的恢复请求。
// Submission admission, media ownership and execution options remain explicit across durable materialization.
// 持久化提交边界显式保留准入、媒体所有权与执行选项。
#[expect(
    clippy::too_many_arguments,
    reason = "Submission admission, media ownership and execution options remain explicit across durable materialization."
)]
async fn service_start_conversation_execution(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    client_submission_id: String,
    project_id: Option<String>,
    conversation_id: Option<String>,
    draft_slot_key: Option<String>,
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
    continuation_dispatch_guard: Option<ContinuationDispatchGuard>,
    activation: SubmissionActivation,
) -> Result<SubmitConversationPromptAck, String> {
    // Measures the whole submit RPC body from service entry to final response.
    // 衡量提交 RPC 主体从服务入口到最终返回的整体耗时。
    let submit_total_started_at = current_unix_millis_value();
    let client_submission_id = client_submission_id.trim().to_string();
    if client_submission_id.is_empty() {
        return Err(String::from("clientSubmissionId must not be empty."));
    }
    // continuationActionId is the sole durable identity that binds Continue admission to Provider dispatch.
    // continuationActionId 是把 Continue 准入绑定到 Provider 分派的唯一可靠身份。
    let continuation_action_id = submission_mode
        .is_continuation()
        .then(|| ClientActionId::try_new(client_submission_id.clone()))
        .transpose()
        .map_err(|error| error.to_string())?;
    // attachmentDraftScope is validated once and remains an internal-only owner identity through queueing or acceptance.
    // attachmentDraftScope 只校验一次，并在排队或接纳全程保持为仅内部使用的所有者身份。
    let attachment_draft_scope = draft_slot_key
        .as_deref()
        .map(composer_attachment_draft_scope)
        .transpose()?;
    let prompt = match (&submission_mode, prompt) {
        (ConversationSubmissionMode::UserPrompt, Some(prompt)) => prompt.trim().to_string(),
        (ConversationSubmissionMode::UserPrompt, None) => {
            return Err(String::from("A user submission requires prompt text."));
        }
        (ConversationSubmissionMode::Continuation, prompt) => {
            prompt.unwrap_or_default().trim().to_string()
        }
    };
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
    let attachment_summaries = attachments.unwrap_or_default();
    if prompt.is_empty() && attachment_summaries.is_empty() && !submission_mode.is_continuation() {
        return Err(String::from("Enter a task instruction."));
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
    // A durable terminal settlement left by an earlier stream keeps the exact session closed; one
    // controlled idempotent retry runs before admission so a recovered store can complete it.
    // 早期流留下的可靠终态结算会让精确会话保持关闭；准入前执行一次受控幂等重试，
    // 使恢复后的存储能够完成结算。
    if let Some(session_id) = submission_session_id {
        ensure_session_terminal_settlement_settled(&state, session_id).await?;
    }
    if let Some(draft_slot_key) = draft_slot_key.as_deref() {
        crate::attachments::composer_attachment_draft_scope_for_owner(
            draft_slot_key,
            project_id.as_deref(),
            conversation_id.as_deref(),
        )?;
    }
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
    let (mut workspace_snapshot, workspace_source) = if let Some(workspace_snapshot) =
        retained_workspace_snapshot
    {
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
            "Workspace is still initializing; new sessions do not fall back to loading the previous workspace. Try again later.",
        ));
    } else {
        (
            load_workspace_for_submission(
                &state,
                controller_runtime.clone(),
                submission_session_id,
            )
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
            &state,
            controller_runtime.clone(),
            &mut workspace_snapshot,
            session_id,
            preserved_workspace_run_state.as_ref(),
        )
        .await?;
    } else {
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut workspace_snapshot,
        );
    }
    webui_submit_trace(
        "submit_prompt_workspace_ready",
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} source={workspace_source} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(workspace_load_started_at)
        ),
    );
    // existingAuthority is loaded before attachment resolution so persisted ResourceId references
    // are resolved only through the exact V1 project owner.
    // existingAuthority 在附件解析前加载，确保持久化 ResourceId 引用只通过精确 V1 项目 owner 解析。
    let existing_submission_v1_authority = match submission_session_id {
        Some(session_id) => Some(
            load_existing_submission_v1_authority(&state, &controller_runtime, session_id).await?,
        ),
        None => None,
    };
    let existing_resource_locator = existing_submission_v1_authority
        .as_ref()
        .map(|authority| authority.handle.storage_locator())
        .transpose()
        .map_err(|error| error.to_string())?;
    // Client action identity selects its accepted input even after the composer lease has been promoted or released.
    // 即使输入区租约已经提升或释放，客户端动作身份仍可选择其已接纳输入。
    let accepted_action_id =
        ClientActionId::try_new(client_submission_id.clone()).map_err(|error| error.to_string())?;
    // Read the exact V1 owner; a missing input means a new admission, not a search through current or latest drafts.
    // 读取精确V1所有者；输入缺失表示新准入，不搜索当前或最近草稿。
    let accepted_input = match existing_submission_v1_authority.as_ref() {
        Some(authority) => authority
            .handle
            .read(|document| document.client_input_for_action(&accepted_action_id))
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?,
        None => None,
    };
    // existingBoundRequestConfig is captured only from durable Session V1 authority. Submitted
    // selector echoes are ignored for established sessions because explicit picker RPCs already own
    // pending Meta updates.
    // existingBoundRequestConfig 仅从可靠 Session V1 权威捕获。已有会话会忽略提交时回显的
    // 选择器值，因为显式选择器 RPC 已经唯一负责更新待生效 Meta。
    let existing_bound_request_config = match existing_submission_v1_authority.as_ref() {
        Some(authority) => {
            let bound_from_existing_input = authority
                .handle
                .read(|document| {
                    document
                        .metadata
                        .input_state
                        .queued_inputs
                        .iter()
                        .find(|input| input.client_action_id == accepted_action_id)
                        .and_then(|input| input.bound_request_config.clone())
                        .or_else(|| {
                            document
                                .metadata
                                .input_state
                                .pending_in_turn_inputs
                                .iter()
                                .find(|input| input.client_action_id == accepted_action_id)
                                .and_then(|input| input.bound_request_config.clone())
                        })
                })
                .map_err(|error| error.to_string())?;
            Some(match bound_from_existing_input {
                Some(config) => config,
                None => crate::session_request_config_v1::reconcile_pending_reasoning_for_next_user_boundary_v1(
                    authority.handle.as_ref(),
                    agent_runtime.public_ai_runtime.as_ref(),
                )?,
            })
        }
        None => None,
    };
    let (reasoning_effort, model_selection) = if submission_session_id.is_some() {
        (None, None)
    } else {
        (reasoning_effort, model_selection)
    };
    if accepted_input
        .as_ref()
        .is_some_and(|accepted| accepted.text != prompt)
    {
        return Err(String::from(
            "Client input retry text differs from its originally accepted input.",
        ));
    }
    // attachmentWorkspacePath binds every browser-provided asset id to the loaded authoritative project before any path can be read or persisted.
    // attachmentWorkspacePath 在读取或持久化任何路径前，把每个浏览器提供的资产标识绑定到已加载的权威项目。
    let attachment_workspace_path = composer_attachment_workspace_path_from_snapshot(
        &workspace_snapshot,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let authoritative_attachment_summaries = attachment_summaries
        .into_iter()
        .map(|summary| {
            match summary.project_resource_id {
                Some(resource_id) => {
                    validate_project_resource_attachment_reference(&summary)?;
                    composer_attachment_from_project_resource_v1(
                        existing_resource_locator.as_ref().ok_or_else(|| {
                            String::from(
                                "New sessions cannot submit a project ResourceId before V1 materialization.",
                            )
                        })?,
                        resource_id,
                    ).map(|mut canonical| { canonical.prompt_source = summary.prompt_source; canonical })
                }
                None => resolve_composer_attachment_summary_for_workspace(
                    &attachment_workspace_path,
                    summary,
                ),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    // canonicalAttachments never retain request-local delivery decisions.
    // canonicalAttachments 永远不保留单次请求局部传输决策。
    let attachments = authoritative_attachment_summaries
        .iter()
        .cloned()
        .map(composer_attachment_summary_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    let attachments_for_draft_promotion = attachments.clone();
    // Capture each managed draft's exact lease-bound origin before queue admission or lease promotion can release it.
    // 在队列准入或租约提升可能释放它之前，捕获每个托管草稿精确租约绑定原身份。
    let media_origins = attachments
        .iter()
        .zip(&authoritative_attachment_summaries)
        .map(|(attachment, summary)| {
            if let Some(raw_resource_id) = summary.project_resource_id {
                // The existing project ID stays authoritative; preparation comes from its exact source occurrence only.
                // 既有项目ID保持权威；准备只来自其精确来源出现。
                let resource_id =
                    crate::conversation_resources_v1::checked_resource_id(raw_resource_id)?;
                // Retry proof is restored later from the accepted input; no current source or draft may replace it.
                // 重试证明稍后从已接纳输入恢复；当前来源或草稿不能替代它。
                let preparation = if accepted_input.is_some() {
                    vulcan_session_core::v1::UserPromptResourcePreparation::Unproven
                } else if let Some(source) = &summary.prompt_source {
                    existing_submission_v1_authority
                        .as_ref()
                        .ok_or_else(|| {
                            String::from(
                                "Project input source requires its exact V1 session owner.",
                            )
                        })?
                        .handle
                        .read(|document| {
                            document.user_prompt_resource_preparation(source, resource_id)
                        })
                        .map_err(|error| error.to_string())?
                        .map_err(|error| error.to_string())?
                } else {
                    vulcan_session_core::v1::UserPromptResourcePreparation::Unproven
                };
                Ok(PromptAttachmentOrigin::Project {
                    resource_id,
                    preparation: Box::new(preparation),
                })
            } else {
                // Only fresh staged input may consult its live composer lease. Exact retries intentionally skip this access.
                // 只有新暂存输入可查询其活动输入区租约；精确重试有意跳过此访问。
                let origin = if accepted_input.is_some() {
                    None
                } else if let Some(scope) = attachment_draft_scope.as_deref() {
                    vulcan_agent_runtime::attachment_asset_index::load_attachment_draft_media(
                        attachment, scope,
                    )?
                } else {
                    None
                };
                Ok(PromptAttachmentOrigin::Staged(origin.map(Box::new)))
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    // requestedProviderAttachments carries only a conservative metadata-only reduction; every
    // byte-exposing mode is recomputed from the exact route at the actual dispatch boundary.
    // requestedProviderAttachments 仅携带保守的仅元数据降级；每种暴露字节的模式都会在实际分发边界
    // 根据精确路线重新计算。
    let requested_provider_attachments = authoritative_attachment_summaries
        .into_iter()
        .map(composer_attachment_summary_to_provider_core)
        .collect::<Result<Vec<_>, _>>()?;
    if let ExistingSubmissionQueueDecision::Queue { session_id } =
        evaluate_existing_submission_queue(
            &state,
            &workspace_snapshot,
            submission_session_id,
            existing_submission_v1_authority
                .as_ref()
                .map(|authority| &authority.handle),
            &submission_mode,
        )?
    {
        if submission_mode.is_continuation() {
            return Err(String::from("ERR_CONVERSATION_CONTINUE_SESSION_BUSY"));
        }
        let v1_authority = existing_submission_v1_authority.as_ref().ok_or_else(|| {
            format!("queued existing session `{session_id}` has no exact V1 authority")
        })?;
        let resource_registry = {
            let guard = state.lock().map_err(|error| {
                format!("failed to lock service state for queued V1 resources: {error}")
            })?;
            Arc::clone(&guard.project_resource_catalog_registry)
        };
        // resourceIds preserve the authoritative browser attachment order and are permanent
        // project resources independent of the transient compatibility queue.
        // resourceIds 保留浏览器附件的权威顺序，并作为独立于瞬时兼容队列的永久项目资源。
        let (resource_ids, parameters) = register_project_prompt_attachments_v1(
            &controller_runtime,
            resource_registry.as_ref(),
            v1_authority.project_id.as_str(),
            attachments.as_slice(),
            &media_origins,
            accepted_input.as_ref(),
        )?;
        let client_action_id = ClientActionId::try_new(client_submission_id.clone())
            .map_err(|error| format!("invalid queued client submission id: {error}"))?;
        let bound_request_config = existing_bound_request_config.clone().ok_or_else(|| {
            format!("queued existing session `{session_id}` has no bound request configuration")
        })?;
        let queued_model_selection = SubmissionModelSelection {
            provider_id: bound_request_config.provider.clone().ok_or_else(|| {
                format!("queued existing session `{session_id}` has no active provider")
            })?,
            model_id: bound_request_config.model.clone().ok_or_else(|| {
                format!("queued existing session `{session_id}` has no active model")
            })?,
            connection_id: bound_request_config.connection_id.clone().ok_or_else(|| {
                format!("queued existing session `{session_id}` has no active connection")
            })?,
        };
        let queued_reasoning_effort = Some(
            bound_request_config
                .thinking_level
                .clone()
                .unwrap_or_else(|| String::from("off")),
        );
        let scheduled_handle = Arc::clone(&v1_authority.handle);
        let scheduled_client_action_id = client_action_id.clone();
        let acknowledgement = queue_existing_submission(
            &app,
            &state,
            &agent_runtime,
            ExistingSubmissionQueueInput {
                v1_handle: Arc::clone(&v1_authority.handle),
                v1_submission: ClientInputSubmission {
                    parameters,
                    bound_request_config: Some(bound_request_config),
                    client_action_id,
                    text: prompt,
                    resource_ids,
                },
                session_id,
                workspace_path: attachment_workspace_path,
                hidden_model_guidance,
                attachments: requested_provider_attachments,
                attachment_draft_scope,
                reasoning_effort: queued_reasoning_effort,
                model_selection: Some(queued_model_selection),
                submit_total_started_at,
            },
        )?;
        crate::session_request_config_v1::persist_missing_reasoning_reconciliation_after_user_admission_v1(
            v1_authority.handle.as_ref(),
            agent_runtime.public_ai_runtime.as_ref(),
            crate::current_v1_unix_millis()?,
        )?;
        if acknowledgement.turn_id.is_some() {
            // Goal supersession has already created the exact open UserRoot Turn durably.
            // Goal 替代已经可靠创建精确开放 UserRoot 回合。
            let authority = queued_dispatch_authority_for_client_action(
                scheduled_handle.as_ref(),
                &scheduled_client_action_id,
            )?;
            if !start_exact_v1_queued_turn(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                session_id,
                authority,
            )
            .await?
            {
                return Err(String::from(
                    "The durable scheduled user Turn could not acquire physical dispatch admission; retry the same client submission id.",
                ));
            }
        }
        return Ok(acknowledgement);
    }
    // The provisional legacy request must not upload or inline bytes before prompt-owned V1 preparation.
    // 临时旧请求不能在提示词所有的V1准备之前上传或内联字节。
    let provider_attachments = defer_attachment_delivery_to_v1(&requested_provider_attachments);
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
    let mut materialized_session = materialize_prompt_session(
        &mut workspace_snapshot,
        materialization_project_id,
        submission_target,
        if prompt.is_empty() {
            "Attachment session"
        } else {
            &prompt
        },
    )?;
    materialized_session.activation = activation;
    project_admission.validate_materialized_session(&materialized_session)?;
    // Dedicated choices are validated before the root publication that installs them.
    // 专职选择在安装它们的根发布之前完成校验。
    let dedicated_creation =
        submission_dedicated_creation(session_settings.as_ref(), execution_mode.as_deref())?;
    if dedicated_creation.is_some()
        && materialized_session.kind != PromptSessionMaterializationKind::Created
    {
        return Err(String::from("ERR_DEDICATED_SESSION_NOT_CONVERTIBLE"));
    }
    if dedicated_creation.is_some() {
        // The list entry carries the same kind the root publication installs.
        // 列表项携带与根发布安装一致的会话种类。
        crate::workspace_session_summary_for_session_mut(
            &mut workspace_snapshot,
            materialized_session.session_id,
        )
        .ok_or_else(|| {
            format!(
                "created conversation `{}` is missing from its workspace slice",
                materialized_session.session_id
            )
        })?
        .dedicated = true;
    }
    // Resolve the new-session default against the actual selected model before publishing the root.
    // 在发布根输入前，按实际所选模型解析新会话默认窗口。
    // The selected model also titles the new session when no title route is saved.
    // 未保存标题路由时，所选模型同时用于为新会话生成标题。
    let (new_session_context_window_node_id, new_session_model) =
        if materialized_session.kind == PromptSessionMaterializationKind::Created {
            let runtime = agent_runtime.public_ai_runtime.as_ref();
            let configuration = runtime.configuration_snapshot()?;
            let selected_model = if let Some(selection) = model_selection.as_ref() {
                Some(resolve_enabled_submission_model_selection(
                    runtime,
                    &configuration,
                    selection,
                )?)
            } else {
                crate::public_ai_workbench_selection(&configuration, runtime, None, None)?
            };
            let window = match selected_model.as_ref() {
                Some(selected) => configuration.new_session_context_window_for_model(
                    selected.route.connection.provider_id.as_str(),
                    selected.model.id.as_str(),
                    selected.model.capabilities.context_limit,
                )?,
                None => None,
            };
            (window, selected_model)
        } else {
            (None, None)
        };
    // Bind the initial reasoning level to the exact model resolved for the new session.
    // 将首个思考档位绑定到为新会话解析出的精确模型。
    let reasoning_effort = new_session_model.as_ref().map(|selection| {
        // remembered is a model-scoped default, used only when the new draft supplied no level.
        // remembered 是模型范围默认值，仅在新草稿未指定档位时使用。
        let remembered = agent_runtime.provider_configuration.remembered_model_reasoning_effort(
            &selection.route.provider.id,
            &selection.model.id,
        );
        // effective validates even a draft that was open before the current manifest arrived.
        // effective 会校验当前清单到达前就已打开的草稿。
        let effective = vulcan_agent_runtime::agent_runtime::provider_reasoning::resolve_effective_reasoning_effort(
            &selection.model.capabilities,
            reasoning_effort.as_deref().and_then(crate::parse_reasoning_effort).or(remembered),
        );
        crate::session_request_config_v1::reasoning_effort_level_key(effective)
    }).or(reasoning_effort);
    // model_selection pins the resolved global default as well as an explicit draft model.
    // model_selection 同时固定已解析的全局默认模型及显式草稿模型。
    let model_selection = new_session_model
        .as_ref()
        .map(|selection| SubmissionModelSelection {
            provider_id: selection.route.provider.id.clone(),
            connection_id: selection.route.connection.id.clone(),
            model_id: selection.model.id.clone(),
        })
        .or(model_selection);
    let title_generation_request = (materialized_session.kind
        == PromptSessionMaterializationKind::Created
        && !prompt.is_empty())
    .then(|| crate::session_title::SessionTitleGenerationRequest {
        session_id: materialized_session.session_id,
        initial_title: materialized_session.session_title.clone(),
        prompt: prompt.clone(),
        session_model: new_session_model,
    });
    // V1 materialization is the first durable session boundary after the old
    // selector has fixed session/project/attachment ownership. It intentionally
    // precedes Goal, Plan, output-style, and pending-model mutations.
    // V1 物化是旧选择器确定会话/项目/附件归属后的首个可靠会话边界；它有意先于
    // Goal、Plan、输出风格与待切换模型修改。
    let continuation_receipt = materialize_submission_session_v1(
        &state,
        &controller_runtime,
        &materialized_session,
        client_submission_id.as_str(),
        prompt.as_str(),
        attachments.as_slice(),
        &media_origins,
        accepted_input.as_ref(),
        hidden_model_guidance.as_deref(),
        &submission_mode,
        existing_submission_v1_authority.as_ref(),
        &turn_admission,
        existing_bound_request_config.as_ref(),
        new_session_context_window_node_id,
        dedicated_creation,
        i64::try_from(materialize_started_at).unwrap_or(i64::MAX),
    )
    .await?;
    // These identities remain owned by the live admission guards until preparation or failure settlement completes.
    // 这些身份在准备成功或失败结算完成前始终由存活的准入守卫持有。
    let session_id = materialized_session.session_id;
    let session_title = materialized_session.session_title.as_str();
    let workspace_path = materialized_session.workspace_path.as_str();
    // Pin the admitted logical turn, never a later replacement inferred from a failed request.
    // 固定已准入逻辑回合，绝不根据失败请求推测后续替代回合。
    let admitted_handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(&state, session_id)?;
    let admitted_turn_id = admitted_handle
        .read(|document| document.active_turn().map(|turn| turn.turn_id))
        .map_err(|error| error.to_string())?
        .ok_or_else(|| String::from("Admitted submission has no V1 turn."))?;
    // Every fallible pre-Hook step is settled before the submission reservation can be released.
    // 每个可能失败的 Hook 前步骤都会在提交预留释放前完成结算。
    let preparation_result = async {
        if let Some(authority) = existing_submission_v1_authority.as_ref() {
            crate::session_request_config_v1::persist_missing_reasoning_reconciliation_after_user_admission_v1(
                authority.handle.as_ref(),
                agent_runtime.public_ai_runtime.as_ref(),
                crate::current_v1_unix_millis()?,
            )?;
        }
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
        if materialized_session.kind == PromptSessionMaterializationKind::Created {
            let runtime_state = workspace_snapshot
                .session_runtime_state
                .get_mut(&materialized_session.session_id)
                .ok_or_else(|| String::from("New conversation has no runtime state."))?;
            runtime_state.context_window_selection = new_session_context_window_node_id;
        }
        if submission_mode.is_continuation() {
            validate_recovery_submission_model_selection(
                agent_runtime.public_ai_runtime.as_ref(),
                &workspace_snapshot,
                materialized_session.session_id,
                model_selection.as_ref(),
            )?;
        } else {
            apply_submission_model_selection(
                agent_runtime.public_ai_runtime.as_ref(),
                &mut workspace_snapshot,
                materialized_session.session_id,
                materialized_session.kind,
                model_selection.as_ref(),
            )?;
            // A submitted picker choice must reach the same durable pending configuration as the explicit picker RPC.
            // 提交携带的模型选择必须进入与显式选择 RPC 相同的可靠待生效配置，不能只修改内存。
            if materialized_session.kind == PromptSessionMaterializationKind::Created
                && let Some(selection) = model_selection.as_ref()
            {
                let handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
                    &state,
                    materialized_session.session_id,
                )?;
                let runtime_state = workspace_snapshot
                    .session_runtime_state
                    .get(&materialized_session.session_id)
                    .ok_or_else(|| {
                        format!(
                            "Session `{}` has no admitted runtime state.",
                            materialized_session.session_id
                        )
                    })?;
                crate::session_request_config_v1::set_pending_model_request_v1(
                    handle.as_ref(),
                    selection.provider_id.clone(),
                    selection.connection_id.clone(),
                    selection.model_id.clone(),
                    None,
                    crate::session_request_config_v1::SessionRequestFallback::from_runtime_state(
                        runtime_state,
                    ),
                    crate::current_v1_unix_millis()?,
                )?;
            }
        }
        if session_settings.is_some()
            && materialized_session.kind != PromptSessionMaterializationKind::Created
        {
            return Err(String::from("Draft session settings can only be applied with the first submission of a new session."));
        }
        apply_submission_session_settings(
            &state,
            agent_runtime.public_ai_runtime.as_ref(),
            &mut workspace_snapshot,
            materialized_session.session_id,
            execution_mode.as_deref(),
            session_settings.as_ref(),
            materialized_session.kind == PromptSessionMaterializationKind::Created,
        )
        .await?;
        webui_submit_trace(
            "submit_prompt_starting_run",
            format!(
                "session={session_id} title_chars={}",
                session_title.chars().count()
            ),
        );
        let submit_run_prepare_started_at = current_unix_millis_value();
        let runtime_options = prepare_submission_runtime_options(
            &state,
            &mut workspace_snapshot,
            session_id,
            SubmissionRuntimeOptionsInput {
                execution_mode,
                reasoning_effort,
            },
        )
        .await?;
        let handle =
            crate::session_goal_lifecycle::registered_session_v1_handle_from_state(&state, session_id)?;
        if materialized_session.kind == PromptSessionMaterializationKind::Created {
            // A newly created session stages its first route after root materialization, so that exact
            // root boundary must activate it. Existing inputs already activated their immutable bound
            // config inside Core; consuming a newer picker intent here would rewrite the running node.
            // 新建会话在根输入物化后才暂存首个路由，因此必须由该精确根边界激活。既有输入已经在 Core
            // 内激活其不可变绑定配置；若在此消费更新的选择器意图，就会改写正在运行的节点。
            handle
                .activate_pending_request_for_new_session_root(crate::current_v1_unix_millis()?)
                .map_err(|error| format!("Failed to activate the V1 request config for the new session root boundary: {error}"))?;
        }
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("Session `{session_id}` has no admitted runtime state."))?;
        crate::session_request_config_v1::synchronize_v1_dispatch_model_selection(
            handle.as_ref(),
            agent_runtime.public_ai_runtime.as_ref(),
            runtime_state,
        )?;
        // The admitted V1 root owns the Provider user identity before Hook preparation begins.
        // 已接纳的 V1 根输入在 Hook 准备开始前持有 Provider 用户身份。
        let direct_canonical_user_message_id = if append_user_records {
            let root_input_seq = handle
                .read(|document| document.active_turn().map(|turn| turn.start_seq))
                .map_err(|error| format!("Failed to read the directly submitted V1 root turn: {error}"))?
                .ok_or_else(|| String::from("The directly submitted V1 root turn does not exist."))?;
            let root_authority =
                user_root_dispatch_authority_from_v1_handle(handle.as_ref(), root_input_seq)?;
            if root_authority.client_action_id != accepted_action_id {
                return Err(format!(
                    "Directly submitted V1 root input identity mismatch: expected `{}`, actual `{}`.",
                    accepted_action_id.as_str(),
                    root_authority.client_action_id.as_str()
                ));
            }
            let projection = handle
                .capture_controller_projection_snapshot()
                .map_err(|error| format!("Failed to capture the directly submitted V1 projection: {error}"))?;
            let canonical_user_message_id =
                crate::agent_stream::provider_v1_sink::provider_projection_user_message_id_for_source(
                    &projection,
                    root_input_seq,
                )?;
            let mut projected_state =
                crate::agent_stream::provider_v1_sink::provider_projection_controller_state(
                    &projection,
                )?;
            // Visible transcript rows remain a separate workspace projection; V1 replaces only logical messages.
            // 可见转录记录仍属独立工作区投影；V1 只替换逻辑消息。
            projected_state.records = std::mem::take(&mut runtime_state.conversation_state.records);
            runtime_state.conversation_state = projected_state;
            Some(canonical_user_message_id)
        } else {
            None
        };
        webui_submit_trace(
            "submit_prompt_runtime_options_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(submit_run_prepare_started_at)
            ),
        );
        // vmmReadiness is captured before Hook turn admission and remains immutable for this request.
        // vmmReadiness 在 Hook 回合准入前捕获，并在本次请求期间保持不可变。
        crate::session_business_hook::synchronize_live_business_hook_inputs(
            handle.as_ref(), &agent_runtime,
        )?;
        let vmm_readiness = webui_vmm_runtime_readiness(&state)?;
        let turn_begin_started_at = current_unix_millis_value();
        let precheck_start = {
            // Preserve the actual Continue admission receipt through Hook preparation instead of deriving an input identity.
            // 将实际 Continue 准入回执保留到 Hook 准备，不派生输入身份。
            let continuation_authority = if submission_mode.is_continuation() {
                let receipt = continuation_receipt.as_ref().ok_or_else(|| String::from("Continue admission has no canonical receipt."))?;
                Some(crate::session_business_hook::continuation_hook_authority(
                    handle.as_ref(), &accepted_action_id, receipt,
                )?)
            } else { None };
            let mut hook_runtime = agent_runtime
                .lock_conversation_hooks()
                .map_err(|error| format!("Failed to lock the session hook runtime: {error}"))?;
            let begin_input = SharedPromptTurnBeginInput {
                    session_id,
                    session_title,
                    workspace_path,
                    prompt_entry_id: format!(
                        "webui-prompt-{session_id}-{}",
                        current_unix_millis_text()
                    ),
                    continuation_action_id: continuation_action_id.clone(),
                    prompt: &prompt,
                    hidden_model_guidance: hidden_model_guidance.as_deref(),
                    provider_visible_agent_messages: Vec::new(),
                    attachments,
                    provider_attachments: Some(provider_attachments),
                    existing_canonical_user_message_id: direct_canonical_user_message_id,
                    append_canonical_user_message: false,
                    append_visible_user_prompt_record: append_user_records,
                    source: prompt_source,
                    vmm_runtime_ready: vmm_readiness.service_ready,
                    vmm_precheck_ready: vmm_readiness.precheck_ready,
                    execution_mode: runtime_options.execution_mode,
                    reasoning_effort: runtime_options.reasoning_effort,
                };
            if let Some(authority) = continuation_authority {
                hook_runtime.restore_business_hook_state(&authority.state)?;
                vulcan_agent_runtime::workbench_prompt_turn::begin_shared_prompt_turn_with_canonical_inputs(
                    &mut workspace_snapshot, &mut hook_runtime, begin_input, &authority.inputs,
                )?
            } else {
                begin_shared_prompt_turn(&mut workspace_snapshot, &mut hook_runtime, begin_input)?
            }
        };
        // Publish original Hook ownership before kernel dispatch can launch the first Provider request.
        // 在 kernel 分发能够启动首个 Provider 请求之前发布原 Hook 归属。
        crate::session_business_hook::publish_prepared_business_hook(
            handle.as_ref(),
            &agent_runtime,
            &precheck_start,
        )?;
        Ok::<_, String>((kernel_reservation_id, precheck_start, turn_begin_started_at))
    }.await;
    let (kernel_reservation_id, mut precheck_start, turn_begin_started_at) =
        match preparation_result {
            Ok(prepared) => prepared,
            Err(error) => {
                // A failed first submission still owns a durable user message; publish its session rather than discard it.
                // 首次提交失败仍持有可靠用户消息；发布其会话，而不是丢弃它。
                let publication_result =
                    if materialized_session.kind == PromptSessionMaterializationKind::Created {
                        workspace_snapshot
                            .session_run_phases
                            .insert(session_id, SessionRunPhase::Stopped);
                        for session in &mut workspace_snapshot.sessions {
                            if session.id == session_id {
                                session.status = vulcan_workbench_model::SessionState::Blocked;
                            }
                        }
                        commit_created_submission_workspace_slice(
                            &state,
                            &workspace_snapshot,
                            &materialized_session,
                        )
                        .and_then(|()| {
                            commit_materialized_submission_activation(
                                &app,
                                &state,
                                &workspace_snapshot,
                                &materialized_session,
                            )
                            .map(|_| ())
                        })
                    } else {
                        Ok(())
                    };
                // Settlement is attempted even if the UI publication failed; the canonical error must not disappear.
                // 即使界面发布失败也必须尝试结算，不能丢失规范错误事实。
                let settlement_result = settle_admitted_submission_failure(
                    &app,
                    &state,
                    session_id,
                    admitted_turn_id,
                    &error,
                )
                .await;
                return Err(match (publication_result, settlement_result) {
                    (Ok(()), Ok(())) => error,
                    (publication, settlement) => format!(
                        "{error}; submission failure publication: {publication:?}; submission failure settlement: {settlement:?}"
                    ),
                });
            }
        };
    // Restored transitional preparation cannot own V1 continuation identity; the durable admission does.
    // 恢复的过渡准备态不能拥有 V1 Continue 身份；该身份由可靠准入事实拥有。
    precheck_start.continuation_action_id = continuation_action_id.clone();
    let accepted_message_id = precheck_start.visible_user_prompt_record_id.clone();
    let submission_target_label = if submission_session_id.is_some() {
        "existing"
    } else {
        "new"
    };
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
    let kernel_start_result = match state.lock() {
        Ok(mut guard) => {
            let kernel_turn_started_at = current_unix_millis_value();
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
        }
        Err(error) => Err(format!("failed to lock service state: {error}")),
    };
    if let Err(error) = kernel_start_result {
        let cleanup_error = mark_webui_prompt_preparation_failed(
            &app,
            &state,
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
    let workspace_commit_result =
        if materialized_session.kind == PromptSessionMaterializationKind::Created {
            commit_created_submission_workspace_slice(
                &state,
                &workspace_snapshot,
                &materialized_session,
            )
        } else {
            commit_existing_submission_workspace_slice(
                &state,
                &mut workspace_snapshot,
                &materialized_session,
            )
        };
    if let Err(error) = workspace_commit_result {
        return Err(cleanup_submission_after_prompt_turn_start(
            &app,
            &state,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            error,
        )
        .await);
    }
    webui_submit_trace(
        "submit_prompt_begin_workspace_committed",
        format!(
            "session={session_id} elapsed_ms={} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(begin_commit_started_at),
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    if let Err(error) = commit_materialized_submission_activation(
        &app,
        &state,
        &workspace_snapshot,
        &materialized_session,
    ) {
        return Err(cleanup_submission_after_prompt_turn_start(
            &app,
            &state,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            error,
        )
        .await);
    }
    webui_submit_trace(
        "submit_prompt_visible_user_message_emitted",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    // V1 root/continue admission was flushed before the process-local workspace slice was committed.
    // V1 根输入或 Continue 准入已在进程内工作区切片提交前可靠落盘。
    let revision = match current_workbench_revision_from_state(&state) {
        Ok(revision) => revision,
        Err(error) => {
            return Err(cleanup_submission_after_prompt_turn_start(
                &app,
                &state,
                &agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                error,
            )
            .await);
        }
    };
    let acknowledgement_elapsed_ms =
        current_unix_millis_value().saturating_sub(submit_total_started_at);
    if let Err(error) = retain_slow_submission_diagnostic(
        &state,
        &client_submission_id,
        submission_target_label,
        "acceptance",
        acknowledgement_elapsed_ms,
    ) {
        return Err(cleanup_submission_after_prompt_turn_start(
            &app,
            &state,
            &agent_runtime,
            session_id,
            &precheck_start.preparation_turn_id,
            error,
        )
        .await);
    }
    webui_submit_trace(
        "submit_prompt_returning",
        format!(
            "session={session_id} client_submission_id={client_submission_id} elapsed_ms_since_received={acknowledgement_elapsed_ms}"
        ),
    );
    let accepted_client_submission_id = client_submission_id.clone();
    if let Err(error) = promote_submission_attachment_draft_scope(
        workspace_path,
        session_id,
        precheck_start.canonical_user_message_id.as_deref(),
        attachments_for_draft_promotion.as_slice(),
        attachment_draft_scope.as_deref(),
    ) {
        // The accepted canonical message remains authoritative; retaining the draft lease is safer than duplicating the user turn.
        // 已接纳的规范消息保持权威；保留草稿租约比重复用户回合更安全。
        webui_submit_trace(
            "composer_attachment_draft_lease_promotion_failed",
            format!("session={session_id} error={error}"),
        );
    }
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
    let workspace_path = workspace_path.to_string();
    let app_for_preparation = app.clone();
    let state_for_preparation = state.clone();
    let controller_runtime_for_preparation = controller_runtime.clone();
    let agent_runtime_for_preparation = agent_runtime.clone();
    let continuation_dispatch_owner = match continuation_dispatch_guard {
        Some(dispatch_guard) => {
            let action_id = continuation_action_id.clone().ok_or_else(|| {
                String::from("Continue dispatch guard has no durable ClientActionId")
            })?;
            let handle = existing_submission_v1_authority
                .as_ref()
                .map(|authority| Arc::clone(&authority.handle))
                .ok_or_else(|| String::from("Continue dispatch guard has no exact V1 handle"))?;
            Some((dispatch_guard, handle, action_id))
        }
        None => None,
    };
    spawn_service_task(async move {
        let preparation_started_at = current_unix_millis_value();
        let preparation_result = (|| -> Result<bool, String> {
            // capturedPreparationOwner clones the workspace only after acknowledgement and binds every later write to the accepted kernel generation.
            // capturedPreparationOwner 仅在确认后克隆工作区，并把所有后续写入绑定到已接纳的 kernel 代际。
            let Some((workspace_snapshot, expected_preparation_generation)) =
                capture_webui_prompt_preparation_workspace(
                    &state_for_preparation,
                    session_id,
                    &preparation_turn_id,
                )?
            else {
                webui_submit_trace(
                    "submit_prompt_preparation_owner_closed",
                    format!("session={session_id} turn_id={preparation_turn_id}"),
                );
                return Ok(false);
            };
            let precheck_plan_started_at = current_unix_millis_value();
            let precheck_plan = if preparation_prompt_source.runs_user_prompt_precheck() {
                webui_shared_prompt_precheck_plan(
                    &agent_runtime_for_preparation,
                    &workspace_snapshot,
                    session_id,
                    &workspace_path,
                    precheck_start.vmm_precheck_ready,
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
            let hook_snapshot_started_at = current_unix_millis_value();
            if precheck_start.vmm_runtime_ready {
                refresh_exact_vmm_hook_session_projection(
                    &agent_runtime_for_preparation.vmm_hook_runtime,
                    &workspace_snapshot,
                    session_id,
                )?;
            }
            webui_submit_trace(
                "submit_prompt_hook_snapshot_ready",
                format!(
                    "session={session_id} elapsed_ms={}",
                    current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
                ),
            );
            // currentPreparationSnapshot revalidates the immutable kernel owner and reads the latest retained UI domain without publishing the detached read-only draft.
            // currentPreparationSnapshot 重新校验不可变 kernel 所有者，并读取最新保留 UI 域，不发布分离的只读草稿。
            let Some(current_preparation_generation) = capture_webui_prompt_preparation_generation(
                &state_for_preparation,
                session_id,
                &preparation_turn_id,
            )?
            else {
                webui_submit_trace(
                    "submit_prompt_preparation_commit_skipped_stale_owner",
                    format!(
                        "session={session_id} generation={expected_preparation_generation} turn_id={preparation_turn_id}"
                    ),
                );
                return Ok(false);
            };
            if current_preparation_generation != expected_preparation_generation {
                webui_submit_trace(
                    "submit_prompt_preparation_generation_replaced",
                    format!(
                        "session={session_id} expected_generation={expected_preparation_generation} current_generation={current_preparation_generation} turn_id={preparation_turn_id}"
                    ),
                );
                return Ok(false);
            }
            // The admitted V1 input and preparation owner are already durable; no legacy workspace snapshot is published.
            // 已准入的 V1 输入与准备所有者已经可靠落盘；此处不再发布旧 workspace 快照。
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
            Ok(precheck_started)
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
            tracing::warn!("[webui-service] failed to retain slow submission diagnostic: {error}");
        }
        if let Err(error) = &preparation_result {
            tracing::warn!("[webui-service] accepted submission preparation failed: {error}");
            if let Err(cleanup_error) = mark_webui_prompt_preparation_failed(
                &app_for_preparation,
                &state_for_preparation,
                &agent_runtime_for_preparation,
                session_id,
                &preparation_turn_id,
                error,
            )
            .await
            {
                tracing::warn!(
                    "[webui-service] accepted submission preparation cleanup failed: {cleanup_error}"
                );
            }
        }
        if let Some((_dispatch_guard, handle, action_id)) = continuation_dispatch_owner
            && preparation_result.as_ref().is_ok_and(|started| *started)
            && let Err(error) = handle
                .wait_until_continuation_dispatch_settled(&action_id)
                .await
        {
            tracing::warn!(
                "[webui-service] Continue dispatch settled before transport observation: {error}"
            );
        }
    });
    if let Some(request) = title_generation_request {
        crate::session_title::spawn_session_title_generation(app, state, request);
    }
    Ok(acknowledgement)
}

/// Promotes one accepted composer draft lease into the canonical user-message owner.
/// 将一条已接纳输入区草稿租约提升为规范用户消息所有者。
///
/// # Parameters
/// 参数
/// `workspace_path` selects the project asset index and `session_id`/`message_id` identify the durable owner.
/// `workspace_path` 选择项目资产索引，`session_id` 与 `message_id` 标识持久所有者。
/// `attachments` contains canonical staged assets and `draft_scope` is their optional transient owner.
/// `attachments` 包含规范暂存资产，`draft_scope` 是其可选瞬时所有者。
///
/// # Returns
/// 返回值
/// Success after atomic promotion, or one explicit missing-owner/index error.
/// 原子提升成功后返回成功，或返回明确的所有者/索引缺失错误。
pub(super) fn promote_submission_attachment_draft_scope(
    workspace_path: &str,
    session_id: SessionId,
    message_id: Option<&str>,
    attachments: &[vulcan_core::types::ConversationAttachment],
    draft_scope: Option<&str>,
) -> Result<(), String> {
    let Some(draft_scope) = draft_scope.filter(|scope| !scope.trim().is_empty()) else {
        return Ok(());
    };
    if attachments.is_empty() {
        return Ok(());
    }
    let message_id = message_id.ok_or_else(|| {
        String::from("Accepted attachment draft lease has no canonical user message owner.")
    })?;
    promote_staged_attachment_to_message_ref(
        workspace_path,
        session_id.to_string().as_str(),
        message_id,
        attachments,
        &[draft_scope.to_string()],
    )
}

/// Builds the compatibility ACK for an already observed durable Continue action.
/// 为已经可靠观察的 Continue 动作构建兼容确认。
///
/// # Parameters / 参数
/// `state` supplies only the current display revision and run phase; `session_id` and
/// `client_action_id` identify the V1 action already proven by the caller.
/// `state` 只提供当前显示版本与运行阶段；`session_id` 与 `client_action_id` 标识调用方已经
/// 证明的 V1 动作。
///
/// # Returns / 返回值
/// An identity-stable ACK without starting another preparation or physical Provider request.
/// 返回身份稳定的确认，且不会再次启动准备流程或物理 Provider 请求。
fn continuation_replay_ack_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    client_action_id: String,
) -> Result<SubmitConversationPromptAck, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from("Observed V1 Continue has no retained workbench display projection")
    })?;
    let run_phase = workspace
        .session_run_phases
        .get(&session_id)
        .copied()
        .ok_or_else(|| {
            String::from("Observed V1 Continue has no retained workbench run-phase projection")
        })?;
    Ok(SubmitConversationPromptAck {
        client_submission_id: client_action_id,
        conversation_id: session_id.to_string(),
        accepted_message_id: None,
        turn_id: None,
        run_phase: run_phase.to_string(),
        queued: false,
        revision: guard.workbench_generation,
    })
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

    /// Verifies every initialized session is preserved when a submission omits settings.
    /// 验证提交缺少设置时保留所有已初始化会话。
    #[test]
    fn omitted_settings_preserve_every_initialized_session() {
        assert!(should_preserve_existing_provider_tool_settings(false, true));
        assert!(!should_preserve_existing_provider_tool_settings(true, true));
        assert!(!should_preserve_existing_provider_tool_settings(
            false, false
        ));
    }

    /// Verifies the draft wire shape carries an optional permission mode.
    /// 验证草稿线格式携带可选的权限模式。
    #[test]
    fn draft_settings_accept_an_optional_permission_mode() {
        use vulcan_session_core::v1::config::PermissionMode;
        let explicit: SubmissionSessionSettings =
            serde_json::from_value(serde_json::json!({"permissionMode": "read_only"}))
                .expect("explicit mode should parse");
        assert_eq!(explicit.permission_mode, Some(PermissionMode::ReadOnly));
        let omitted: SubmissionSessionSettings =
            serde_json::from_value(serde_json::json!({})).expect("omitted mode should parse");
        assert_eq!(omitted.permission_mode, None);
    }

    /// Verifies an ordinary existing-session submission cannot activate a newly selected style.
    /// 验证已有会话的普通提交不能激活新选择的输出风格。
    #[test]
    fn ordinary_submission_keeps_output_style_snapshot_unchanged() {
        let settings = SubmissionSessionSettings {
            answer_mode: None,
            permission_mode: None,
            output_style_id: Some(String::from("rigorous")),
            vmm_policy: None,
            provider_tool_settings: None,
            knowledge_selection: None,
            initial_goal: None,
            dedicated: None,
            prompt_skill_overrides: Default::default(),
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
