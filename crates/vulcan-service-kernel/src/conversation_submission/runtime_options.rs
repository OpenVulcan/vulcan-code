//! Runtime option preparation for immediate conversation submissions.
//! 即时会话提交的运行选项准备。

use std::collections::{BTreeMap, BTreeSet};

use vulcan_provider_config::config::{
    AiToolKind, PersistedAiToolConfiguration, ReasoningEffortLevel,
};
use vulcan_workbench_model::{PendingSessionPromptSource, SessionId, SessionRuntimeStateEntry};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use crate::{
    ai_capabilities::{
        parse_ai_capability_target_order_overrides, parse_ai_capability_target_selectors,
    },
    current_unix_millis_text,
    session_goal_lifecycle::ensure_session_can_enter_plan_mode,
    settings_runtime_projection::session_ai_tool_configuration_or_default,
    workspace_persistence_versions::parse_reasoning_effort,
};
use vulcan_agent_runtime::workbench_ai_capability_router::effective_workbench_disabled_ai_capability_kinds;

/// Raw request controls required to prepare one immediate submission turn.
/// 准备一次即时提交回合所需的原始请求控制项。
pub(crate) struct SubmissionRuntimeOptionsInput {
    /// Optional execution-mode wire value; absence selects build mode.
    /// 可选执行模式传输值；缺省时选择构建模式。
    pub(crate) execution_mode: Option<String>,
    /// Optional reasoning-effort wire value for this turn.
    /// 本回合可选推理强度传输值。
    pub(crate) reasoning_effort: Option<String>,
    /// Optional prompt-level disabled capability keys; presence replaces the session default set.
    /// 可选提示词级禁用能力键；存在时替换会话默认集合。
    pub(crate) disabled_ai_capability_kinds: Option<Vec<String>>,
    /// Optional prompt-level disabled capability target selectors.
    /// 可选提示词级禁用能力目标选择器。
    pub(crate) disabled_ai_capability_target_selectors: Option<Vec<String>>,
    /// Optional prompt-level target order overrides keyed by capability wire value.
    /// 可选提示词级目标顺序覆盖，按能力传输值分组。
    pub(crate) ai_capability_target_order_overrides: Option<BTreeMap<String, Vec<String>>>,
}

/// Strongly typed runtime controls consumed by one shared prompt turn.
/// 单个共享提示词回合消费的强类型运行控制项。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SubmissionRuntimeOptions {
    /// Effective execution-mode label forwarded to execution preparation.
    /// 转发给执行准备阶段的生效执行模式标签。
    pub(crate) execution_mode: String,
    /// Parsed reasoning effort for this turn, when the wire value is recognized.
    /// 传输值可识别时得到的本回合推理强度。
    pub(crate) reasoning_effort: Option<ReasoningEffortLevel>,
    /// Effective semantic capability kinds disabled for this turn.
    /// 本回合生效的禁用语义能力类型。
    pub(crate) disabled_ai_capability_kinds: BTreeSet<AiToolKind>,
    /// Cleaned capability target selectors disabled for this turn.
    /// 本回合禁用的已清洗能力目标选择器。
    pub(crate) disabled_ai_capability_target_selectors: BTreeSet<String>,
    /// Cleaned capability target order overrides for this turn.
    /// 本回合已清洗的能力目标顺序覆盖。
    pub(crate) ai_capability_target_order_overrides: BTreeMap<AiToolKind, Vec<String>>,
}

impl SubmissionRuntimeOptions {
    /// Returns whether the prepared turn requests Plan execution semantics.
    /// 返回准备后的回合是否请求计划执行语义。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the case-insensitive `plan` execution-mode label.
    /// 仅当执行模式标签大小写不敏感地等于 `plan` 时返回 `true`。
    fn requests_plan_mode(&self) -> bool {
        self.execution_mode.eq_ignore_ascii_case("plan")
    }
}

