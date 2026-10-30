//! Regression coverage for user insertions while collaboration keeps a business Turn open.
//! 协作保持业务回合开放时用户插队的回归覆盖。

use super::*;
use vulcan_session_core::v1::{
    ActivateChildTaskInput, CollaborationExecutionId, CollaborationOperationId,
    CollaborationParticipant, CollaborationRunControl, PendingInputId, QueuedInputIdentity,
    ReserveChildTaskInput, SetRunControlInput, SubagentReturnInput, TurnId, UserInputSource,
};

/// Distinct inputs prove ordered batch delivery without depending on production wording.
/// 独立测试输入用于证明有序批次投递，不依赖生产文案。
const INSERTION_TEXTS: [&str; 2] = ["insertion-alpha", "insertion-beta"];

/// Produces one validated child return through the real sink; no Provider transport is started.
/// 通过真实接收器产生一条已校验子任务回流，不启动供应商传输。
/// Returns the exact durable result accepted by the parent's public return API.
/// 返回父会话公开回流接口接纳的精确持久结果。
async fn completed_child_return() -> SubagentReturnInput {
    // The child owns independent canonical history and its own sink.
    // 子任务持有独立规范历史及接收器。
    let child = published_open_turn();
    // Physical lifecycle facts are persisted without network I/O.
    // 无网络操作地持久化物理生命周期事实。
    let mut sink = ProviderV1StreamSink::new(
        child.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request(&child, &mut sink);
    sink.append_assistant_delta("child-result-marker", 5)
        .unwrap();
    sink.accept_terminal(terminal_facts(0)).await.unwrap();
    // Exact durable receipts select the child's final text and Turn.
    // 精确持久回执选择子任务最终正文及回合。
    let durability = sink.take_finished_durability();
    child
        .read(|document| {
            document.build_subagent_return_input(SubagentReturnBuildRequest {
                final_turn_id: durability.api_terminal.as_ref().unwrap().value.turn_id,
                child_task_id: SubagentTaskId::try_new("insertion-child-return")?,
                source_assistant_text_seq: durability
                    .completed_assistant_text
                    .as_ref()
                    .unwrap()
                    .seq,
            })
        })
        .unwrap()
        .unwrap()
}

/// Enqueues an ordinary follow-up and two insertions on `handle`, returning their original IDs.
/// 在 `handle` 排入一个普通后续输入及两条插队，返回插队原始身份。
fn admit_insertion_batch(handle: &SessionHandle) -> Vec<PendingInputId> {
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                document.apply_atomic_transition(|document| {
                    document.enqueue_input(
                        ClientInputSubmission::new(
                            ClientActionId::try_new("ordinary-followup")?,
                            "ordinary-followup-marker",
                            Vec::new(),
                        ),
                        6,
                    )?;
                    // Keep each accepted identity for later event and dispatch assertions.
                    // 保留每个接纳身份，供后续事件及分发断言使用。
                    let mut identities = Vec::new();
                    for (position, text) in INSERTION_TEXTS.into_iter().enumerate() {
                        // Queue promotion mirrors the UI's explicit insertion operation.
                        // 队列提升对应界面明确的插队操作。
                        let action = ClientActionId::try_new(format!("insert-{position}"))?;
                        // The queue receipt supplies the authoritative identity.
                        // 队列回执提供权威身份。
                        let input_id = document
                            .enqueue_input(
                                ClientInputSubmission::new(action.clone(), text, Vec::new()),
                                7,
                            )?
                            .into_value()
                            .input_id;
                        document.promote_queued_input_to_pending_turn(
                            QueuedInputIdentity {
                                client_action_id: action,
                                input_id,
                            },
                            8,
                        )?;
                        identities.push(input_id);
                    }
                    Ok(identities)
                })
            },
        )
        .unwrap()
        .value
}

