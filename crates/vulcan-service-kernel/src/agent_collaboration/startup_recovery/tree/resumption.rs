//! Restores a parent-committed successor before generic interruption recovery changes its source.
//! 在通用中断恢复改变来源之前恢复父侧已提交后继。

use super::*;
use vulcan_session_core::v1::child_execution::ChildCreationOrigin;

/// Loads one exact child and publishes its pending successor before projecting process interruptions.
/// 加载精确子任务，并在投影进程中断之前发布其待办后继。
/// Callers retain root, parent and child lifecycle gates; running children must use ordinary loading.
/// 调用方持有根、父、子生命周期锁；运行中子任务必须使用普通加载。
/// `registry` owns canonical handles; `root_generation` is the discovered root generation, not a refreshed value.
/// `registry` 拥有规范句柄；`root_generation` 为发现时的根代次，而非刷新后的值。
/// `parent` owns `records`; `locator` is the exact root-derived child path and `root` owns that storage tree.
/// `parent` 拥有 `records`；`locator` 为根派生的精确子路径，`root` 拥有该存储树。
/// Returns the canonical handle; changed sources are reported without reconstructing or replacing history.
/// 返回规范句柄；来源变化时报告错误，不重建或替换历史。
pub(in crate::agent_collaboration) fn load(
    registry: &Arc<SessionRuntimeRegistry>,
    root: &SessionHandle,
    root_generation: u64,
    parent: &SessionHandle,
    locator: SessionV1StorageLocator,
    records: &[CollaborationChildTaskRecord],
) -> Result<Arc<SessionHandle>, String> {
    let child_id = Uuid::parse_str(locator.session_id()).map_err(|error| error.to_string())?;
    let root_available = root
        .read_full_durable(|document| {
            if document.segment_generation() != root_generation {
                return Err(String::from(
                    "Resumption recovery root generation was replaced.",
                ));
            }
            Ok(document.metadata.archived_at_ms.is_none()
                && !matches!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
                ))
        })
        .map_err(|error| error.to_string())??;
    let intent = parent
        .read_full_durable(|document| {
            if !root_available
                || document.metadata.archived_at_ms.is_some()
                || matches!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
                )
            {
                return Ok(None);
            }
            // The current unique Reserved execution determines authority; terminal history never supplies it.
            // 当前唯一 Reserved 执行决定权威；终态历史绝不提供该权威。
            let mut pending = None;
            for discovered in records {
                let current = document
                    .collaboration_state()
                    .child_tasks
                    .get(&discovered.execution_id)
                    .ok_or_else(|| {
                        String::from("Resumption recovery lost its parent reservation.")
                    })?;
                if current.state != CollaborationChildTaskState::Reserved {
                    continue;
                }
                let Some(intent) = document
                    .metadata
                    .child_creation_intents
                    .get(&current.execution_id)
                else {
                    continue;
                };
                if !matches!(intent.origin, ChildCreationOrigin::Resumption { .. }) {
                    continue;
                }
                require_open_parent_turn(document, current.parent_turn_id)
                    .map_err(|error| error.to_string())?;
                if current != discovered
                    || current.parent_segment_generation != document.segment_generation()
                    || current.child.session_id != locator.session_id()
                    || current.child.project_id != locator.project_id()
                    || current.child.root_session_id != root.session_id().to_string()
                    || locate_embedded_task_session_v1(
                        &root.storage_locator().map_err(|error| error.to_string())?,
                        locator.session_id(),
                    )
                    .map_err(|error| error.to_string())?
                        != locator
                    || pending.replace(intent.clone()).is_some()
                {
                    return Err(String::from(
                        "Resumption recovery has conflicting publication ownership.",
                    ));
                }
            }
            Ok(pending)
        })
        .map_err(|error| error.to_string())??;
    let Some(intent) = intent else {
        return load_embedded_canonical_handle(registry, locator, child_id);
    };
    // Do not project old API interruption here: it would invalidate the parent's exact source revision.
    // 此处不投影旧 API 中断，否则会使父侧精确来源修订失效。
    let child = registry
        .get_or_load(child_id, || {
            let loaded = load_session_v1_for_runtime(&locator)?;
            if loaded.metadata.session_id != child_id.to_string() {
                return Err(SessionRuntimeError::LoadedSessionDocumentIdentityMismatch {
                    expected: child_id,
                    actual: loaded.metadata.session_id,
                });
            }
            Ok(SessionHandle::new(
                child_id,
                loaded.locator,
                loaded.document,
            ))
        })
        .map_err(|error| error.to_string())?;
    if child.storage_locator().map_err(|error| error.to_string())? != locator {
        return Err(String::from(
            "Resumption recovery canonical locator was replaced.",
        ));
    }
    child
        .mutate_durable_if_changed(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                Ok(if document.publish_child_resumption(&intent)? {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(child)
}