/// Prepares one turn's runtime controls and commits authorized Plan state to the session.
/// 准备单回合运行控制项，并将已授权的计划状态提交到会话。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the goal store used by Plan-mode mutual-exclusion checks.
/// `controller_runtime` 持有计划模式互斥检查所用的目标存储。
/// `workspace_snapshot` owns the authoritative session configuration and persistent work mode.
/// `workspace_snapshot` 持有权威会话配置与持久工作模式。
/// `session_id` selects the materialized session receiving these controls.
/// `session_id` 选择接收这些控制项的已物化会话。
/// `default_ai_tool_configuration` is used only when the session has no configuration snapshot.
/// `default_ai_tool_configuration` 仅在会话没有配置快照时使用。
/// `input` contains request-scoped wire controls consumed exactly once.
/// `input` 包含只消费一次的请求级传输控制项。
///
/// # Returns
/// 返回值
/// Strongly typed turn controls after parsing and any required Plan authorization.
/// 返回解析并完成必要计划授权后的强类型回合控制项。
pub(crate) async fn prepare_submission_runtime_options(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    default_ai_tool_configuration: &PersistedAiToolConfiguration,
    input: SubmissionRuntimeOptionsInput,
) -> Result<SubmissionRuntimeOptions, String> {
    // Parse every request-scoped value before mutating persistent Plan state so malformed input leaves the session unchanged.
    // 在修改持久计划状态前解析全部请求级值，确保畸形输入不会改变会话。
    let options = resolve_submission_runtime_options(
        workspace_snapshot,
        session_id,
        default_ai_tool_configuration,
        input,
    )?;
    if options.requests_plan_mode() {
        // Goal exclusion is the sole authorization boundary; persistent Plan state is written only after it succeeds.
        // 目标互斥是唯一授权边界；仅在检查成功后写入持久计划状态。
        ensure_session_can_enter_plan_mode(controller_runtime, session_id).await?;
        apply_authorized_plan_mode(workspace_snapshot, session_id);
    }
    Ok(options)
}

/// Resolves raw submission controls without mutating session runtime state.
/// 在不修改会话运行态的前提下解析原始提交控制项。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides the session-scoped AI tool configuration.
/// `workspace_snapshot` 提供会话级 AI 工具配置。
/// `session_id` selects the configuration owner.
/// `session_id` 选择配置所有者。
/// `default_ai_tool_configuration` is the explicit fallback template.
/// `default_ai_tool_configuration` 是显式回退模板。
/// `input` contains request-scoped wire controls.
/// `input` 包含请求级传输控制项。
///
/// # Returns
/// 返回值
/// Parsed turn controls, or an error for an invalid capability key or order kind.
/// 返回解析后的回合控制项；能力键或排序类型无效时返回错误。
fn resolve_submission_runtime_options(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    default_ai_tool_configuration: &PersistedAiToolConfiguration,
    input: SubmissionRuntimeOptionsInput,
) -> Result<SubmissionRuntimeOptions, String> {
    // Destructuring makes the one-shot ownership transfer from WebUI payload to turn controls explicit.
    // 解构明确 WebUI 载荷到回合控制项的一次性所有权转移。
    let SubmissionRuntimeOptionsInput {
        execution_mode,
        reasoning_effort,
        disabled_ai_capability_kinds,
        disabled_ai_capability_target_selectors,
        ai_capability_target_order_overrides,
    } = input;
    let disabled_ai_capability_kinds = effective_session_disabled_ai_capability_kinds(
        workspace_snapshot,
        session_id,
        default_ai_tool_configuration,
        disabled_ai_capability_kinds.as_deref(),
    )?;

    Ok(SubmissionRuntimeOptions {
        execution_mode: execution_mode.unwrap_or_else(|| String::from("build")),
        reasoning_effort: reasoning_effort.as_deref().and_then(parse_reasoning_effort),
        disabled_ai_capability_kinds,
        disabled_ai_capability_target_selectors: parse_ai_capability_target_selectors(
            disabled_ai_capability_target_selectors,
        ),
        ai_capability_target_order_overrides: parse_ai_capability_target_order_overrides(
            ai_capability_target_order_overrides,
        )?,
    })
}

