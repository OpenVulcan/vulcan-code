//! The Plan artifact that still awaits the user's decision.
//! 仍在等待用户决定的计划产物。
//!
//! While a session is in Plan mode, its latest successful `submit_plan` result is pending until the user decides:
//! a later user message, a Plan-mode switch, or an explicit decision in this process ends it. The V1 transcript
//! holds the submitted plan, so the pending plan survives a Core restart without any extra persisted field; only a
//! decision that leaves no transcript trace (dismissing the panel) is remembered in memory.
//! 会话处于计划模式时，最近一次成功的 `submit_plan` 结果在用户决定前保持待定：之后的用户消息、计划模式切换，或
//! 本进程内的明确决定都会结束它。计划内容保存在 V1 转录中，因此待定计划无需额外持久字段即可在 Core 重启后恢复；
//! 只有不留转录痕迹的决定（关闭面板）记在内存中。

use super::*;
use std::collections::HashMap;
use vulcan_session_core::v1::document::SessionDocument;
use vulcan_session_core::v1::event::{EventBody, ToolCallStatus};
use vulcan_session_core::v1::ids::Seq;

/// Tool whose result carries a submitted plan.
/// 其结果携带已提交计划的工具。
const SUBMIT_PLAN_TOOL_ID: &str = "submit_plan";

/// Host result kind of a submitted plan.
/// 已提交计划的宿主结果类型。
const PLAN_ARTIFACT_HOST_KIND: &str = "plan_artifact";

/// System-context key that the Plan-mode switch writes into the transcript.
/// 计划模式切换写入转录的系统上下文键。
const PLAN_MODE_CONTEXT_KEY: &str = "plan_mode";

/// Submitted plans the user already decided on in this process, as the transcript seq of each session's decided call.
/// 本进程内用户已做出决定的已提交计划，按会话记录已决定调用在转录中的 seq。
static DECIDED_PLAN_SEQS: std::sync::OnceLock<std::sync::Mutex<HashMap<SessionId, Seq>>> =
    std::sync::OnceLock::new();

/// Returns the process-local decision registry.
/// 返回进程内决定注册表。
fn decided_plan_seqs() -> &'static std::sync::Mutex<HashMap<SessionId, Seq>> {
    DECIDED_PLAN_SEQS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Parses one `plan_artifact` host result; any other kind or an incomplete payload yields `None`.
/// 解析一个 `plan_artifact` 宿主结果；其他类型或不完整载荷返回 `None`。
fn plan_artifact_from_host_result(host_result: &Value) -> Option<SessionPlanArtifactState> {
    if host_result.get("kind").and_then(Value::as_str) != Some(PLAN_ARTIFACT_HOST_KIND) {
        return None;
    }
    let payload = host_result.get("payload")?;
    let text = |field: &str| {
        payload
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    };
    Some(SessionPlanArtifactState {
        name: text("name")?,
        path: text("path")?,
        content: payload
            .get("content")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string),
        updated_at: text("updatedAt")?,
    })
}

/// Reads the submitted plan carried by one finished `submit_plan` runtime event.
/// 读取单个已完成 `submit_plan` 运行时事件携带的已提交计划。
pub(crate) fn plan_artifact_from_tool_finished_event(
    event: &RuntimeEvent,
) -> Option<SessionPlanArtifactState> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return None;
    }
    plan_artifact_from_host_result(event.metadata.as_ref()?.get("host_result")?)
}

