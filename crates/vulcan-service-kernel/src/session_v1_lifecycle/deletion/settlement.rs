//! Preserve target settlement evidence until the source durably acknowledges it.
//! 保留目标结算证据，直到来源可靠确认。

use super::*;
use std::collections::BTreeSet;
use vulcan_session_core::v1::collaboration::{
    CollaborationInboundDeliveryRecord, CollaborationParticipant,
};

/// Parses the exact session and root coordinates of `participant`, attributing failure to `target`.
/// 解析 `participant` 的精确会话与根坐标，将失败归因于 `target`。
/// Returns the two immutable UUIDs without guessing an alternate participant or storage root.
/// 返回两个不可变 UUID，不猜测替代参与者或存储根。
fn coordinates(
    target: SessionId,
    participant: &CollaborationParticipant,
) -> Result<(SessionId, SessionId), SessionV1LifecycleError> {
    Ok((
        participant
            .session_id
            .parse()
            .map_err(|error: uuid::Error| invalid(target, error.to_string()))?,
        participant
            .root_session_id
            .parse()
            .map_err(|error: uuid::Error| invalid(target, error.to_string()))?,
    ))
}

/// Captures source/root gate identities needed by all retained inbound deliveries in `tree`.
/// 捕获 `tree` 全部保留入站投递所需的来源及根锁身份。
/// `registry` supplies warm authority; cold owners use their exact metadata. Caller rechecks after locking.
/// `registry` 提供驻留权威；冷态所有者使用精确元数据。调用方在取锁后重查。
pub(super) fn peer_ids(
    registry: &SessionRuntimeRegistry,
    tree: &BTreeMap<SessionId, SessionV1StorageLocator>,
) -> Result<BTreeSet<SessionId>, SessionV1LifecycleError> {
    // Lock every retained source, including deliveries that can settle while admission is waiting.
    // 锁定每个保留来源，包括准入等待期间可能结算的投递。
    let mut peers = BTreeSet::new();
    for (id, locator) in tree {
        // Warm and cold are explicit registry states, not competing guesses about physical ownership.
        // 驻留与冷态是明确注册表状态，不是对物理归属的竞争猜测。
        let state = match registry.get(*id) {
            Ok(handle) => {
                if handle.storage_locator()? != *locator {
                    return Err(invalid(
                        *id,
                        "Settlement target has a replaced canonical locator.",
                    ));
                }
                handle.read(|document| document.collaboration_state().clone())?
            }
            Err(SessionRuntimeError::SessionNotRegistered(_)) => {
                locator.read_collaboration_recovery_state()?
            }
            Err(error) => return Err(error.into()),
        };
        for inbound in state.inbound_deliveries.values() {
            // Root ownership must remain stable even when the source is embedded rather than indexed.
            // 即使来源为嵌入会话而非索引会话，根归属仍必须稳定。
            let (source, root) = coordinates(*id, &inbound.source)?;
            peers.extend([source, root]);
        }
    }
    Ok(peers)
}

