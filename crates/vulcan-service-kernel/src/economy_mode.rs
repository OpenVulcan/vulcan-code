//! Application services and runtime boundaries for per-session economy mode.
//! 会话节约用量模式的应用服务与运行边界。

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use crate::{SharedServiceBridgeState, current_v1_unix_millis, parse_session_id};
use serde::Serialize;
use vulcan_session_core::v1::document::SessionDocument;
use vulcan_session_core::v1::event::ToolCallEvent;

/// User-visible selection and the pending transition, without historical receipt contents.
/// 面向用户的选择与待生效状态，不携带历史回执正文。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModePreference {
    /// Selected mode for new user work.
    /// 新用户任务选择的模式。
    enabled: bool,
    /// Monotonic selection revision for concurrent clients.
    /// 并发客户端使用的单调偏好修订。
    revision: u64,
    /// An existing task or baseline preparation still owns the current context.
    /// 当前上下文仍由已有任务或基线准备阶段持有。
    pending: bool,
}

/// Reads or changes the preference of a regular root session and returns its committed view.
/// 读取或修改普通根会话偏好并返回已提交状态。
pub(crate) async fn preference(
    state: SharedServiceBridgeState,
    id: String,
    edit: Option<(bool, u64)>,
) -> Result<ModePreference, String> {
    let id = parse_session_id(&id).ok_or_else(|| "Invalid economy session identity.".to_owned())?;
    let registry = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or("Workspace is not ready.")?;
        if !crate::workspace_snapshot_contains_regular_session(workspace, id) {
            return Err("Economy mode requires a regular root session.".into());
        }
        guard.session_runtime_registry.clone()
    };
    let _lifecycle = registry
        .acquire_lifecycle_transition(id)
        .await
        .map_err(|e| e.to_string())?;
    let handle = crate::session_goal_lifecycle::session_v1_handle_from_state(&state, id).await?;
    let project = |doc: &SessionDocument| {
        let selection = &doc.metadata.config.economy;
        ModePreference {
            enabled: selection.enabled,
            revision: selection.revision,
            pending: selection.enabled && selection.initialize,
        }
    };
    let Some((enabled, revision)) = edit else {
        return handle.read(project).map_err(|e| e.to_string());
    };
    let now = current_v1_unix_millis()?;
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |doc| {
                let changed = doc.set_economy_mode(enabled, revision, now)?;
                Ok(if changed {
                    SessionMutationChange::Changed(project(doc))
                } else {
                    SessionMutationChange::Unchanged(project(doc))
                })
            },
        )
        .map(|r| r.value)
        .map_err(|e| e.to_string())
}

/// Captures the window once for new work and returns the exact baseline boundary when needed.
/// 为新任务固定一次窗口参数，并在需要时返回准确的初始化边界。
pub(crate) fn prepare_root_window(
    state: &SharedServiceBridgeState,
    runtime: &crate::AgentRuntimeBridge,
    id: vulcan_workbench_model::SessionId,
) -> Result<
    Option<(
        vulcan_session_core::v1::TurnId,
        vulcan_session_core::v1::Seq,
        u64,
    )>,
    String,