/// Checks exact same-Turn ownership, ordered Provider delivery, and cold reload for `identities`.
/// 检查 `identities` 的精确同回合归属、有序供应商投递及冷加载。
/// `turn_id` is the original business owner; success returns no value.
/// `turn_id` 是原业务所有者；成功时无返回值。
fn assert_insertions_ready(handle: &SessionHandle, turn_id: TurnId, identities: &[PendingInputId]) {
    // Read the actual persisted document so memory-only projection cannot conceal a missing owner.
    // 读取实际持久化文档，避免仅内存投影掩盖所有者缺失。
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(
        &handle.storage_locator().unwrap(),
    )
    .unwrap()
    .document;
    assert_eq!(loaded.active_turn().unwrap().turn_id, turn_id);
    assert!(
        loaded
            .metadata
            .input_state
            .pending_in_turn_inputs
            .is_empty()
    );
    assert_eq!(loaded.metadata.input_state.queued_inputs.len(), 1);
    // Filter by stable identities, not event positions that unrelated facts can shift.
    // 按稳定身份筛选，不使用可能被无关事实移动的事件位置。
    let actual_inputs = loaded
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::UserInput(input) if identities.contains(&input.input_id) => {
                assert_eq!(event.turn_id, Some(turn_id));
                assert_eq!(input.input_source, UserInputSource::PendingInTurn);
                Some(input.input_id)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_inputs, identities);
    // The next physical request reads this production projection.
    // 下一物理请求读取此生产投影。
    let projection = handle.capture_controller_projection_snapshot().unwrap();
    // Each insertion must appear once, in acceptance order, while the ordinary queue stays out.
    // 各插队必须按接纳顺序仅出现一次，普通队列保持在外。
    let projected_inputs = projection
        .items
        .iter()
        .filter_map(|item| match item {
            ProjectedItem::UserInput { text, .. } if INSERTION_TEXTS.contains(&text.as_str()) => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(projected_inputs, INSERTION_TEXTS);
    assert!(!projection.items.iter().any(|item|
        matches!(item, ProjectedItem::UserInput { text, .. } if text == "ordinary-followup-marker")));
}

/// Child-return continuation consumes every insertion before its next API and schedules FIFO afterward.
/// 子任务回流续跑在下一请求前消费全部插队，之后才调度普通先进先出队列。
#[tokio::test]
async fn child_return_continuation_preserves_insertion_batch() {
    // Real isolated storage covers both publication and strict restart reconstruction.
    // 真实隔离存储覆盖发布及严格重启重建。
    let (_directory, parent) = published_stored_open_turn();
    // The original Turn must survive until child-result processing is complete.
    // 子结果处理完成前，原回合必须保持。
    let original_turn = parent
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    // Start the parent API before user steering and child completion arrive.
    // 在用户指引与子任务完成到达前启动父请求。
    let mut sink = ProviderV1StreamSink::new(
        parent.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request(&parent, &mut sink);
    // Retain exact insertion identities across continuation and final settlement.
    // 在续跑及最终结算之间保留精确插队身份。
    let identities = admit_insertion_batch(&parent);
    // The parent receives the real child's durable content while its API is active.
    // 父请求活动期间接收真实子任务的持久正文。
    let returned = parent
        .append_subagent_return(completed_child_return().await, 9)
        .unwrap()
        .value;
    sink.append_assistant_delta("parent-response-marker", 9)
        .unwrap();
    sink.accept_terminal(terminal_facts(0)).await.unwrap();
    // Physical completion must publish inserted input authority without closing the logical Turn.
    // 物理完成必须发布插队输入权威，同时不关闭逻辑回合。
    let durability = sink.take_finished_durability();
    assert!(durability.turn_completion.is_none());
    // The sink carries the actual post-terminal promotion receipt to the stream dispatcher.
    // 接收器把实际终态后提升回执携带至流分发器。
    let terminal = durability.api_terminal.unwrap();
    assert_eq!(terminal.value.promoted_input_seqs.len(), identities.len());
    crate::webui_prompt_lifecycle::pending_in_turn_dispatch_authority_from_v1_handle(
        &parent,
        &terminal.value,
    )
    .unwrap();
    crate::webui_prompt_lifecycle::subagent_return_dispatch_authority_from_v1_handle(
        &parent, &returned,
    )
    .unwrap();
    assert_insertions_ready(&parent, original_turn, &identities);
    // A repeated settlement cannot append the insertion batch twice.
    // 重复结算不能把插队批次追加两次。
    let retry = parent.complete_successful_turn(11).await.unwrap();
    assert!(!retry.changed);
    assert!(matches!(
        retry.value,
        SuccessfulTurnSettlement::SubagentReturn
    ));
    assert_insertions_ready(&parent, original_turn, &identities);
    // Admit the actual next business API over the same canonical history.
    // 在同一规范历史上准入实际下一业务请求。
    let mut continuation = ProviderV1StreamSink::new(
        parent.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    // Request time changes without changing the route.
    // 只改变请求时间，不改变路由。
    let mut next_facts = request_facts();
    next_facts.prepared_at_ms = 12;
    prepare_sink_request_with_facts(&parent, &mut continuation, next_facts.clone());
    assert!(
        parent
            .read(|document| document.subagent_return_dispatch_started(&returned))
            .unwrap()
    );
    assert!(
        parent
            .read(|document| document.business_input_dispatch_started(
                original_turn,
                *terminal.value.promoted_input_seqs.last().unwrap()
            ))
            .unwrap()
    );
    assert_insertions_ready(&parent, original_turn, &identities);
    continuation
        .append_assistant_delta("processed-batch-marker", 13)
        .unwrap();
    // Final completion now releases only the still-queued ordinary input.
    // 此时最终完成只释放仍在排队的普通输入。
    let mut completed = terminal_facts(0);
    completed.request_facts = next_facts;
    completed.completed_at_ms = 14;
    completed.timing.completed_at_ms = Some(14);
    continuation.accept_terminal(completed).await.unwrap();
    // The old insertion batch must not become another fresh Turn after being consumed.
    // 已消费的旧插队批次不能再变成另一个新回合。
    let successor = continuation
        .take_finished_durability()
        .turn_completion
        .unwrap()
        .value
        .successor;
    let TurnSuccessor::QueuedUser {
        client_action_id, ..
    } = successor
    else {
        panic!("the ordinary queue must follow the completed continuation");
    };
    assert_eq!(client_action_id.as_str(), "ordinary-followup");
}

/// Pending child work preserves insertion continuation instead of waiting blindly for its result.
/// 待完成子任务保留插队续跑，而不是忽略指引等待子任务结果。
#[tokio::test]
async fn running_child_keeps_insertion_batch_in_original_turn() {
    // Use actual storage and stable parent ownership.
    // 使用实际存储及稳定父级归属。
    let (_directory, parent) = published_stored_open_turn();
    // Read the declared project from the authoritative storage locator.
    // 从权威存储定位器读取声明的项目。
    let project = parent.storage_locator().unwrap().project_id().to_owned();
    // The active document supplies the exact parent Turn.
    // 活动文档提供精确父回合。
    let turn = parent
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.apply_atomic_transition(|document| {
                    // The exact reserved child remains active through parent API completion.
                    // 精确预留子任务在父请求完成期间保持活动。
                    let task = SubagentTaskId::try_new("running-insertion-child")?;
                    // The reservation and activation share the original execution identity.
                    // 预留与激活共用原始执行身份。
                    let execution =
                        CollaborationExecutionId::try_new("running-insertion-execution")?;
                    document.reserve_child_task(ReserveChildTaskInput {
                        operation_id: CollaborationOperationId::try_new(
                            "running-insertion-reserve",
                        )?,
                        source_tool_name: "spawn_agent".to_owned(),
                        execution_id: execution.clone(),
                        task_id: task.clone(),
                        parent_turn_id: turn,
                        child: CollaborationParticipant::new(
                            "running-child-session",
                            parent.session_id().to_string(),
                            project.as_str(),
                            1,
                            None,
                            Some(task.clone()),
                        )?,
                        reserved_at_ms: 3,
                    })?;
                    document.activate_child_task(ActivateChildTaskInput {
                        operation_id: CollaborationOperationId::try_new(
                            "running-insertion-activate",
                        )?,
                        execution_id: execution,
                        task_id: task,
                        active_at_ms: 4,
                    })
                })
            },
        )
        .unwrap();
    // The parent can answer without making its owned child disappear.
    // 父级可以返回正文，但所属子任务仍存在。
    let mut sink = ProviderV1StreamSink::new(
        parent.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request(&parent, &mut sink);
    // User steering must be admitted even while the child is still running.
    // 子任务仍在运行时也必须接纳用户指引。
    let identities = admit_insertion_batch(&parent);
    sink.append_assistant_delta("waiting-child-marker", 9)
        .unwrap();
    sink.accept_terminal(terminal_facts(0)).await.unwrap();
    // The same-Turn receipt authorizes a request without falsely declaring business completion.
    // 同回合回执授权请求，同时不错误声明业务完成。
    let durability = sink.take_finished_durability();
    assert!(durability.turn_completion.is_none());
    crate::webui_prompt_lifecycle::pending_in_turn_dispatch_authority_from_v1_handle(
        &parent,
        &durability.api_terminal.unwrap().value,
    )
    .unwrap();
    assert_insertions_ready(&parent, turn, &identities);
    assert_eq!(
        parent
            .read(|document| document
                .completion_obligations(turn)
                .open_child_task_ids
                .len())
            .unwrap(),
        1
    );
}

/// Explicit Stop prevents child-return settlement from promoting or dispatching waiting insertions.
/// 明确停止阻止子任务回流结算提升或分发等待中的插队。
#[tokio::test]
async fn stopped_child_return_does_not_promote_insertion_batch() {
    // Stop is tested against the same real stored parent and child-return path.
    // 在相同真实持久父级及子任务回流路径上测试停止。
    let (_directory, parent) = published_stored_open_turn();
    // The final physical API can race with an explicit stop state.
    // 最后一次物理请求可能与明确停止状态交错。
    let mut sink = ProviderV1StreamSink::new(
        parent.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request(&parent, &mut sink);
    admit_insertion_batch(&parent);
    parent
        .append_subagent_return(completed_child_return().await, 9)
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("stop-insertion-test")?,
                    state: CollaborationRunControl::Stopped,
                    changed_at_ms: 9,
                })
            },
        )
        .unwrap();
    sink.append_assistant_delta("stopped-parent-marker", 9)
        .unwrap();
    sink.accept_terminal(terminal_facts(0)).await.unwrap();
    // No continuation receipt can bypass explicit run control.
    // 任何续跑回执都不能绕过明确运行控制。
    let durability = sink.take_finished_durability();
    assert!(durability.turn_completion.is_none());
    assert!(
        durability
            .api_terminal
            .unwrap()
            .value
            .promoted_input_seqs
            .is_empty()
    );
    assert_eq!(
        parent
            .read(|document| document.metadata.input_state.pending_in_turn_inputs.len())
            .unwrap(),
        INSERTION_TEXTS.len()
    );
    assert!(!parent.complete_successful_turn(11).await.unwrap().changed);
}
