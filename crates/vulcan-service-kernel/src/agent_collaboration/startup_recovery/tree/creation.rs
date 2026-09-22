//! Rebuilds an unpublished initial child from its parent's durable creation intent.
//! 从父侧可靠创建意图重建尚未发布的初始子任务。

use super::*;
use vulcan_workspace_store::workspace_store::{
    CreateEmbeddedTaskSessionV1Request, create_embedded_task_session_v1,
};

/// Publishes only the initial child owned by the exact still-reserved parent execution.
/// 仅发布精确且仍处于预留状态的父执行所拥有的初始子任务。
/// Callers hold root, parent and child lifecycle gates; existing child publications are preserved.
/// 调用方持有根、父及子生命周期锁；保留已有子发布。
/// Returns its metadata snapshot, None for stopped work, or an explicit ownership/storage error.
/// 返回元数据快照；停止工作返回 None；归属或存储异常返回明确错误。
pub(in crate::agent_collaboration::startup_recovery) fn publish_missing(
    root: &SessionHandle,
    parent: &SessionHandle,
    root_generation: u64,
    parent_generation: u64,
    locator: &SessionV1StorageLocator,
    record: &CollaborationChildTaskRecord,
) -> Result<Option<SessionV1CollaborationRecoverySnapshot>, String> {
    // Another legitimate publisher may have completed while discovery waited for the gates.
    // 发现等待锁期间，另一个合法发布者可能已经完成发布。
    if let Some(snapshot) = snapshot_if_present(locator)? {
        return Ok(Some(snapshot));
    }
    let root_locator = root.storage_locator().map_err(|error| error.to_string())?;
    if locate_embedded_task_session_v1(&root_locator, locator.session_id())
        .map_err(|error| error.to_string())?
        != *locator
        || record.child.root_session_id != root.session_id().to_string()
        || record.child.project_id != root_locator.project_id()
        || record.child.session_id != locator.session_id()
        || record.task_id.as_str() != locator.session_id()
    {
        return Err(String::from(
            "Missing child creation has conflicting storage ownership.",
        ));
    }
    // A root-wide stop must prevent rebuilding descendants even when the direct parent is unchanged.
    // 即使直接父节点未变化，整棵根树停止也必须阻止重建后代。
    let stopped = root
        .read_full_durable(|document| {
            if document.segment_generation() != root_generation {
                return Err(String::from(
                    "Missing child creation root generation was replaced.",
                ));
            }
            Ok(document.metadata.archived_at_ms.is_some()
                || matches!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
                ))
        })
        .map_err(|error| error.to_string())??;
    if stopped {
        return Ok(None);
    }
    let intent = parent
        .read_full_durable(|document| {
            if document.metadata.archived_at_ms.is_some()
                || matches!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
                )
            {
                return Ok(None);
            }
            if document.segment_generation() != parent_generation
                || record.parent_segment_generation != parent_generation
                || document
                    .collaboration_state()
                    .child_tasks
                    .get(&record.execution_id)
                    != Some(record)
                || record.state != CollaborationChildTaskState::Reserved
                || document
                    .active_turn()
                    .is_none_or(|turn| turn.closed || turn.turn_id != record.parent_turn_id)
            {
                return Err(String::from(
                    "Missing child creation no longer owns the original reservation.",
                ));
            }
            // Full durable loading validates the intent against the exact record, generation and project.
            // 完整可靠加载校验意图与精确记录、代次及项目的一致性。
            document
                .metadata
                .child_creation_intents
                .get(&record.execution_id)
                .cloned()
                .map(Some)
                .ok_or_else(|| {
                    String::from("Missing child has no durable initial creation intent.")
                })
        })
        .map_err(|error| error.to_string())??;
    let Some(intent) = intent else {
        return Ok(None);
    };
    let mut document = intent
        .rebuild_initial_document()
        .map_err(|error| error.to_string())?;
    let candidate = document
        .prepare_flush_candidates()
        .map_err(|error| error.to_string())?
        .ok_or("Rebuilt initial child has no publication candidate.")?;
    // The production staging writer checks any preexisting directory instead of overwriting it.
    // 生产暂存写入器检查任何已存在目录，绝不覆盖。
    let created = create_embedded_task_session_v1(
        &root_locator,
        &CreateEmbeddedTaskSessionV1Request {
            metadata: &candidate.metadata,
            runtime_lock: &candidate.runtime_lock,
            compactions: &candidate.compactions,
            events: &candidate.tail_events,
        },
    )
    .map_err(|error| error.to_string())?;
    if created.locator != *locator {
        return Err(String::from(
            "Rebuilt child publication returned a different locator.",
        ));
    }
    // Canonical hydration remains in load_selected, which rechecks the parent's original discovery edge.
    // 规范水合仍由 load_selected 完成，其会重查父侧原始发现边。
    snapshot_if_present(locator)?
        .map(Some)
        .ok_or_else(|| String::from("Rebuilt child publication is still missing."))
}