> {
    let registry = state
        .lock()
        .map_err(|e| e.to_string())?
        .session_runtime_registry
        .clone();
    let handle = match registry.get(id) {
        Ok(handle) => handle,
        Err(crate::session_runtime_registry_v1::SessionRuntimeError::SessionNotRegistered(_)) => {
            return Ok(None);
        }
        Err(error) => return Err(error.to_string()),
    };
    if !handle
        .read(|doc| doc.metadata.config.economy.enabled)
        .map_err(|e| e.to_string())?
    {
        return Ok(None);
    }
    let window = runtime
        .public_ai_runtime
        .configuration_snapshot()?
        .economy_window;
    let now = current_v1_unix_millis()?;
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |doc| {
                let changed = doc.prepare_economy_turn(window, now)?;
                let economy = &doc.metadata.config.economy;
                let baseline = economy
                    .active
                    .as_ref()
                    .filter(|active| {
                        economy.initialize
                            && !(doc.metadata.mode.goal.enabled
                                && doc.metadata.mode.goal.state
                                    == vulcan_session_core::v1::config::GoalState::Active)
                            && doc
                                .active_turn()
                                .is_some_and(|t| !t.closed && t.turn_id == active.turn_id)
                    })
                    .map(|a| (a.turn_id, a.start, economy.revision));
                Ok(if changed {
                    SessionMutationChange::Changed(baseline)
                } else {
                    SessionMutationChange::Unchanged(baseline)
                })
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(receipt.value)
}

/// Returns local coverage counters as of the exact request frontier; diagnostic output contains no user content.
/// 根据请求的精确前沿返回本地覆盖计数，诊断不携带用户正文。
pub(crate) fn diagnostics(
    doc: &SessionDocument,
    through: vulcan_session_core::v1::Seq,
) -> Option<serde_json::Value> {
    let receipts = &doc.metadata.config.economy.settled;
    let end = receipts.partition_point(|r| r.answer <= through);
    if end == 0 {
        return None;
    }
    let mut retained = 0;
    let mut summarized = 0;
    let mut last_reason = None;
    for receipt in &receipts[..end] {
        if receipt.retired_at.is_some_and(|at| at <= through) {
            continue;
        }
        retained += 1;
        summarized += usize::from(receipt.summary.is_some());
        if let Some(reason) = &receipt.retained_reason {
            last_reason = Some(
                serde_json::json!({ "turnId": receipt.turn.turn_id.value(), "reason": reason }),
            );
        }
    }
    Some(
        serde_json::json!({ "retained": retained, "summarized": summarized, "retired": end-retained, "lastRetainedReason": last_reason }),
    )
}

/// Completes a handoff using sealed source events and authenticated, consumed job notifications.
/// 用已封存的来源事件和模型确已消费的作业通知完成一次交接。
pub(crate) fn settle_document_turn(
    document: &mut SessionDocument,
    owner: vulcan_session_core::v1::TurnId,
    candidate: &vulcan_session_core::v1::economy::EconomyCandidate,
    locator: Option<&vulcan_workspace_store::workspace_store::SessionV1StorageLocator>,
    session_id: vulcan_workbench_model::SessionId,
) -> Result<(), vulcan_session_core::v1::DomainError> {
    use vulcan_session_core::v1::Seq;
    use vulcan_session_core::v1::event::{EventBody, UserInputSource};
    // A previous turn can still own a running job even when this turn only read a file.
    // 即使本轮只读取文件，同一会话的早先回合仍可能有后台作业在运行。
    let session = vulcan_agent_runtime::tooling::native::canonical_shell_job_session_id(
        &session_id.to_string(),
    );
    if vulcan_host_tools::shell_job::ShellJobRuntime::global().running_background_count(&session)
        > 0
    {
        return Ok(());
    }
    let prefix = match (
        document.metadata.config.economy.active.as_ref(),
        document.events().first(),
        locator,
    ) {
        (Some(turn), Some(first), Some(locator)) if turn.start < first.seq => {
            let until = Seq::try_new(first.seq.value() - 1)?;
            match locator.load_event_range(turn.start, until) {
                Ok(range) if range.state.segment_generation == document.segment_generation() => {
                    range.events
                }
                Ok(_) => Vec::new(),
                Err(error) => {
                    tracing::warn!(%error, "Economy settlement retained an unreadable turn range");
                    Vec::new()
                }
            }
        }
        _ => Vec::new(),
    };
    // A job outcome alone does not show whether its result reached this physical request.
    // 仅有作业终态无法证明其结果进入了本次物理请求。
    let delivered = prefix
        .iter()
        .chain(document.events())
        .take_while(
            |e| !matches!(&e.event, EventBody::ApiStarted(a) if a.api_call_id == candidate.api),
        )
        .filter(|e| e.turn_id == Some(owner))
        .filter_map(|e| {
            let EventBody::UserInput(input) = &e.event else {
                return None;
            };
            if input.input_source != UserInputSource::AgentMessage {
                return None;
            }
            let facts =
                vulcan_session_core::v1::shell_job_notice::ShellJobNoticeFacts::parse(&input.text)?;
            (input.client_action_id.as_str() == format!("shell-job-notice-{}", facts.job_id))
                .then_some(facts.job_id)
        })
        .collect();
    // Load the session's job index once if a long task or a restart has outlived the live job registry.
    // 长任务或重启超出运行时作业缓存时，按需读取一次当前会话的持久索引。
    let archive = std::cell::OnceCell::new();
    let job_records = || {
        archive
            .get_or_init(|| {
                let Some(locator) = locator else {
                    return Vec::new();
                };
                let path = locator
                    .session_directory()
                    .join(vulcan_host_tools::shell_job::SHELL_JOBS_DIRECTORY);
                match vulcan_host_tools::shell_job::read_session_job_records(&path) {
                    Ok(records) => records,
                    Err(error) => {
                        tracing::warn!(%error, "Could not verify archived shell job completion");
                        Vec::new()
                    }
                }
            })
            .as_slice()
    };
    document.settle_economy_turn(owner, candidate, &prefix, |tool| {
        can_settle_tool(&session, tool, &delivered, job_records)
    })
}

/// Checks tool completion using its session, durable invocation, delivered notices and archived job evidence.
/// 根据所属会话、可靠调用、已消费通知及归档作业证据核验工具是否完成。
fn can_settle_tool<'a>(
    session: &str,
    tool: &ToolCallEvent,
    delivered: &std::collections::BTreeSet<String>,
    archive: impl Fn() -> &'a [vulcan_host_tools::shell_job::ShellJobRecord],
) -> bool {
    use vulcan_host_tools::shell_job::{
        ShellJobFinal, ShellJobOrigin, ShellJobRuntime, ShellJobStatus,
    };
    if !tool.external_refs.is_empty() {
        return false;
    }
    let consumed = |id: &str, background: bool, outcome: Option<&ShellJobFinal>| {
        outcome.is_some_and(|result| result.status != ShellJobStatus::Lost)
            && (!background || delivered.contains(id))
    };
    match tool.tool_name.as_str() {
        "file"
        | "file_read"
        | "file_write"
        | "file_edit"
        | "apply_patch"
        | "ask"
        | "skill"
        | "list_agents"
        | "agent_orchestration_info"
        | "wait_agents_result"
        | "ask_question"
        | "reply_question"
        | "cancel_question"
        | "ask_parent"
        | "send_message"
        | "close_agent"
        | "interrupt_agent" => true,
        "shell_job" => {
            if tool.input.string_argument("action") == Some("list") {
                return true;
            }
            let Some(id) = tool.input.string_argument("job_id") else {
                return false;
            };
            if let Some(job) = ShellJobRuntime::global().job(session, id) {
                return consumed(id, job.is_background(), job.outcome().as_ref());
            }
            archive()
                .iter()
                .find(|record| record.job_id == id)
                .is_some_and(|record| consumed(id, record.background, record.outcome.as_ref()))
        }
        "shell_command" => {
            let origin = ShellJobOrigin {
                api_call_id: tool.api_call_id.value(),
                tool_call_id: tool.tool_call_id.value(),
            };
            // Rewind can reuse numerical IDs; match only jobs actually started during this tool invocation.
            // 回退可复用数字标识，因此只匹配这次工具调用期间真正启动的作业。
            let within_tool = |time: u64| {
                i64::try_from(time).ok().is_some_and(|start| {
                matches!((tool.started_at_ms, tool.finished_at_ms), (Some(from), Some(to)) if from <= start && start <= to)
            })
            };
            if let Some(job) = ShellJobRuntime::global().job_for_origin(session, origin)
                && within_tool(job.started_at_ms())
            {
                return consumed(job.id(), job.is_background(), job.outcome().as_ref());
            }
            let mut matches = archive().iter().filter(|record| {
                record.origin == Some(origin) && within_tool(record.started_at_ms)
            });
            matches.next().is_some_and(|record| {
                consumed(&record.job_id, record.background, record.outcome.as_ref())
            }) && matches.next().is_none()
        }
        _ => false,
    }
}

