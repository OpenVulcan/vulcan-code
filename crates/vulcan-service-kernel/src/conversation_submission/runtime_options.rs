//! Runtime option preparation for immediate conversation submissions.
//! 即时会话提交的运行选项准备。

use vulcan_provider_config::config::ReasoningEffortLevel;
use vulcan_workbench_model::{
    PendingPromptRuntimeOptions, PendingSessionPromptSource, SessionId, SessionRuntimeStateEntry,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::{
    SharedServiceBridgeState, current_unix_millis_text, persist_session_plan_mode_v1,
    session_goal_lifecycle::{
        current_v1_goal_unix_millis, ensure_session_can_enter_plan_mode,
        registered_session_v1_handle_from_state,
    },
    session_request_config_v1::{SessionRequestFallback, set_pending_reasoning_request_v1},
    workspace_persistence_versions::parse_reasoning_effort,
};

/// Raw request controls required to prepare one immediate submission turn.
/// 准备一次即时提交回合所需的原始请求控制项。
pub(crate) struct SubmissionRuntimeOptionsInput {
    /// Optional execution-mode wire value; absence selects build mode.
    /// 可选执行模式传输值；缺省时选择构建模式。
    pub(crate) execution_mode: Option<String>,
    /// Optional reasoning-effort wire value for this turn.
    /// 本回合可选推理强度传输值。
    pub(crate) reasoning_effort: Option<String>,
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
/// `workspace_snapshot` owns the process-local execution mirror; the registered V1 document owns request authority.
/// `workspace_snapshot` 持有进程内执行镜像；已注册 V1 文档持有请求权威。
/// `session_id` selects the materialized session receiving these controls.
/// `session_id` 选择接收这些控制项的已物化会话。
/// `input` contains request-scoped wire controls consumed exactly once.
/// `input` 包含只消费一次的请求级传输控制项。
///
/// # Returns
/// 返回值
/// Strongly typed turn controls after parsing and any required Plan authorization.
/// 返回解析并完成必要计划授权后的强类型回合控制项。
pub(crate) async fn prepare_submission_runtime_options(
    state: &SharedServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    input: SubmissionRuntimeOptionsInput,
) -> Result<SubmissionRuntimeOptions, String> {
    let options = resolve_submission_runtime_options(input);
    if options.requests_plan_mode() {
        // The V1 Goal metadata is the sole authorization fact and set_plan_mode is the durable mode transition.
        // V1 Goal metadata 是唯一授权事实，set_plan_mode 是可靠的模式转换。
        ensure_session_can_enter_plan_mode(state, session_id).await?;
        persist_session_plan_mode_v1(state, session_id, true).await?;
        apply_authorized_plan_mode(workspace_snapshot, session_id);
    }
    if let Some(reasoning_effort) = options.reasoning_effort {
        let handle = registered_session_v1_handle_from_state(state, session_id)?;
        commit_submission_reasoning_effort(
            handle.as_ref(),
            workspace_snapshot,
            session_id,
            reasoning_effort,
            current_v1_goal_unix_millis()?,
        )?;
    }
    Ok(options)
}

/// Durably stages one submission-scoped reasoning choice before updating its process mirror.
/// 在更新进程内镜像前，可靠暂存一次提交范围的思考选择。
///
/// # Parameters
/// 参数
/// `handle` owns the authoritative V1 request configuration.
/// `handle` 持有权威 V1 请求配置。
/// `workspace_snapshot` contains the exact admitted process-local runtime projection.
/// `workspace_snapshot` 包含精确已准入的进程内运行时投影。
/// `session_id` selects the single session receiving both projections.
/// `session_id` 选择同时接收两种投影的唯一会话。
/// `reasoning_effort` is the validated provider-neutral reasoning level.
/// `reasoning_effort` 是经过校验的供应商中立思考档位。
/// `now_ms` is the checked V1 mutation timestamp.
/// `now_ms` 是经过检查的 V1 修改时间戳。
///
/// # Returns
/// 返回值
/// Success only after the V1 pending request is durable and the process mirror matches it.
/// 仅在 V1 待生效请求可靠落盘且进程内镜像与其一致后返回成功。
fn commit_submission_reasoning_effort(
    handle: &crate::session_runtime_registry_v1::SessionHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    reasoning_effort: ReasoningEffortLevel,
    now_ms: i64,
) -> Result<(), String> {
    let fallback = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(SessionRequestFallback::from_runtime_state)
        .ok_or_else(|| {
            format!("Session `{session_id}` has no admitted runtime state for reasoning selection.")
        })?;
    set_pending_reasoning_request_v1(handle, reasoning_effort, fallback, now_ms)?;
    // The process mirror changes only after the authoritative pending request is durable.
    // 只有权威待生效请求可靠落盘后，进程内镜像才允许改变。
    apply_submission_reasoning_effort(workspace_snapshot, session_id, reasoning_effort);
    Ok(())
}

/// Commits the submitted reasoning effort to the exact materialized session.
/// 将提交的思考档位写入精确的已物化会话。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the target session runtime state.
/// `workspace_snapshot` 持有目标会话运行时状态。
/// `session_id` selects the session receiving the reasoning choice.
/// `session_id` 选择接收思考档位的会话。
/// `reasoning_effort` is the validated provider-neutral reasoning value.
/// `reasoning_effort` 是已验证的供应商中立思考档位。
///
/// # Returns
/// 返回值
/// This function returns no value.
/// 此函数没有返回值。
fn apply_submission_reasoning_effort(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    reasoning_effort: ReasoningEffortLevel,
) {
    let updated_at = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: updated_at.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.active_reasoning_effort = Some(reasoning_effort);
    runtime_state.active_reasoning_effort_explicit = true;
    runtime_state.updated_at = updated_at;
}

/// Resolves raw submission controls without mutating session runtime state.
/// 在不修改会话运行态的前提下解析原始提交控制项。
///
/// # Parameters
/// 参数
/// `input` contains request-scoped wire controls.
/// `input` 包含请求级传输控制项。
///
/// # Returns
/// 返回值
/// Parsed turn controls with stable build-mode defaults.
/// 返回带有稳定构建模式默认值的已解析回合控制项。
fn resolve_submission_runtime_options(
    input: SubmissionRuntimeOptionsInput,
) -> SubmissionRuntimeOptions {
    SubmissionRuntimeOptions {
        execution_mode: input
            .execution_mode
            .unwrap_or_else(|| String::from("build")),
        reasoning_effort: input
            .reasoning_effort
            .as_deref()
            .and_then(parse_reasoning_effort),
    }
}

/// Builds the runtime controls for a host adapter that dispatches an already durable V1 input.
/// 为分发已可靠写入 V1 输入的宿主适配器构建运行控制项。
///
/// # Parameters
/// 参数
/// `handle` owns the exact pending-or-active request configuration for the next physical API.
/// `handle` 持有下一次物理 API 的精确待生效或当前请求配置。
///
/// # Returns
/// 返回值
/// Runtime controls whose reasoning effort exactly matches the configuration that the Provider
/// permit will promote, or an explicit error when the durable value cannot be represented.
/// 返回推理强度与 Provider 许可将提升的配置完全一致的运行控制项；可靠值无法表示时显式报错。
pub(crate) fn exact_v1_dispatch_runtime_options(
    handle: &crate::session_runtime_registry_v1::SessionHandle,
) -> Result<PendingPromptRuntimeOptions, String> {
    let thinking_level = handle
        .read(|document| {
            if let Some(pending) = document.metadata.config.pending_request.as_ref() {
                pending.thinking_level.clone()
            } else {
                document
                    .metadata
                    .config
                    .active_request
                    .thinking_level
                    .clone()
            }
        })
        .map_err(|error| format!("读取 V1 精确请求推理配置失败：{error}"))?;
    let reasoning_effort = match thinking_level {
        Some(thinking_level) => parse_reasoning_effort(thinking_level.as_str())
            .ok_or_else(|| format!("V1 精确请求包含无法识别的推理档位 `{thinking_level}`。"))?,
        None => ReasoningEffortLevel::Off,
    };
    Ok(PendingPromptRuntimeOptions {
        reasoning_effort,
        ..PendingPromptRuntimeOptions::default()
    })
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
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use super::*;
    use crate::session_runtime_registry_v1::{SessionHandle, SessionPersistenceBackend};
    use vulcan_session_core::v1::config::{ActiveRequestConfig, PendingRequestConfig};
    use vulcan_session_core::v1::document::{FlushCandidate, SessionDocument};
    use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

    /// In-memory publication seam for submission runtime-option durability tests.
    /// 提交运行选项可靠性测试使用的内存发布接缝。
    struct SubmissionRuntimeOptionsTestBackend;

    impl SessionPersistenceBackend for SubmissionRuntimeOptionsTestBackend {
        /// Accepts one complete V1 flush candidate without touching the filesystem.
        /// 接纳一份完整 V1 flush 候选且不访问文件系统。
        ///
        /// # Parameters
        /// 参数
        /// `candidate` is the exact candidate emitted by the session owner.
        /// `candidate` 是会话 owner 发出的精确候选。
        ///
        /// # Returns
        /// 返回值
        /// A successful publication receipt carrying the candidate metadata.
        /// 返回携带候选 metadata 的成功发布回执。
        fn flush(
            &self,
            candidate: &FlushCandidate,
        ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
            Ok(SessionV1FlushResult {
                written_file_count: candidate.dirty.len(),
                published_metadata: Some(candidate.metadata.clone()),
            })
        }
    }

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
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies raw request controls produce typed runtime options.
    /// 验证原始请求控制项会生成强类型运行选项。
    #[test]
    fn request_controls_produce_typed_turn_options() {
        let options = resolve_submission_runtime_options(SubmissionRuntimeOptionsInput {
            execution_mode: Some(String::from("plan")),
            reasoning_effort: Some(String::from("HIGH")),
        });

        assert!(options.requests_plan_mode());
        assert_eq!(options.reasoning_effort, Some(ReasoningEffortLevel::High));
    }

    /// Verifies authorized Plan application creates the exact persistent session marker.
    /// 验证已授权计划应用会创建精确的持久会话标记。
    #[test]
    fn authorized_plan_mode_updates_only_the_target_session() {
        let target_session_id = SessionId::new_v4();
        let unrelated_session_id = SessionId::new_v4();
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

    /// Verifies submission reasoning updates only the newly materialized target session.
    /// 验证提交的思考档位只更新新物化的目标会话。
    #[test]
    fn submission_reasoning_updates_only_the_target_session() {
        let target_session_id = SessionId::new_v4();
        let unrelated_session_id = SessionId::new_v4();
        let mut workspace_snapshot = empty_workspace_snapshot(target_session_id);
        workspace_snapshot.session_runtime_state.insert(
            unrelated_session_id,
            SessionRuntimeStateEntry {
                active_reasoning_effort: Some(ReasoningEffortLevel::Low),
                ..SessionRuntimeStateEntry::default()
            },
        );

        apply_submission_reasoning_effort(
            &mut workspace_snapshot,
            target_session_id,
            ReasoningEffortLevel::Adaptive,
        );

        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&target_session_id)
                .and_then(|runtime_state| runtime_state.active_reasoning_effort),
            Some(ReasoningEffortLevel::Adaptive)
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&unrelated_session_id)
                .and_then(|runtime_state| runtime_state.active_reasoning_effort),
            Some(ReasoningEffortLevel::Low)
        );
    }

    /// Verifies a submission reasoning change reaches V1 pending metadata before its process mirror changes.
    /// 验证提交思考档位变化会先进入 V1 pending metadata，再更新进程内镜像。
    #[test]
    fn submission_reasoning_stages_v1_pending_before_process_mirror() {
        let session_id = SessionId::from_u128(0x6301);
        let mut document = SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Submission reasoning",
            1,
        );
        document.metadata.config.active_request = ActiveRequestConfig {
            provider: Some(String::from("provider-a")),
            connection_id: Some(String::from("connection-a")),
            model: Some(String::from("model-a")),
            thinking_level: Some(String::from("high")),
            temperature: Some(0.25),
        };
        let handle = SessionHandle::with_backend(
            session_id,
            document,
            Arc::new(SubmissionRuntimeOptionsTestBackend),
        );
        let mut workspace_snapshot = empty_workspace_snapshot(session_id);
        workspace_snapshot.session_runtime_state.insert(
            session_id,
            SessionRuntimeStateEntry {
                active_provider_id: Some(String::from("provider-a")),
                active_connection_id: Some(String::from("connection-a")),
                active_model_id: Some(String::from("model-a")),
                active_reasoning_effort: Some(ReasoningEffortLevel::High),
                active_reasoning_effort_explicit: true,
                active_temperature: Some(0.25),
                ..SessionRuntimeStateEntry::default()
            },
        );

        commit_submission_reasoning_effort(
            handle.as_ref(),
            &mut workspace_snapshot,
            session_id,
            ReasoningEffortLevel::Max,
            2,
        )
        .expect("submission reasoning should stage durably");

        let (active, pending) = handle
            .read(|document| {
                (
                    document.metadata.config.active_request.clone(),
                    document.metadata.config.pending_request.clone(),
                )
            })
            .expect("request metadata should remain readable");
        assert_eq!(active.thinking_level.as_deref(), Some("high"));
        let pending = pending.expect("submission reasoning should create pending metadata");
        assert_eq!(pending.provider.as_deref(), Some("provider-a"));
        assert_eq!(pending.connection_id.as_deref(), Some("connection-a"));
        assert_eq!(pending.model.as_deref(), Some("model-a"));
        assert_eq!(pending.thinking_level.as_deref(), Some("max"));
        assert_eq!(pending.temperature, Some(0.25));
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.active_reasoning_effort),
            Some(ReasoningEffortLevel::Max)
        );
    }

    /// Verifies a generation-root adapter reuses the durable active reasoning level instead of the Off sentinel.
    /// 验证代际根适配器复用可靠的当前推理档位，而不是使用 Off 哨兵值。
    #[test]
    fn exact_v1_dispatch_uses_active_reasoning_after_generation_rebase() {
        let session_id = SessionId::from_u128(0x6302);
        let mut document = SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Edited generation reasoning",
            1,
        );
        document.metadata.config.active_request.thinking_level = Some(String::from("max"));
        let handle = SessionHandle::with_backend(
            session_id,
            document,
            Arc::new(SubmissionRuntimeOptionsTestBackend),
        );

        let options = exact_v1_dispatch_runtime_options(handle.as_ref())
            .expect("active V1 reasoning should produce exact dispatch controls");

        assert_eq!(options.reasoning_effort, ReasoningEffortLevel::Max);
    }

    /// Verifies an explicit pending Off selection wins over the previous active reasoning level.
    /// 验证显式待生效 Off 选择优先于上一份当前推理档位。
    #[test]
    fn exact_v1_dispatch_preserves_pending_off_selection() {
        let session_id = SessionId::from_u128(0x6303);
        let mut document = SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Pending Off reasoning",
            1,
        );
        document.metadata.config.active_request = ActiveRequestConfig {
            provider: Some(String::from("provider-a")),
            connection_id: Some(String::from("connection-a")),
            model: Some(String::from("model-a")),
            thinking_level: Some(String::from("max")),
            temperature: None,
        };
        document
            .set_pending_request(
                PendingRequestConfig {
                    provider: Some(String::from("provider-a")),
                    connection_id: Some(String::from("connection-a")),
                    model: Some(String::from("model-a")),
                    thinking_level: None,
                    temperature: None,
                    set_at_ms: 2,
                },
                2,
            )
            .expect("pending Off selection should be accepted");
        let handle = SessionHandle::with_backend(
            session_id,
            document,
            Arc::new(SubmissionRuntimeOptionsTestBackend),
        );

        let options = exact_v1_dispatch_runtime_options(handle.as_ref())
            .expect("pending Off selection should produce exact dispatch controls");

        assert_eq!(options.reasoning_effort, ReasoningEffortLevel::Off);
    }
}
