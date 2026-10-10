//! Verified participant identity across indexed roots and embedded task sessions.
//! 跨索引根会话与内嵌任务会话的已验证参与者身份。

use super::*;

#[cfg(test)]
mod admission_tests;

/// Rechecks message admission inside the target's canonical document mutation.
/// 在目标规范文档修改内部重新检查消息准入。
/// `source` and `target` are authenticated participants; returns an explicit admission error.
/// `source` 与 `target` 是已认证参与者；拒绝时返回明确准入错误。
/// MainOnly authority comes from this parent's exact child reservation and retained turn.
/// MainOnly 权限来自此父文档的精确子任务预留及保留回合。
pub(super) fn require_document_message_admission(
    document: &vulcan_session_core::v1::document::SessionDocument,
    source: &CollaborationParticipant,
    target: &CollaborationParticipant,
) -> Result<(), vulcan_session_core::v1::errors::DomainError> {
    use vulcan_session_core::v1::errors::DomainError;
    match document.collaboration_run_control() {
        CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents => {
            Err(DomainError::CollaborationUnauthorized {
                action: "message_target_stopped",
                identity: target.session_id.clone(),
            })
        }
        CollaborationRunControl::MainOnly => {
            // Do not infer a parent from a shared root: only a local reservation grants this exception.
            // 不能从共同根节点推断父子关系；只有本地预留授予此例外。
            let admitted = document
                .collaboration_state()
                .child_tasks
                .values()
                .any(|child| {
                    source.task_id.as_ref() == Some(&child.task_id)
                        && same_participant_scope(&child.child, source)
                        && child.parent_segment_generation == target.segment_generation
                        && document.segment_generation() == target.segment_generation
                        && document.metadata.session_id == target.session_id
                        && target.turn_id == Some(child.parent_turn_id)
                        && document.active_turn().is_some_and(|turn| {
                            !turn.closed && turn.turn_id == child.parent_turn_id
                        })
                });
            if !admitted {
                return Err(DomainError::CollaborationUnauthorized {
                    action: "message_main_only_direct_child",
                    identity: source.session_id.clone(),
                });
            }
            Ok(())
        }
        CollaborationRunControl::Running | CollaborationRunControl::NaturallyIdle => Ok(()),
    }
}

/// Loads a trusted persisted participant through its exact indexed root and verifies its generation.
/// 通过精确索引根加载可信持久参与者，并验证其代际。
/// Participant data must come from a durable collaboration record, never tool-supplied JSON.
/// 参与者数据必须来自持久协作记录，不能来自工具提交的 JSON。
pub(super) async fn load_participant_handle(
    invocation: &ValidatedInvocation,
    identity: &CollaborationParticipant,
) -> Result<Arc<SessionHandle>, String> {
    let session_id = Uuid::parse_str(&identity.session_id).map_err(|error| error.to_string())?;
    let root_session_id =
        Uuid::parse_str(&identity.root_session_id).map_err(|error| error.to_string())?;
    let root = crate::session_goal_lifecycle::session_v1_handle_from_state(
        &invocation.state,
        root_session_id,
    )
    .await?;
    let root_locator = root.storage_locator().map_err(|error| error.to_string())?;
    let handle = if session_id == root_session_id {
        root
    } else {
        let locator = locate_embedded_task_session_v1(&root_locator, &identity.session_id)
            .map_err(|error| error.to_string())?;
        let child = load_embedded_canonical_handle(&invocation.registry, locator, session_id)?;
        let usage = root.lifetime_usage_snapshot()?;
        let parent = usage
            .children_by_session_id
            .get(&identity.session_id)
            .map(|child| child.parent_session_id.as_str())
            .ok_or_else(|| String::from("persisted participant has no verified parent locator"))?;
        let parent_id = Uuid::parse_str(parent).map_err(|error| error.to_string())?;
        child.bind_child_usage(&root, parent_id, now_ms()?);
        child
    };
    let actual = participant_for_handle(invocation, session_id, &handle, None).await?;
    if !same_participant_scope(identity, &actual) {
        return Err(String::from(
            "persisted participant does not match its canonical handle",
        ));
    }
    Ok(handle)
}

/// Confirms absent storage after a failed participant load, without interpreting error strings.
/// 参与者加载失败后确认存储缺失，不解释错误字符串。
/// `identity` supplies the exact root/child path; false preserves unreadable or mismatched data.
/// `identity` 提供精确根或子路径；返回假会保留不可读或身份不匹配的数据。
pub(super) async fn participant_storage_missing(
    invocation: &ValidatedInvocation,
    identity: &CollaborationParticipant,
) -> Result<bool, String> {
    // A registered root owns its locator even if its document cannot currently be read.
    // 即使当前文档无法读取，已注册根仍拥有其定位器。
    let root_id = Uuid::parse_str(&identity.root_session_id).map_err(|error| error.to_string())?;
    // Only a typed absent registration permits consulting the authoritative index.
    // 只有强类型的未注册结果允许查询权威索引。
    let root_locator = match invocation.registry.get(root_id) {
        Ok(root) => root.storage_locator().map_err(|error| error.to_string())?,
        Err(SessionRuntimeError::SessionNotRegistered(_)) => {
            match vulcan_workspace_store::workspace_store::locate_optional_session_v1_storage(
                &invocation.controller_runtime,
                &identity.root_session_id,
            )
            .await
            .map_err(|error| error.to_string())?
            {
                Some(locator) => locator,
                None => return Ok(true),
            }
        }
        Err(error) => return Err(error.to_string()),
    };
    // Embedded tasks have a deterministic locator, never a guessed sibling search.
    // 嵌入任务使用确定性定位器，绝不猜测或搜索同级路径。
    let locator = if identity.session_id == identity.root_session_id {
        root_locator
    } else {
        locate_embedded_task_session_v1(&root_locator, &identity.session_id)
            .map_err(|error| error.to_string())?
    };
    locator
        .metadata_exists()
        .map(|exists| !exists)
        .map_err(|error| error.to_string())
}