/// Verifies terminal inbound receipts on `handles` against source acknowledgement under all peer gates.
/// 在持有全部对端锁时，对照来源确认校验 `handles` 上的入站终态回执。
/// `controller` owns SQLite locator existence; `registry` supplies canonical source documents.
/// `controller` 持有 SQLite 定位存在性权威；`registry` 提供规范来源文档。
/// Returns an unsettled error instead of deleting the only evidence needed for source recovery.
/// 返回未结错误，而不是删除来源恢复所需的唯一证据。
pub(super) async fn require_source_acknowledgements(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    handles: &[Arc<SessionHandle>],
) -> Result<(), SessionV1LifecycleError> {
    for target in handles {
        // Clone receipt evidence outside the document lock before resolving an independent source.
        // 在文档锁外复制回执证据，再解析独立来源。
        let (inbound, operations, revision) = target.read_full_durable(|document| {
            (
                document
                    .collaboration_state()
                    .inbound_deliveries
                    .values()
                    .filter(|delivery| delivery.settlement.is_some())
                    .cloned()
                    .collect::<Vec<_>>(),
                document.collaboration_state().operations.clone(),
                document.durable_revision(),
            )
        })?;
        for delivery in inbound {
            // Selection above guarantees the presence of the target's terminal settlement.
            // 上面的筛选保证目标终态结算存在。
            let settlement = delivery
                .settlement
                .as_ref()
                .expect("selected terminal settlement");
            // A stable event proves the target receipt; later archive revisions do not replace that event.
            // 稳定事件证明目标回执；后续归档修订不能替换该事件。
            let event_seq = operations
                .get(&settlement.target_operation_id)
                .ok_or_else(|| {
                    invalid(
                        target.session_id(),
                        "Target settlement has no operation receipt.",
                    )
                })?
                .event_seq;
            // Both source identities already participate in the caller's global lifecycle lock order.
            // 两个来源身份均已参与调用方的全局生命周期锁顺序。
            let (source_id, root_id) = coordinates(target.session_id(), &delivery.source)?;
            // An absent indexed root is unavailable authority, never permission to search or recreate its files.
            // 缺失索引根意味着权威不可寻址，绝不授权搜索或重建其文件。
            let Some(root) =
                locate_optional_session_v1_storage(controller, &root_id.to_string()).await?
            else {
                // A successfully deleted source has no remaining acknowledgement obligation to this target.
                // 已成功删除的来源不再对本目标保留确认义务。
                continue;
            };
            if root.project_id() != delivery.source.project_id {
                return Err(invalid(
                    target.session_id(),
                    "Settlement source root project changed.",
                ));
            }
            // The participant explicitly declares indexed or embedded ownership.
            // 参与者明确声明索引或嵌入归属。
            let locator = if source_id == root_id {
                root
            } else {
                locate_embedded_task_session_v1(&root, &source_id.to_string())?
            };
            // Cold peers are inspected without publishing an incompletely initialized runtime owner.
            // 冷态对端仅被检查，不发布运行时初始化不完整的所有者。
            match registry.get(source_id) {
                Ok(source) => {
                    if source.storage_locator()? != locator {
                        return Err(invalid(
                            target.session_id(),
                            "Settlement source canonical locator changed.",
                        ));
                    }
                    source.read_full_durable(|document| {
                        verify_acknowledgement(
                            target.session_id(),
                            document,
                            &delivery,
                            event_seq,
                            revision,
                        )
                    })??;
                }
                Err(SessionRuntimeError::SessionNotRegistered(_)) => {
                    // Full disk history is sufficient for this read; normal runtime loading keeps its own initialization.
                    // 完整磁盘历史足以完成本次读取；正常运行时加载保留自身初始化。
                    let loaded = load_session_v1_full(&locator)?;
                    if loaded.metadata.session_id != source_id.to_string() {
                        return Err(invalid(
                            target.session_id(),
                            "Settlement source metadata identity changed.",
                        ));
                    }
                    verify_acknowledgement(
                        target.session_id(),
                        &loaded.document,
                        &delivery,
                        event_seq,
                        revision,
                    )?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

/// Validates `inbound` against the pinned durable source `document` and target event/revision.
/// 对照固定的可靠来源 `document` 及目标事件和修订校验 `inbound`。
/// Returns an explicit pending/mismatch error, or permits a newer source generation already protected by Core editing.
/// 返回明确待定或不一致错误，或允许已经受 Core 编辑保护的新来源代次。
fn verify_acknowledgement(
    target: SessionId,
    document: &SessionDocument,
    inbound: &CollaborationInboundDeliveryRecord,
    target_event_seq: Seq,
    target_revision: u64,
) -> Result<(), SessionV1LifecycleError> {
    if document.segment_generation() > inbound.source.segment_generation {
        // Core rejects source generation replacement until questions and acknowledgements are settled.
        // Core 在问题和确认结算前拒绝替换来源代次。
        return Ok(());
    }
    if document.segment_generation() != inbound.source.segment_generation {
        return Err(invalid(
            target,
            "Settlement source generation precedes its accepted participant.",
        ));
    }
    // Same-generation evidence must retain the exact accepted question and both participants.
    // 同代次证据必须保留精确接纳问题及双方参与者。
    let question = document
        .question_snapshot(&inbound.question_id)
        .ok_or_else(|| {
            invalid(
                target,
                "Settlement source question is missing in the same generation.",
            )
        })?;
    // The outbound record carries the reliable source-side acknowledgement.
    // 出站记录携带来源侧可靠确认。
    let outbound = document
        .delivery_snapshot(&inbound.delivery_id)
        .ok_or_else(|| {
            invalid(
                target,
                "Settlement source delivery is missing in the same generation.",
            )
        })?;
    if question.source != inbound.source
        || question.target != inbound.target
        || question.delivery_id != inbound.delivery_id
        || outbound.question_id != inbound.question_id
        || outbound.source != inbound.source
        || outbound.target != inbound.target
    {
        return Err(invalid(
            target,
            "Settlement peer identity conflicts with its accepted delivery.",
        ));
    }
    // A local target terminal fact does not prove the source has observed that fact.
    // 目标本地终态事实不能证明来源已经观察到该事实。
    let Some(ack) = outbound.settlement_ack.as_ref() else {
        return Err(SessionV1LifecycleError::UnsettledCollaboration(target));
    };
    // This function is only called for terminal inbound records selected above.
    // 本函数仅对上面筛选的终态入站记录调用。
    let settlement = inbound
        .settlement
        .as_ref()
        .expect("selected terminal settlement");
    if ack.state != settlement.state
        || ack.source_event_seq != settlement.source_event_seq
        || ack.target_event_seq != target_event_seq
        || ack.target_revision < inbound.target_revision
        || ack.target_revision > target_revision
    {
        return Err(invalid(
            target,
            "Settlement source acknowledgement conflicts with the target receipt.",
        ));
    }
    Ok(())
}
