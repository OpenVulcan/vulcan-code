//! Publishes an admitted child execution using its explicit initial or resumption authority.
//! 使用明确的初始或续跑权威发布已准入子执行。

use super::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskRecord, CollaborationChildTaskState,
};

/// Publishes the exact stored creation intent and transfers its canonical child to a listener.
/// 发布精确持久创建意图，并将规范子句柄交给监听器。
/// Callers retain root, parent and child lifecycle gates; `record` is their durable reservation receipt.
/// 调用方保留根、父及子生命周期锁；`record` 是其可靠预留回执。
/// `admit` performs synchronous listener handoff; errors close only the child owned by this creation.
/// `admit` 执行同步监听交接；失败只闭合本次创建拥有的子任务。
pub(super) fn publish(
    registry: &SessionRuntimeRegistry,
    root: &SessionHandle,
    parent: &SessionHandle,
    record: &CollaborationChildTaskRecord,
    admit: impl FnOnce(Arc<SessionHandle>) -> Result<(), String>,
) -> Result<(), String> {
    // Read full durable authority again; a discovery record cannot create storage on its own.
    // 再次读取完整可靠权威；发现记录自身不能授权创建存储。
    let intent = parent
        .read_full_durable(|document| {
            if record.state != CollaborationChildTaskState::Reserved
                || document.segment_generation() != record.parent_segment_generation
                || document
                    .collaboration_state()
                    .child_tasks
                    .get(&record.execution_id)
                    != Some(record)
                || document.metadata.archived_at_ms.is_some()
                || matches!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
                )
            {
                return Err(String::from(
                    "Child publication no longer owns its parent reservation.",
                ));
            }
            require_open_parent_turn(document, record.parent_turn_id)
                .map_err(|error| error.to_string())?;
            document
                .metadata
                .child_creation_intents
                .get(&record.execution_id)
                .cloned()
                .ok_or_else(|| String::from("Child publication has no durable creation intent."))
        })
        .map_err(|error| error.to_string())??;
    // The root's storage facade remains the only authority for embedded paths.
    // 根存储门面仍是嵌入路径的唯一权威。
    let root_locator = root.storage_locator().map_err(|error| error.to_string())?;
    if intent.profile.root_session_id != root.session_id().to_string()
        || intent.project_tag != root_locator.project_id()
        || record.child.root_session_id != root_locator.session_id()
        || record.child.project_id != root_locator.project_id()
    {
        return Err(String::from(
            "Child publication has conflicting root storage ownership.",
        ));
    }
    // IDs and profile come from the validated durable intent, never from current UI selection.
    // 身份与配置来自已校验可靠意图，绝不来自当前界面选择。
    let child_id = Uuid::parse_str(&record.child.session_id).map_err(|error| error.to_string())?;
    let locator = locate_embedded_task_session_v1(&root_locator, &record.child.session_id)
        .map_err(|error| error.to_string())?;
    let profile = intent.profile.clone();
    // Retain the just-written child even if registry publication subsequently rejects the load.
    // 即使注册表发布随后拒绝加载，仍保留刚写出的子任务。
    let mut written_child: Option<Arc<SessionHandle>> = None;
    let loaded = match intent.origin {
        vulcan_session_core::v1::child_execution::ChildCreationOrigin::Initial => registry
            .get_or_load(child_id, || {
                // Registry admission happens before disk creation, so shutdown cannot produce an unregistered file first.
                // 注册表准入先于磁盘创建，因此关闭不会先产生未注册文件。
                let mut document = intent.rebuild_initial_document()?;
                let candidate = document
                    .prepare_flush_candidates()?
                    .ok_or(SessionRuntimeError::MutationProducedNoRevision)?;
                let created = create_embedded_task_session_v1(
                    &root_locator,
                    &CreateEmbeddedTaskSessionV1Request {
                        metadata: &candidate.metadata,
                        runtime_lock: &candidate.runtime_lock,
                        compactions: &candidate.compactions,
                        events: &candidate.tail_events,
                    },
                )?;
                document.apply_published_storage_metadata(&created.metadata)?;
                document.advance_durable(candidate.revision)?;
                let handle = SessionHandle::new(child_id, created.locator, document);
                written_child = Some(handle.clone());
                Ok(handle)
            }),
        vulcan_session_core::v1::child_execution::ChildCreationOrigin::Resumption { .. } => {
            (|| {
                let child = registry.get(child_id)?;
                if child.storage_locator()? != locator {
                    return Err(SessionRuntimeError::ProviderProjectionInvariant(
                        String::from("Child resumption has a different canonical locator."),
                    ));
                }
                child.mutate_durable_if_changed(
                    SessionMutationAdmission::RootTurn,
                    SessionFlushReason::InputAccepted,
                    |document| {
                        let changed = document.publish_child_resumption(&intent)?;
                        // Ownership begins only after the original successor was accepted, including uncertain flush failure.
                        // 仅在原后继被接纳后取得归属，也覆盖刷写失败结果不确定的情形。
                        written_child = Some(child.clone());
                        Ok(if changed {
                            SessionMutationChange::Changed(())
                        } else {
                            SessionMutationChange::Unchanged(())
                        })
                    },
                )?;
                Ok(child)
            })()
        }
    };
    // A late registry rejection still leaves this creator responsible for its exact initial turn.
    // 迟到的注册表拒绝仍由本创建者负责精确初始回合。
    let child = match loaded {
        Ok(child) => child,
        Err(error) => {
            let primary = error.to_string();
            if let Some(child) = written_child {
                let cleanup = crate::agent_stream::startup_rejection::capture_owner(
                    &child,
                    record.execution_id.as_str(),
                )
                .and_then(|owner| {
                    crate::agent_stream::startup_rejection::close_rejected_child(
                        &child, owner, &primary,
                    )
                });
                if let Err(cleanup) = cleanup {
                    return Err(format!("{primary} Child closure failed: {cleanup}"));
                }
            }
            return Err(primary);
        }
    };
    if child.storage_locator().map_err(|error| error.to_string())? != locator
        || !child
            .read_full_durable(|document| {
                document.metadata.child_execution_profile.as_ref() == Some(&profile)
                    && document.active_api().is_none()
                    && document.metadata.archived_at_ms.is_none()
                    && document.collaboration_run_control() == CollaborationRunControl::Running
            })
            .map_err(|error| error.to_string())?
    {
        return Err(String::from(
            "Child publication found a different canonical execution.",
        ));
    }
    // Freeze the original open turn before a rejecting listener may already close it itself.
    // 在拒绝监听器可能自行闭合之前冻结原开放回合。
    let owner = crate::agent_stream::startup_rejection::capture_owner(
        &child,
        record.execution_id.as_str(),
    )?;
    child.bind_child_usage(root, parent.session_id(), intent.created_at_ms);
    match admit(child.clone()) {
        Ok(()) => Ok(()),
        Err(primary) => match crate::agent_stream::startup_rejection::close_rejected_child(
            &child, owner, &primary,
        ) {
            Ok(()) => Err(primary),
            Err(cleanup) => Err(format!("{primary} Child closure failed: {cleanup}")),
        },
    }
}