/// Loads the exact parent ancestry and excludes concurrent lifecycle changes before initial publication.
/// 加载精确父祖先链，并在首次发布前排除并发生命周期变更。
/// `records` are discovery hints only; returns the preserved or recreated child snapshot.
/// `records` 仅为发现线索；返回保留或重建后的子快照。
pub(super) async fn recover_missing(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    nodes: &[RecoveryNode],
    parent_index: usize,
    locator: &SessionV1StorageLocator,
    records: &[CollaborationChildTaskRecord],
) -> Result<Option<SessionV1CollaborationRecoverySnapshot>, String> {
    // Terminal history does not authorize creating a new physical session.
    // 终态历史不授权创建新的物理会话。
    let mut live = records
        .iter()
        .filter(|record| record.state != CollaborationChildTaskState::Terminal);
    let Some(record) = live.next() else {
        return Ok(None);
    };
    if live.next().is_some() || record.state != CollaborationChildTaskState::Reserved {
        return Err(String::from(
            "Missing child does not have one initial reserved execution.",
        ));
    }
    worker::require_active_owner(state, registry)?;
    let child_id = Uuid::parse_str(locator.session_id()).map_err(|error| error.to_string())?;
    let runtime = state
        .lock()
        .map_err(|error| error.to_string())?
        .agent_runtime
        .clone();
    if let Some(runtime) = &runtime
        && runtime.task_runtime.is_task_running(child_id)?
    {
        return Ok(None);
    }
    let parent = load_selected(state, registry, nodes, parent_index).await?;
    let root_id =
        Uuid::parse_str(nodes[0].locator.session_id()).map_err(|error| error.to_string())?;
    let root = registry.get(root_id).map_err(|error| error.to_string())?;
    // The unborn child has no handle yet, but its UUID already participates in canonical lock ordering.
    // 尚未创建的子任务没有句柄，但其 UUID 已参与规范锁顺序。
    let mut gates = Vec::new();
    for identity in BTreeSet::from([root_id, parent.session_id(), child_id]) {
        gates.push(
            registry
                .acquire_lifecycle_transition(identity)
                .await
                .map_err(|error| error.to_string())?,
        );
    }
    let state = state.clone();
    let registry = registry.clone();
    let locator = locator.clone();
    let record = record.clone();
    let parent_generation = nodes[parent_index].snapshot.segment_generation;
    let root_generation = nodes[0].snapshot.segment_generation;
    crate::spawn_service_blocking_task(move || {
        // Guard ownership follows the blocking write even if its async waiter is cancelled.
        // 即使异步等待者取消，守卫所有权仍跟随阻塞写入。
        let _gates = gates;
        worker::require_active_owner(&state, &registry)?;
        for handle in [&root, &parent] {
            if !Arc::ptr_eq(
                &registry
                    .get(handle.session_id())
                    .map_err(|error| error.to_string())?,
                handle,
            ) {
                return Err(String::from(
                    "Missing child creation ancestor was replaced.",
                ));
            }
        }
        let runtime = state
            .lock()
            .map_err(|error| error.to_string())?
            .agent_runtime
            .clone();
        if let Some(runtime) = runtime
            && runtime.task_runtime.is_task_running(child_id)?
        {
            return Ok(None);
        }
        // A resident child proves prior publication; missing disk storage cannot be repaired from its initial intent.
        // 常驻子句柄证明曾经发布；不能用初始意图修复其缺失的磁盘存储。
        if snapshot_if_present(&locator)?.is_none() {
            match registry.get(child_id) {
                Ok(_) => {
                    return Err(String::from(
                        "Missing child storage still has a canonical resident owner.",
                    ));
                }
                Err(SessionRuntimeError::SessionNotRegistered(_)) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        publish_missing(
            &root,
            &parent,
            root_generation,
            parent_generation,
            &locator,
            &record,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}