/// Archived command evidence must match the invocation and include a consumed background completion notice.
/// 归档作业证据必须与调用匹配，后台作业还需要已消费的完成通知。
#[cfg(test)]
#[test]
fn economy_archived_jobs_require_terminal_evidence_and_consumption() {
    use vulcan_host_tools::shell_job::{
        ShellJobFinal, ShellJobOrigin, ShellJobRecord, ShellJobStatus,
    };
    let tool: ToolCallEvent = serde_json::from_value(serde_json::json!({
        "api_call_id": 1, "tool_call_id": 2, "request_stream_id": "command", "tool_name": "shell_command",
        "input": {}, "status": "completed", "started_at_ms": 100, "finished_at_ms": 104
    })).unwrap();
    let mut job = ShellJobRecord {
        job_id: "job".into(),
        command: "fixture".into(),
        purpose: String::new(),
        shell: String::new(),
        cwd: String::new(),
        started_at_ms: 102,
        origin: Some(ShellJobOrigin {
            api_call_id: 1,
            tool_call_id: 2,
        }),
        background: true,
        outcome: None,
    };
    let mut delivered = std::collections::BTreeSet::new();
    let check = |record: &ShellJobRecord, notices: &_| {
        can_settle_tool("isolated-economy-job-fixture", &tool, notices, || {
            std::slice::from_ref(record)
        })
    };
    assert!(!check(&job, &delivered));
    job.outcome = Some(ShellJobFinal {
        status: ShellJobStatus::Completed,
        exit_code: Some(0),
        duration_ms: 2,
        finished_at_ms: 110,
        failure_reason: None,
    });
    assert!(!check(&job, &delivered));
    delivered.insert(job.job_id.clone());
    assert!(check(&job, &delivered));
    job.outcome.as_mut().unwrap().status = ShellJobStatus::Lost;
    assert!(!check(&job, &delivered));
    job.outcome.as_mut().unwrap().status = ShellJobStatus::Completed;
    job.started_at_ms = 30;
    assert!(!check(&job, &delivered));
}
