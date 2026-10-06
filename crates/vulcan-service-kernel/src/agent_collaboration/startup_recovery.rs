//! Indexed startup repair without hydrating unrelated conversation histories.
//! 基于索引的启动修复，不水合无关会话历史。

use super::*;
pub(super) mod native_execution;
pub(in crate::agent_collaboration) mod native_results;
pub(super) mod tree;
pub(super) mod worker;
use std::collections::BTreeSet;
use vulcan_session_core::v1::collaboration::{
    CollaborationNotificationState, CollaborationState, CollaborationTaskMessageEffectKind,
};
use vulcan_workspace_store::workspace_store::locate_session_v1_storage;

/// Selects durable delivery and queued-task work; questions do not become business completion duties.
/// 筛选可靠投递和排队任务待办；问题不会变成业务完成义务。
/// `collaboration` is metadata only; returns whether canonical loading is necessary for repair.
/// `collaboration` 仅是元数据；返回是否需要为修复加载规范文档。
fn needs_delivery_recovery(collaboration: &CollaborationState) -> bool {
    // A claim or failed application can survive a crash before its result was recorded.
    // 认领或应用前失败可能在结果记录前遭遇崩溃后存续。
    if collaboration
        .task_message_effects
        .iter()
        .any(|(delivery, effect)| {
            matches!(
                effect.outcome,
                CollaborationTaskMessageEffectKind::Claimed { .. }
                    | CollaborationTaskMessageEffectKind::Failed { .. }
            ) && !collaboration.task_results.contains_key(delivery)
        })
    {
        return true;
    }
    collaboration
        .outbound_task_messages
        .values()
        .any(|record| record.acceptance.is_none())
        || collaboration.task_results.keys().any(|delivery| {
            !collaboration
                .task_result_acknowledgements
                .contains_key(delivery)
        })
        || collaboration
            .inbound_task_messages
            .keys()
            .any(|delivery| !collaboration.task_message_effects.contains_key(delivery))
        || collaboration.notifications.values().any(|notification| {
            matches!(
                notification.state,
                CollaborationNotificationState::Recorded | CollaborationNotificationState::Accepted
            )
        })
        || collaboration.outbound_deliveries.values().any(|delivery| {
            // Failed delivery has no target settlement to replay after the source is cancelled.
            // 失败投递在来源撤销后没有需要重放的目标结算。
            matches!(
                delivery.state,
                CollaborationDeliveryState::Recorded
                    | CollaborationDeliveryState::Accepted
                    | CollaborationDeliveryState::Projected
            ) || (delivery.state != CollaborationDeliveryState::Failed
                && delivery.settlement_ack.is_none()
                && collaboration
                    .questions
                    .get(&delivery.question_id)
                    .is_some_and(|question| question.state != CollaborationQuestionState::Pending))
        })
}

/// Repairs `session_ids` independently using exact indexed locators and published state.
/// 使用精确索引定位器和已发布状态，独立修复 `session_ids`。
/// `app` emits launches and `state` supplies service ownership; returns collected per-session failures.
/// `app` 发出启动，`state` 提供服务所有权；返回汇总的逐会话失败。
#[cfg(test)]
pub(super) async fn recover_indexed_collaboration(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_ids: BTreeSet<Uuid>,
) -> Result<(), String> {
    let registry = state
        .lock()
        .map_err(|error| error.to_string())?
        .session_runtime_registry
        .clone();
    recover_indexed_collaboration_owned(app, state, session_ids, &registry).await
}