/// Resolves effective semantic capability disables from one session and an optional prompt override.
/// 根据单个会话及可选提示词覆盖解析生效的语义能力禁用集合。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides the authoritative session-scoped AI tool configuration.
/// `workspace_snapshot` 提供权威的会话级 AI 工具配置。
/// `session_id` selects the configuration owner.
/// `session_id` 选择配置所有者。
/// `default_ai_tool_configuration` is used only when the session has no configuration snapshot.
/// `default_ai_tool_configuration` 仅在会话没有配置快照时使用。
/// `prompt_disabled_ai_capability_kinds` replaces session defaults when explicitly present.
/// `prompt_disabled_ai_capability_kinds` 显式存在时替换会话默认值。
///
/// # Returns
/// 返回值
/// The exact disabled capability kinds that must be forwarded into one provider-visible turn.
/// 返回必须转发到单个 Provider 可见回合的精确禁用能力集合。
pub(crate) fn effective_session_disabled_ai_capability_kinds(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    default_ai_tool_configuration: &PersistedAiToolConfiguration,
    prompt_disabled_ai_capability_kinds: Option<&[String]>,
) -> Result<BTreeSet<AiToolKind>, String> {
    // Session configuration is the persistent source shared by user submissions and every automatic continuation.
    // 会话配置是用户提交与所有自动续轮共享的持久化来源。
    let session_ai_tool_configuration = session_ai_tool_configuration_or_default(
        workspace_snapshot,
        session_id,
        default_ai_tool_configuration,
    );
    effective_workbench_disabled_ai_capability_kinds(
        session_ai_tool_configuration,
        prompt_disabled_ai_capability_kinds,
    )
}