/// Finds the latest successful `submit_plan` that no later user message or Plan-mode switch has answered.
/// 查找之后没有用户消息或计划模式切换回应的最近一次成功 `submit_plan`。
fn latest_unanswered_plan(document: &SessionDocument) -> Option<(Seq, SessionPlanArtifactState)> {
    for event in document.events().iter().rev() {
        match &event.event {
            EventBody::UserInput(_) => return None,
            EventBody::SystemContext(context) if context.context_key == PLAN_MODE_CONTEXT_KEY => {
                return None;
            }
            EventBody::ToolCall(tool)
                if tool.tool_name == SUBMIT_PLAN_TOOL_ID
                    && tool.status == ToolCallStatus::Completed =>
            {
                let output = serde_json::to_value(tool.output.as_ref()?).ok()?;
                if output.get("is_error").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                if let Some(artifact) = output
                    .get("host_result")
                    .and_then(plan_artifact_from_host_result)
                {
                    return Some((event.seq(), artifact));
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns the plan that still awaits the user's decision in one session document.
/// 返回单个会话文档中仍在等待用户决定的计划。
pub(crate) fn pending_plan_artifact_from_document(
    document: &SessionDocument,
    session_id: SessionId,
) -> Option<SessionPlanArtifactState> {
    if !document.metadata.mode.plan.enabled {
        return None;
    }
    let (seq, artifact) = latest_unanswered_plan(document)?;
    let decided = decided_plan_seqs()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&session_id)
        .is_some_and(|decided| *decided >= seq);
    (!decided).then_some(artifact)
}

/// Records that the user decided on the session's latest submitted plan, so it is no longer pending.
/// 记录用户已对会话最近一次提交的计划做出决定，使其不再待定。
pub(crate) fn record_plan_decision(document: &SessionDocument, session_id: SessionId) {
    let Some((seq, _)) = latest_unanswered_plan(document) else {
        return;
    };
    decided_plan_seqs()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(session_id, seq);
}

/// Shows a plan that a `submit_plan` call just finished as pending, then publishes the session addons.
/// 将刚完成的 `submit_plan` 调用提交的计划显示为待定，然后发布会话附属状态。
pub(crate) fn publish_submitted_plan_artifact(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    artifact: SessionPlanArtifactState,
) -> Result<(), String> {
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(workspace) = guard.workspace_snapshot.as_mut() else {
            return Ok(());
        };
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            });
        runtime_state.plan_mode.artifact = Some(artifact);
    }
    emit_session_addons_changed_delta(app, state, session_id).map(|_| ())
}

/// Restores the Plan switch and any pending plan of a session being selected, so navigation after a Core restart
/// shows the plan decision instead of the composer. Only a session in Plan mode loads its transcript.
/// 恢复正在选中会话的计划开关及待定计划，使 Core 重启后的导航显示计划决定面板而非输入框。只有处于计划模式的
/// 会话才加载转录。
pub(crate) async fn restore_selected_session_plan_mode(
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    let registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .session_runtime_registry,
    );
    let plan_enabled = match registry.get(session_id) {
        Ok(handle) => handle
            .read(|document| document.metadata.mode.plan.enabled)
            .map_err(|error| format!("Failed to read the session Plan mode: {error}"))?,
        Err(crate::session_runtime_registry_v1::SessionRuntimeError::SessionNotRegistered(_)) => {
            vulcan_workspace_store::workspace_store::locate_session_v1_storage(
                controller_runtime,
                &session_id.to_string(),
            )
            .await
            .map_err(|error| format!("Failed to locate the session Plan mode: {error}"))?
            .read_plan_mode_enabled()
            .map_err(|error| format!("Failed to read the session Plan mode: {error}"))?
        }
        Err(error) => return Err(error.to_string()),
    };
    let artifact = if plan_enabled {
        session_goal_lifecycle::session_v1_handle_from_state(state, session_id)
            .await?
            .read(|document| pending_plan_artifact_from_document(document, session_id))
            .map_err(|error| format!("Failed to read the pending plan: {error}"))?
    } else {
        None
    };
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace) = guard.workspace_snapshot.as_mut() {
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_default();
        if plan_enabled {
            runtime_state.active_work_mode = Some(String::from("plan"));
        } else if runtime_state.active_work_mode.as_deref() == Some("plan") {
            runtime_state.active_work_mode = None;
        }
        runtime_state.plan_mode.artifact = artifact;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vulcan_session_core::v1::document::PlanModeTransitionInput;

    fn plan_host_result(name: &str) -> Value {
        json!({
            "kind": PLAN_ARTIFACT_HOST_KIND,
            "payload": {"name": name, "path": "docs/plan/a.md", "content": "# Plan", "updatedAt": "1"}
        })
    }

    #[test]
    fn host_result_parses_only_complete_plan_artifacts() {
        let artifact = plan_artifact_from_host_result(&plan_host_result("verify"))
            .expect("a complete plan artifact should parse");
        assert_eq!(artifact.name, "verify");
        assert_eq!(artifact.content.as_deref(), Some("# Plan"));
        assert!(
            plan_artifact_from_host_result(&json!({"kind": "todolist", "payload": {}})).is_none()
        );
        assert!(
            plan_artifact_from_host_result(
                &json!({"kind": PLAN_ARTIFACT_HOST_KIND, "payload": {"name": "x"}})
            )
            .is_none()
        );
    }

    #[test]
    fn finished_submit_plan_event_carries_its_artifact() {
        let finished = RuntimeEvent::new(RuntimeEventKind::ToolFinished, "submit_plan", "")
            .with_metadata(Some(json!({"host_result": plan_host_result("verify")})));
        assert!(plan_artifact_from_tool_finished_event(&finished).is_some());
        let started = RuntimeEvent::new(RuntimeEventKind::ToolStarted, "submit_plan", "")
            .with_metadata(Some(json!({"host_result": plan_host_result("verify")})));
        assert!(plan_artifact_from_tool_finished_event(&started).is_none());
    }

    #[test]
    fn a_document_without_plan_mode_has_no_pending_plan() {
        let session_id = SessionId::from_u128(0x51a);
        let mut document = SessionDocument::new(session_id.to_string(), "project", "title", 1);
        assert!(pending_plan_artifact_from_document(&document, session_id).is_none());
        let enter = PlanModeTransitionInput::try_new(true, "plan context").expect("plan input");
        assert!(document.set_plan_mode(enter, 2).expect("enter plan mode"));
        // Entering Plan mode writes the switch itself, which answers every earlier plan.
        // 进入计划模式会写入切换本身，回应此前所有计划。
        assert!(pending_plan_artifact_from_document(&document, session_id).is_none());
    }
}