/// Derives the participant from a canonical handle and its exact persisted root locator.
/// 从规范句柄及其精确持久化根定位器派生参与者。
/// The optional turn is supplied by a validated invocation or the target's current turn.
/// 可选回合来自已验证调用或目标当前回合。
pub(super) async fn participant_for_handle(
    invocation: &ValidatedInvocation,
    session_id: Uuid,
    handle: &Arc<SessionHandle>,
    turn_id: Option<u64>,
) -> Result<CollaborationParticipant, String> {
    // The lifetime owner supplies an exact locator hint, never collaboration authorization.
    // 生命周期归属只提供精确定位提示，不提供协作授权。
    let usage = handle.lifetime_usage_snapshot()?;
    let root_session_id = Uuid::parse_str(&usage.root_session_id)
        .map_err(|error| format!("invalid persisted root identity: {error}"))?;
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    let generation = handle
        .read(|document| document.segment_generation())
        .map_err(|error| error.to_string())?;
    let root_locator =
        locate_optional_session_v1_storage(&invocation.controller_runtime, &usage.root_session_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| String::from("participant root has no indexed locator"))?;
    let task_id = if session_id == root_session_id {
        if locator != root_locator {
            return Err(String::from(
                "root participant locator does not match its index",
            ));
        }
        None
    } else {
        let expected_locator =
            locate_embedded_task_session_v1(&root_locator, &session_id.to_string())
                .map_err(|error| error.to_string())?;
        if locator != expected_locator {
            return Err(String::from(
                "embedded participant locator does not match its root",
            ));
        }
        Some(SubagentTaskId::try_new(session_id.to_string()).map_err(|error| error.to_string())?)
    };
    participant(
        session_id,
        root_session_id,
        locator.project_id(),
        generation,
        turn_id,
        task_id,
    )
}

/// Resolves the exact direct parent and verifies its durable child reservation.
/// 解析精确直接父会话，并核对父会话的持久子任务预留。
/// Returns the authorized parent session ID or an explicit missing or mismatched ownership error.
/// 返回已验证父会话 ID；归属缺失或不一致时返回明确错误。
pub(super) async fn verified_direct_parent(
    invocation: &ValidatedInvocation,
) -> Result<Uuid, String> {
    let usage = invocation.source_handle.lifetime_usage_snapshot()?;
    let source_id = invocation.source_session_id.to_string();
    let parent_id = usage
        .children_by_session_id
        .get(&source_id)
        .ok_or_else(|| String::from("ask_parent requires an embedded child session"))?
        .parent_session_id
        .as_str();
    let parent_session_id = Uuid::parse_str(parent_id).map_err(|error| error.to_string())?;
    let root_session_id =
        Uuid::parse_str(&usage.root_session_id).map_err(|error| error.to_string())?;
    let root = crate::session_goal_lifecycle::session_v1_handle_from_state(
        &invocation.state,
        root_session_id,
    )
    .await?;
    let root_locator = root.storage_locator().map_err(|error| error.to_string())?;
    let parent = if parent_session_id == root_session_id {
        root
    } else {
        let locator = locate_embedded_task_session_v1(&root_locator, parent_id)
            .map_err(|error| error.to_string())?;
        load_embedded_canonical_handle(&invocation.registry, locator, parent_session_id)?
    };
    let task_id = SubagentTaskId::try_new(source_id.clone()).map_err(|error| error.to_string())?;
    let current = participant_for_handle(
        invocation,
        invocation.source_session_id,
        &invocation.source_handle,
        None,
    )
    .await?;
    // Ownership may span repeated executions, but must match the complete current child scope.
    // 归属可以跨越重复执行，但必须匹配当前子参与者的完整范围。
    let owns_child = parent
        .read(|document| {
            document
                .collaboration_state()
                .child_tasks
                .values()
                .any(|record| {
                    record.task_id == task_id && same_participant_scope(&record.child, &current)
                })
        })
        .map_err(|error| error.to_string())?;
    if !owns_child {
        return Err(String::from(
            "direct parent reservation does not match the current child identity",
        ));
    }
    Ok(parent_session_id)
}

/// Rejects stopped recipients, allowing MainOnly exclusively for their verified direct child.
/// 拒绝已停止接收者；MainOnly 仅允许来自已验证直接子会话的消息。
/// Returns success for an admissible recipient or an explicit stopped-target diagnostic.
/// 接收者可准入时返回成功，否则返回明确的目标停止诊断。
pub(super) async fn require_message_admission(
    invocation: &ValidatedInvocation,
    target_session_id: Uuid,
    target: &ResolvedSession,
) -> Result<(), String> {
    // Stop policy comes from the durable control state, never from a transient UI status.
    // 停止策略来自持久控制状态，不依赖短暂界面状态。
    let control = target
        .handle
        .read(|document| document.collaboration_run_control())
        .map_err(|error| error.to_string())?;
    match control {
        CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents => Err(
            String::from("The target agent has stopped and rejects new messages."),
        ),
        CollaborationRunControl::MainOnly => {
            if verified_direct_parent(invocation).await? != target_session_id {
                return Err(String::from(
                    "The stopped main session accepts messages only from its owned child.",
                ));
            }
            Ok(())
        }
        CollaborationRunControl::Running | CollaborationRunControl::NaturallyIdle => Ok(()),
    }
}
