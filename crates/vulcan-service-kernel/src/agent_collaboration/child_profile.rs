//! Host-only durable capture of resolved child execution configuration.
//! 仅宿主使用的已解析子执行配置可靠捕获。

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest;
use vulcan_session_core::v1::DomainError;

/// Persists `request` only while its exact TaskRuntime execution still owns the child document.
/// 仅在精确 TaskRuntime 执行仍拥有子文档时持久化 `request`。
/// `state` supplies the canonical registries; returns after durable publication or an explicit error.
/// `state` 提供规范注册表；可靠发布后返回，否则返回明确错误。
pub(super) async fn capture(
    state: &SharedServiceBridgeState,
    request: ChildExecutionProfileCaptureRequest,
) -> Result<(), String> {
    // Copy runtime handles without retaining the service-state lock across document I/O.
    // 复制运行时句柄，避免服务状态锁跨越文档 I/O。
    let (registry, task_runtime) = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        let runtime = guard.agent_runtime.as_ref().ok_or_else(|| {
            String::from("Child profile capture requires an initialized runtime.")
        })?;
        (
            Arc::clone(&guard.session_runtime_registry),
            Arc::clone(&runtime.task_runtime),
        )
    };
    let session_id = request.task_id;
    let handle = registry
        .get(session_id)
        .map_err(|error| error.to_string())?;
    let now_ms = now_ms()?;
    mutate_handle_with_admission(
        &registry,
        &handle,
        session_id,
        SessionMutationAdmission::ExistingTurn,
        move |document| {
            // The runtime proof is re-read after canonical lifecycle admission, not from caller-provided labels.
            // 在规范生命周期准入之后重读运行时凭据，不信任调用方提供的标签。
            let facts = task_runtime
                .active_task_v1_execution_facts(session_id, &request.execution_id)
                .map_err(|_| DomainError::CollaborationStateViolation {
                    field: "child_execution_profile.runtime_owner",
                })?;
            if facts.task_id != session_id
                || facts.root_session_id != request.root_session_id
                || facts.parent_session_id != request.parent_session_id
                || facts.display_title != request.runtime_identity.display_title
                || facts.prompt != request.invocation_config.prompt
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "child_execution_profile.runtime_binding",
                });
            }
            let turn = document
                .active_turn()
                .filter(|turn| !turn.closed)
                .ok_or(DomainError::NoActiveTurn)?;
            let profile = request.bind_child_turn(document.segment_generation(), turn.turn_id)?;
            Ok(
                if document.capture_child_execution_profile(profile, now_ms)? {
                    IdempotentMutation::Committed(())
                } else {
                    IdempotentMutation::Idempotent(())
                },
            )
        },
    )
    .await?;
    Ok(())
}