/// Repairs one indexed pass only for the captured canonical registry.
/// 仅为捕获的规范注册表修复一轮索引扫描。
/// `registry` is worker ownership, and every selected node rechecks replacement and shutdown before repair.
/// `registry` 是任务所有权；每个选中节点在修复前重新检查替换及关闭状态。
async fn recover_indexed_collaboration_owned(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_ids: BTreeSet<Uuid>,
    registry: &Arc<SessionRuntimeRegistry>,
) -> Result<(), String> {
    // Startup publication precedes this call; retain only immutable service owners across awaits.
    // 启动发布先于此调用；跨等待只保留不可变服务所有者。
    let controller = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        if !Arc::ptr_eq(&guard.session_runtime_registry, registry) {
            return Err(String::from(
                "Collaboration recovery registry was replaced.",
            ));
        }
        guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("collaboration recovery requires controller runtime"))?
    };
    // Do not abort remaining independent sessions when one journal or locator is unavailable.
    // 一个日志或定位器不可用时，不终止其余独立会话。
    let mut failures = Vec::new();
    for session_id in session_ids {
        worker::require_active_owner(state, registry)?;
        let attempt = async {
            let locator = locate_session_v1_storage(&controller, &session_id.to_string())
                .await
                .map_err(|error| error.to_string())?;
            // Native descendants have no SQLite rows; discover them through exact durable reservations.
            // 原生后代没有 SQLite 行；通过精确可靠预留发现它们。
            let discovery = tree::discover(state, registry, locator).await?;
            failures.extend(discovery.failures);
            let nodes = discovery.nodes;
            for (index, node) in nodes.iter().enumerate() {
                worker::require_active_owner(state, registry)?;
                // Native unfinished reservations require source inspection even without message outbox work.
                // 即使没有消息出站待办，原生未结算预留也要求检查来源。
                let native_pending = node.parent.as_ref().is_some_and(|(_, records)| records.iter()
                    .any(|record| record.state != vulcan_session_core::v1::collaboration::CollaborationChildTaskState::Terminal));
                if !needs_delivery_recovery(&node.snapshot.collaboration) && !native_pending {
                    continue;
                }
                // One damaged participant must not prevent independent discovered peers from repair.
                // 一个损坏参与者不能阻止其他已发现独立参与者修复。
                let session_id = Uuid::parse_str(node.locator.session_id())
                    .map_err(|error| error.to_string())?;
                let repair = async {
                    // Full canonical loading revalidates event coverage and the current generation.
                    // 完整规范加载重新校验事件覆盖及当前代际。
                    let handle = tree::load_selected(state, registry, &nodes, index).await?;
                    worker::require_active_owner(state, registry)?;
                    if let Some((parent_index, records)) = &node.parent {
                        // Loading the selected ancestry already established this exact canonical parent.
                        // 加载选中祖先链已经建立此精确规范父节点。
                        let parent_id = Uuid::parse_str(nodes[*parent_index].locator.session_id())
                            .map_err(|error| error.to_string())?;
                        let parent = registry.get(parent_id).map_err(|error| error.to_string())?;
                        // The indexed root owns tree-wide stop authority across every descendant level.
                        // 索引根拥有贯穿全部后代层级的整树停止权威。
                        let root_id = Uuid::parse_str(nodes[0].locator.session_id())
                            .map_err(|error| error.to_string())?;
                        // Loading the ancestry established this exact canonical storage owner.
                        // 加载祖先链已建立此精确规范存储所有者。
                        let root = registry.get(root_id).map_err(|error| error.to_string())?;
                        if let Err(error) = native_results::recover_terminal_returns(
                            state, registry, root, nodes[0].snapshot.segment_generation,
                            parent.clone(), handle.clone(), records.clone(),
                        ).await {
                            failures.push(format!("session {session_id} native results: {error}"));
                        }
                        // Keep the native preparation state off the discovery caller's stack.
                        // 将原生准备状态移出发现调用链的栈空间。
                        if let Err(error) = Box::pin(native_execution::recover(
                            app, state, registry, parent, handle.clone(), records.clone(),
                        )).await {
                            failures.push(format!("session {session_id} native execution: {error}"));
                        }
                    }
                    let invocation = recovery_invocation(
                        state.clone(),
                        controller.clone(),
                        registry.clone(),
                        session_id,
                        handle.clone(),
                    )?;
                    if let Err(error) =
                        reconcile_canonical_collaboration_outbox(app, &invocation).await
                    {
                        failures.push(format!("session {session_id} outbox: {error}"));
                    }
                    if let Err(error) = reconcile_task_results_for_session(state, session_id).await
                    {
                        failures.push(format!("session {session_id} results: {error}"));
                    }
                    // Previously acknowledged starts may have crashed before the advisory wakeup.
                    // 已确认的启动可能在提示性唤醒之前崩溃。
                    if node.parent.is_none()
                        && let Err(error) = task_launch::wake_session_tasks_if_idle(
                            app,
                            state,
                            &controller,
                            session_id,
                        )
                        .await
                    {
                        failures.push(format!("session {session_id} queued tasks: {error}"));
                    }
                    let questions = handle
                        .read(|document| {
                            document
                                .collaboration_state()
                                .questions
                                .values()
                                .filter(|question| {
                                    question.state == CollaborationQuestionState::Pending
                                })
                                .map(|question| question.question_id.clone())
                                .collect::<Vec<_>>()
                        })
                        .map_err(|error| error.to_string())?;
                    for question_id in questions {
                        if let Err(error) =
                            launch::wake_accepted_question(app, &invocation, &question_id).await
                        {
                            failures.push(format!(
                                "session {session_id} question {}: {error}",
                                question_id.as_str()
                            ));
                        }
                    }
                    Ok::<(), String>(())
                }
                .await;
                if let Err(error) = repair {
                    failures.push(format!("session {session_id}: {error}"));
                }
            }
            // An open historical turn is not execution consent. Only the source-specific
            // collaboration work above may wake execution; ordinary business waits for user action.
            // 历史开放回合不构成执行许可。只有上述具有明确来源的协作待办可以唤醒执行；
            // 普通业务必须等待用户操作。
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = attempt {
            failures.push(format!("session {session_id}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Collaboration startup recovery has {} unresolved item(s): {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

/// Schedules serialized startup repair with retained failure retries for active and archived sessions.
/// 为活动及归档会话调度串行启动修复，并保留失败重试。
/// `state` owns the published workspace and `app` reports diagnostics; never blocks readiness.
/// `state` 拥有已发布工作区，`app` 报告诊断；不阻塞就绪发布。
pub(crate) fn start_collaboration_recovery(
    state: SharedServiceBridgeState,
    app: crate::ServiceEventEmitter,
) {
    if let Err(error) = worker::schedule(&state, app) {
        tracing::warn!(%error, "Collaboration recovery worker could not start");
        if let Err(diagnostic) = crate::push_webui_service_diagnostic(
            &state,
            format!("代理协作恢复任务启动失败：{error}"),
        ) {
            tracing::warn!(%diagnostic, "Collaboration recovery diagnostic publication failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_session_core::v1::collaboration::{
        CollaborationDeliveryId, CollaborationInboundNotificationRecord,
        CollaborationNotificationId, CollaborationNotificationRecord,
        CollaborationNotificationState, CollaborationOperationId, CollaborationParticipant,
    };

    /// A source-only recorded notification must make the indexed recovery pass load its owner.
    /// 只有来源侧已记录的通知也必须让索引恢复扫描加载其所有者。
    /// The source outbox replay is unreachable unless this session-level gate selects the record.
    /// 若会话级闸门不选中该记录，来源出站重放就永远不可达。
    #[test]
    fn recorded_or_accepted_outbound_notification_triggers_startup_recovery() {
        let participant = |session: &str| CollaborationParticipant {
            session_id: session.to_owned(),
            root_session_id: session.to_owned(),
            project_id: String::from("project-1"),
            segment_generation: 1,
            turn_id: None,
            task_id: None,
        };
        let notification_id = CollaborationNotificationId::try_new("notification-1").unwrap();
        let mut collaboration = CollaborationState::default();
        collaboration.notifications.insert(
            notification_id.clone(),
            CollaborationNotificationRecord {
                notification_id,
                delivery_id: CollaborationDeliveryId::try_new("delivery-1").unwrap(),
                operation_id: CollaborationOperationId::try_new("operation-source").unwrap(),
                source: participant("session-source"),
                target: participant("session-target"),
                body: String::from("child reached validation"),
                state: CollaborationNotificationState::Recorded,
                failure: None,
                target_revision: None,
                target_event_seq: None,
                created_at_ms: 10,
                updated_at_ms: 10,
            },
        );
        assert!(
            needs_delivery_recovery(&collaboration),
            "the indexed gate must select a source whose notification still needs delivery"
        );
        let notification = collaboration
            .notifications
            .values_mut()
            .next()
            .expect("the notification remains in the source outbox");
        notification.state = CollaborationNotificationState::Accepted;
        notification.target_revision = Some(3);
        assert!(
            needs_delivery_recovery(&collaboration),
            "the indexed gate must also select a source awaiting target projection progress"
        );
    }

    /// A stuck inbound notification alone must be visible to the session-level recovery gate.
    /// 单独滞留的入站通知必须能被会话级恢复闸门看见。
    ///
    /// # Notes
    /// 说明
    /// `project_collaboration_before_api` selects `Accepted` inbound notifications that were never
    /// projected, so that state is reachable and is meant to be projected. This test pins whether the
    /// startup gate that decides to load a session at all also notices it. It deliberately asserts the
    /// **current** behaviour so a later change is a conscious decision rather than a silent one.
    /// `project_collaboration_before_api` 会选取已接纳但从未投影的入站通知，因此该状态可达且本应被投影。
    /// 本测试固定"决定是否加载会话"的启动闸门是否也注意到它，并刻意断言**当前**行为，
    /// 使日后改变成为有意识的决定而非静默漂移。
    #[test]
    fn accepted_but_unprojected_inbound_notification_is_not_a_session_level_recovery_trigger() {
        let participant = |session: &str| CollaborationParticipant {
            session_id: session.to_owned(),
            root_session_id: session.to_owned(),
            project_id: String::from("project-1"),
            segment_generation: 1,
            turn_id: None,
            task_id: None,
        };
        let mut collaboration = CollaborationState::default();
        // The record is accepted by the target but carries no projection proof.
        // 该记录已被目标接纳，但不带任何投影证明。
        collaboration.inbound_notifications.insert(
            CollaborationNotificationId::try_new("notification-1").unwrap(),
            CollaborationInboundNotificationRecord {
                notification_id: CollaborationNotificationId::try_new("notification-1").unwrap(),
                delivery_id: CollaborationDeliveryId::try_new("delivery-1").unwrap(),
                source_operation_id: CollaborationOperationId::try_new("operation-source").unwrap(),
                operation_id: CollaborationOperationId::try_new("operation-accept").unwrap(),
                source: participant("session-source"),
                target: participant("session-target"),
                body: String::from("child reached validation"),
                state: CollaborationNotificationState::Accepted,
                failure: None,
                target_revision: 3,
                projected_seq: None,
                projected_input_id: None,
                projected_turn_id: None,
                accepted_at_ms: 10,
                updated_at_ms: 11,
            },
        );
        assert!(
            !needs_delivery_recovery(&collaboration),
            "current behaviour: an accepted-but-unprojected notification does not by itself make the \
             startup gate load this session; if this changes, update S5 §三 which records it"
        );
    }
}