/// Resolves semantic capability disables for a queued prompt at its actual dispatch boundary.
/// 在排队提示词的实际分发边界解析语义能力禁用集合。
///
/// # Parameters
/// 参数
/// `source` identifies whether runtime controls were authored by the user or synthesized by the host.
/// `source` 标识运行控制项由用户编写还是由宿主合成。
/// `captured_disabled_ai_capability_kinds` contains the immutable controls captured for a user-authored queue entry.
/// `captured_disabled_ai_capability_kinds` 包含为用户编写队列项捕获的不可变控制项。
/// `workspace_snapshot` provides the authoritative session-scoped AI tool configuration.
/// `workspace_snapshot` 提供权威的会话级 AI 工具配置。
/// `session_id` selects the configuration owner.
/// `session_id` 选择配置所有者。
/// `default_ai_tool_configuration` is used only when the session has no configuration snapshot.
/// `default_ai_tool_configuration` 仅在会话没有配置快照时使用。
///
/// # Returns
/// 返回值
/// The exact disabled capability kinds for this queued provider-visible turn.
/// 返回当前排队 Provider 可见回合的精确禁用能力集合。
pub(crate) fn effective_pending_prompt_disabled_ai_capability_kinds(
    source: PendingSessionPromptSource,
    captured_disabled_ai_capability_kinds: &BTreeSet<AiToolKind>,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    default_ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Result<BTreeSet<AiToolKind>, String> {
    if source == PendingSessionPromptSource::GoalContinuation {
        // Host-created goal guidance must share the persistent session catalog instead of using its neutral queue placeholder.
        // 宿主创建的目标引导必须共享持久化会话工具目录，而不能使用其中性的队列占位配置。
        return effective_session_disabled_ai_capability_kinds(
            workspace_snapshot,
            session_id,
            default_ai_tool_configuration,
            None,
        );
    }
    Ok(captured_disabled_ai_capability_kinds.clone())
}

/// Resolves reasoning effort for a queued prompt at its actual dispatch boundary.
/// 在排队提示词的实际分发边界解析推理强度。
///
/// # Parameters
/// 参数
/// `source` identifies whether the queue entry is user-authored or host-created.
/// `source` 标识队列项由用户编写还是由宿主创建。
/// `captured_reasoning_effort` is the immutable control captured for a user-authored queue entry.
/// `captured_reasoning_effort` 是为用户编写队列项捕获的不可变控制项。
/// `active_reasoning_effort` is the current session/provider reasoning selection.
/// `active_reasoning_effort` 是当前会话与 Provider 的推理强度选择。
///
/// # Returns
/// 返回值
/// The reasoning effort that must be forwarded into the queued turn.
/// 返回必须转发到排队回合的推理强度。
pub(crate) fn effective_pending_prompt_reasoning_effort(
    source: PendingSessionPromptSource,
    captured_reasoning_effort: ReasoningEffortLevel,
    active_reasoning_effort: Option<ReasoningEffortLevel>,
) -> Option<ReasoningEffortLevel> {
    if source == PendingSessionPromptSource::GoalContinuation {
        // Goal guidance is generated without user controls, so it inherits the same live selection as ordinary automatic continuation.
        // 目标引导生成时不含用户控制项，因此继承与普通自动续轮相同的实时选择。
        active_reasoning_effort
    } else {
        Some(captured_reasoning_effort)
    }
}

/// Commits Plan mode after the caller has completed the goal mutual-exclusion check.
/// 在调用方完成目标互斥检查后提交计划模式。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the target session runtime state.
/// `workspace_snapshot` 持有目标会话运行态。
/// `session_id` selects the session entering Plan mode.
/// `session_id` 选择进入计划模式的会话。
///
/// # Returns
/// 返回值
/// This function returns no value.
/// 此函数没有返回值。
fn apply_authorized_plan_mode(workspace_snapshot: &mut WorkspaceSnapshot, session_id: SessionId) {
    let updated_at = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: updated_at.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.active_work_mode = Some(String::from("plan"));
    runtime_state.updated_at = updated_at;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an empty workspace snapshot with one explicit selection anchor.
    /// 构建带有一个显式选择锚点的空工作区快照。
    ///
    /// # Parameters
    /// 参数
    /// `selected_session` is the stable selection identifier required by the snapshot model.
    /// `selected_session` 是快照模型要求的稳定选择标识。
    ///
    /// # Returns
    /// 返回值
    /// A structurally complete workspace snapshot with no sessions or runtime payloads.
    /// 返回结构完整且不含会话或运行载荷的工作区快照。
    fn empty_workspace_snapshot(selected_session: SessionId) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies absent prompt overrides inherit persistent capability disables and build defaults.
    /// 验证缺少提示词覆盖时会继承持久能力禁用项与构建默认值。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails when session defaults leak into unrelated controls.
    /// 此测试无返回值；若会话默认值泄漏到无关控制项则失败。
    #[test]
    fn absent_prompt_overrides_use_session_capability_defaults() {
        // session_id identifies the owner of the persistent AI tool configuration.
        // session_id 标识持久 AI 工具配置的所有者。
        let session_id = SessionId::new_v4();
        // workspace_snapshot contains the exact session-scoped disabled capability set.
        // workspace_snapshot 包含精确的会话级禁用能力集合。
        let mut workspace_snapshot = empty_workspace_snapshot(session_id);
        workspace_snapshot.session_runtime_state.insert(
            session_id,
            SessionRuntimeStateEntry {
                ai_tool_configuration: Some(PersistedAiToolConfiguration {
                    disabled_capability_kinds: BTreeSet::from([AiToolKind::WebSearch]),
                    ..PersistedAiToolConfiguration::default()
                }),
                ..SessionRuntimeStateEntry::default()
            },
        );
        // default_configuration proves the session snapshot wins over the global template.
        // default_configuration 证明会话快照优先于全局模板。
        let default_configuration = PersistedAiToolConfiguration {
            disabled_capability_kinds: BTreeSet::from([AiToolKind::UnderstandImage]),
            ..PersistedAiToolConfiguration::default()
        };

        let options = resolve_submission_runtime_options(
            &workspace_snapshot,
            session_id,
            &default_configuration,
            SubmissionRuntimeOptionsInput {
                execution_mode: None,
                reasoning_effort: Some(String::from("unsupported")),
                disabled_ai_capability_kinds: None,
                disabled_ai_capability_target_selectors: None,
                ai_capability_target_order_overrides: None,
            },
        )
        .expect("session defaults should resolve");

        assert_eq!(options.execution_mode, "build");
        assert_eq!(options.reasoning_effort, None);
        assert_eq!(
            options.disabled_ai_capability_kinds,
            BTreeSet::from([AiToolKind::WebSearch])
        );
        assert!(options.disabled_ai_capability_target_selectors.is_empty());
        assert!(options.ai_capability_target_order_overrides.is_empty());
    }

    /// Verifies host-created goal queue entries inherit live session controls while user entries keep captured controls.
    /// 验证宿主创建的目标队列项继承实时会话控制，而用户队列项保留已捕获控制。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails when an internal goal turn mutates the provider request configuration.
    /// 此测试无返回值；若内部目标回合改变 Provider 请求配置则失败。
    #[test]
    fn goal_pending_prompt_inherits_live_session_runtime_controls() {
        // session_id identifies the session whose persistent tool catalog must remain stable.
        // session_id 标识持久化工具目录必须保持稳定的会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot carries the authoritative disabled capability selected for the session.
        // workspace_snapshot 携带为该会话选择的权威禁用能力。
        let mut workspace_snapshot = empty_workspace_snapshot(session_id);
        workspace_snapshot.session_runtime_state.insert(
            session_id,
            SessionRuntimeStateEntry {
                ai_tool_configuration: Some(PersistedAiToolConfiguration {
                    disabled_capability_kinds: BTreeSet::from([AiToolKind::WebSearch]),
                    ..PersistedAiToolConfiguration::default()
                }),
                ..SessionRuntimeStateEntry::default()
            },
        );
        // captured_user_controls model an independently authored queued prompt.
        // captured_user_controls 模拟一条独立编写的用户排队提示词。
        let captured_user_controls = BTreeSet::from([AiToolKind::UnderstandImage]);

        assert_eq!(
            effective_pending_prompt_disabled_ai_capability_kinds(
                PendingSessionPromptSource::GoalContinuation,
                &captured_user_controls,
                &workspace_snapshot,
                session_id,
                &PersistedAiToolConfiguration::default(),
            )
            .expect("goal continuation controls should resolve"),
            BTreeSet::from([AiToolKind::WebSearch])
        );
        assert_eq!(
            effective_pending_prompt_disabled_ai_capability_kinds(
                PendingSessionPromptSource::UserFollowup,
                &captured_user_controls,
                &workspace_snapshot,
                session_id,
                &PersistedAiToolConfiguration::default(),
            )
            .expect("user queue controls should resolve"),
            captured_user_controls
        );
        assert_eq!(
            effective_pending_prompt_reasoning_effort(
                PendingSessionPromptSource::GoalContinuation,
                ReasoningEffortLevel::Medium,
                Some(ReasoningEffortLevel::High),
            ),
            Some(ReasoningEffortLevel::High)
        );
        assert_eq!(
            effective_pending_prompt_reasoning_effort(
                PendingSessionPromptSource::UserFollowup,
                ReasoningEffortLevel::Medium,
                Some(ReasoningEffortLevel::High),
            ),
            Some(ReasoningEffortLevel::Medium)
        );
    }

    /// Verifies explicit prompt controls replace session disables and normalize target selectors.
    /// 验证显式提示词控制项会替换会话禁用项并规范化目标选择器。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails when one request-scoped control is lost or persisted implicitly.
    /// 此测试无返回值；若任一请求级控制项丢失或被隐式持久化则失败。
    #[test]
    fn explicit_prompt_controls_produce_typed_turn_options() {
        // session_id identifies the owner whose persistent defaults are explicitly overridden.
        // session_id 标识持久默认值被显式覆盖的所有者。
        let session_id = SessionId::new_v4();
        // workspace_snapshot provides a persistent disable that an explicit empty list must clear for this turn.
        // workspace_snapshot 提供持久禁用项，显式空列表必须仅在本回合清除它。
        let mut workspace_snapshot = empty_workspace_snapshot(session_id);
        workspace_snapshot.session_runtime_state.insert(
            session_id,
            SessionRuntimeStateEntry {
                ai_tool_configuration: Some(PersistedAiToolConfiguration {
                    disabled_capability_kinds: BTreeSet::from([AiToolKind::WebSearch]),
                    ..PersistedAiToolConfiguration::default()
                }),
                ..SessionRuntimeStateEntry::default()
            },
        );

        let options = resolve_submission_runtime_options(
            &workspace_snapshot,
            session_id,
            &PersistedAiToolConfiguration::default(),
            SubmissionRuntimeOptionsInput {
                execution_mode: Some(String::from("plan")),
                reasoning_effort: Some(String::from("HIGH")),
                disabled_ai_capability_kinds: Some(Vec::new()),
                disabled_ai_capability_target_selectors: Some(vec![
                    String::from("  vendor-a/vision  "),
                    String::from("  "),
                ]),
                ai_capability_target_order_overrides: Some(BTreeMap::from([(
                    String::from("understand_image"),
                    vec![String::from(" vendor-b/vision ")],
                )])),
            },
        )
        .expect("explicit prompt controls should resolve");

        assert!(options.requests_plan_mode());
        assert_eq!(options.reasoning_effort, Some(ReasoningEffortLevel::High));
        assert!(options.disabled_ai_capability_kinds.is_empty());
        assert_eq!(
            options.disabled_ai_capability_target_selectors,
            BTreeSet::from([String::from("vendor-a/vision")])
        );
        assert_eq!(
            options.ai_capability_target_order_overrides,
            BTreeMap::from([(
                AiToolKind::UnderstandImage,
                vec![String::from("vendor-b/vision")],
            )])
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
            None
        );
    }

    /// Verifies malformed capability order input fails before any persistent Plan mutation.
    /// 验证畸形能力排序输入会在任何持久计划修改前失败。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails when an unknown capability kind is accepted.
    /// 此测试无返回值；若未知能力类型被接受则失败。
    #[test]
    fn unknown_capability_order_kind_is_rejected_without_mutation() {
        // session_id identifies the session whose state must remain untouched.
        // session_id 标识状态必须保持不变的会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot starts without runtime state so accidental mutation is observable.
        // workspace_snapshot 初始不含运行态，使意外修改可被观察。
        let workspace_snapshot = empty_workspace_snapshot(session_id);

        let error = resolve_submission_runtime_options(
            &workspace_snapshot,
            session_id,
            &PersistedAiToolConfiguration::default(),
            SubmissionRuntimeOptionsInput {
                execution_mode: Some(String::from("plan")),
                reasoning_effort: None,
                disabled_ai_capability_kinds: None,
                disabled_ai_capability_target_selectors: None,
                ai_capability_target_order_overrides: Some(BTreeMap::from([(
                    String::from("not_a_capability"),
                    vec![String::from("vendor/target")],
                )])),
            },
        )
        .expect_err("unknown capability order kind should fail");

        assert!(error.contains("not_a_capability"));
        assert!(workspace_snapshot.session_runtime_state.is_empty());
    }

    /// Verifies authorized Plan application creates the exact persistent session marker.
    /// 验证已授权计划应用会创建精确的持久会话标记。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails when Plan state is not session-scoped.
    /// 此测试无返回值；若计划状态未按会话归属则失败。
    #[test]
    fn authorized_plan_mode_updates_only_the_target_session() {
        // target_session_id identifies the session whose authorization already succeeded.
        // target_session_id 标识已经授权成功的目标会话。
        let target_session_id = SessionId::new_v4();
        // unrelated_session_id guards against cross-session runtime-state mutation.
        // unrelated_session_id 防止跨会话运行态修改。
        let unrelated_session_id = SessionId::new_v4();
        // workspace_snapshot owns both session runtime-state entries observed by the mutation.
        // workspace_snapshot 持有本次修改观察的两个会话运行态条目。
        let mut workspace_snapshot = empty_workspace_snapshot(target_session_id);
        workspace_snapshot.session_runtime_state.insert(
            unrelated_session_id,
            SessionRuntimeStateEntry {
                active_work_mode: Some(String::from("build")),
                ..SessionRuntimeStateEntry::default()
            },
        );

        apply_authorized_plan_mode(&mut workspace_snapshot, target_session_id);

        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&target_session_id)
                .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
            Some("plan")
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&unrelated_session_id)
                .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
            Some("build")
        );
    }
}
