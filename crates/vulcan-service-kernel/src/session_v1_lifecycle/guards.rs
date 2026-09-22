//! Preserve peer settlement receipts across history edits and physical deletion.
//! 在历史编辑与物理删除期间保留对端结算回执。

use super::*;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_session_core::v1::collaboration::{
    CollaborationInboundDeliveryRecord, CollaborationParticipant,
};
use vulcan_session_core::v1::document::EditGenerationInput;
use vulcan_workspace_store::workspace_store::SessionV1StorageLocator;

use crate::session_runtime_registry_v1::{
    SessionEditGenerationReceipt, SessionLifecycleTransitionGuard,
};

/// Parses the exact session and root coordinates of `participant`, attributing failure to `owner`.
/// 解析 `participant` 的精确会话与根坐标，将失败归因于 `owner`。
/// Returns the two immutable UUIDs without guessing an alternate participant or storage root.
/// 返回两个不可变 UUID，不猜测替代参与者或存储根。
pub(super) fn coordinates(
    owner: SessionId,
    participant: &CollaborationParticipant,
) -> Result<(SessionId, SessionId), SessionV1LifecycleError> {
    Ok((
        participant
            .session_id
            .parse()
            .map_err(|error: uuid::Error| invalid(owner, error.to_string()))?,
        participant
            .root_session_id
            .parse()
            .map_err(|error: uuid::Error| invalid(owner, error.to_string()))?,
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
        for (source, root) in peer_coordinates(registry, *id, locator)? {
            peers.extend([source, root]);
        }
    }
    Ok(peers)
}

/// Captures the exact source and source-root identities of every inbound delivery of one owner.
/// 捕获单个所有者每条入站投递的精确来源与来源根身份。
/// Warm and cold are explicit registry states, not competing guesses about physical ownership.
/// 驻留与冷态是明确注册表状态，不是对物理归属的竞争猜测。
fn peer_coordinates(
    registry: &SessionRuntimeRegistry,
    id: SessionId,
    locator: &SessionV1StorageLocator,
) -> Result<Vec<(SessionId, SessionId)>, SessionV1LifecycleError> {
    let state = match registry.get(id) {
        Ok(handle) => {
            if handle.storage_locator()? != *locator {
                return Err(invalid(
                    id,
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
    let mut peers = Vec::with_capacity(state.inbound_deliveries.len());
    for inbound in state.inbound_deliveries.values() {
        // Root ownership must remain stable even when the source is embedded rather than indexed.
        // 即使来源为嵌入会话而非索引会话，根归属仍必须稳定。
        peers.push(coordinates(id, &inbound.source)?);
    }
    Ok(peers)
}

/// Acquires exactly `identities` in one global order and releases them with the returned set.
/// 按单一全局顺序精确取得 `identities`，并随返回集合释放。
async fn acquire_lifecycle_guards(
    registry: &SessionRuntimeRegistry,
    identities: &BTreeSet<SessionId>,
) -> Result<Vec<SessionLifecycleTransitionGuard>, SessionV1LifecycleError> {
    let mut guards = Vec::with_capacity(identities.len());
    for id in identities {
        guards.push(registry.acquire_lifecycle_transition(*id).await?);
    }
    Ok(guards)
}

/// Returns the exact identities whose lifecycle gate a collaboration write must hold for `tree`.
/// 返回针对 `tree` 的协作写入必须持有的精确生命周期门身份。
/// The written target is always a member: production publication acquires the gate of every session
/// it mutates, so omitting the target would admit a fresh unacknowledged receipt after the check.
/// 被写入的目标始终是成员：生产发布取得其修改的每个会话的门，遗漏目标会在校验之后放行新的
/// 未确认回执。
fn write_gate_ids(
    registry: &SessionRuntimeRegistry,
    tree: &BTreeMap<SessionId, SessionV1StorageLocator>,
) -> Result<BTreeSet<SessionId>, SessionV1LifecycleError> {
    let mut identities = tree.keys().copied().collect::<BTreeSet<_>>();
    identities.extend(peer_ids(registry, tree)?);
    Ok(identities)
}

/// Verifies unsettled collaboration on every member of `tree`, returning the warm handles.
/// 校验 `tree` 每个成员上的未结协作，并返回驻留句柄。
///
/// # Parameters
/// 参数
/// `registry` is the unique runtime registry; `tree` is the physically declared edit membership.
/// `registry` 是唯一运行时注册表；`tree` 是物理声明的编辑成员集合。
///
/// # Returns
/// 返回值
/// Returns the registered members' handles in stable session-id order. Warm and cold are explicit
/// registry states, exactly as peer resolution treats them: an unregistered member is read through its
/// locator's recovery state instead of being skipped, so a cold child cannot hide an unsettled receipt.
/// 按稳定的会话标识顺序返回已注册成员的句柄。驻留与冷态是明确的注册表状态，与对端解析的处理一致：
/// 未注册成员改为通过其定位器的恢复状态读取而不是被跳过，因此冷态子会话无法隐藏未结回执。
fn settled_tree_owner_handles(
    registry: &SessionRuntimeRegistry,
    tree: &BTreeMap<SessionId, SessionV1StorageLocator>,
) -> Result<Vec<Arc<SessionHandle>>, SessionV1LifecycleError> {
    let mut handles = Vec::with_capacity(tree.len());
    for (id, locator) in tree {
        match registry.get(*id) {
            Ok(handle) => {
                if handle.storage_locator()? != *locator {
                    return Err(SessionV1LifecycleError::CanonicalOwnerUnavailable {
                        session_id: *id,
                    });
                }
                if handle.read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)? {
                    return Err(SessionV1LifecycleError::UnsettledCollaboration(*id));
                }
                handles.push(handle);
            }
            Err(SessionRuntimeError::SessionNotRegistered(_)) => {
                // The locator is the only remaining authority for a cold member, and the unsettled
                // predicate has a single definition shared with the warm document.
                // 定位器是冷态成员仅存的权威，且未结判定与驻留文档共享唯一定义。
                if locator
                    .read_collaboration_recovery_state()?
                    .requires_edit_or_delete_settlement()
                {
                    return Err(SessionV1LifecycleError::UnsettledCollaboration(*id));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(handles)
}

/// Verifies terminal inbound receipts on `handles` against source acknowledgement under all peer gates.
/// 在持有全部对端锁时，对照来源确认校验 `handles` 上的入站终态回执。
/// `controller` owns SQLite locator existence; `registry` supplies canonical source documents.
/// `controller` 持有 SQLite 定位存在性权威；`registry` 提供规范来源文档。
/// Returns an unsettled error instead of discarding the only evidence needed for source recovery.
/// 返回未结错误，而不是丢弃来源恢复所需的唯一证据。
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
            require_source_acknowledgement(
                controller,
                registry,
                target,
                &operations,
                revision,
                &delivery,
            )
            .await?;
        }
    }
    Ok(())
}

/// Verifies one terminal inbound `delivery` of `target` against its durable source acknowledgement.
/// 对照可靠来源确认校验 `target` 的一条入站终态 `delivery`。
/// `operations` and `revision` are the target's pinned durable facts captured outside its document lock.
/// `operations` 与 `revision` 是在目标文档锁外捕获的精确可靠事实。
async fn require_source_acknowledgement(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    target: &Arc<SessionHandle>,
    operations: &std::collections::BTreeMap<
        vulcan_session_core::v1::collaboration::CollaborationOperationId,
        vulcan_session_core::v1::collaboration::CollaborationOperationRecord,
    >,
    revision: u64,
    delivery: &CollaborationInboundDeliveryRecord,
) -> Result<(), SessionV1LifecycleError> {
    // Selection by the caller guarantees the presence of the target's terminal settlement.
    // 调用方的筛选保证目标终态结算存在。
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
    let Some(root) = locate_optional_session_v1_storage(controller, &root_id.to_string()).await?
    else {
        // A successfully deleted source has no remaining acknowledgement obligation to this target.
        // 已成功删除的来源不再对本目标保留确认义务。
        return Ok(());
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
                verify_acknowledgement(target.session_id(), document, delivery, event_seq, revision)
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
                delivery,
                event_seq,
                revision,
            )?;
        }
        Err(error) => return Err(error.into()),
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

/// Holds the complete cross-session admission required before one indexed root history edit.
/// 在单次索引根历史编辑之前持有完整的跨会话准入。
///
/// The guard keeps every source and source-root gate of the target's retained inbound deliveries
/// until it is dropped, so no peer can publish a new settlement while the edit is being published.
/// 该守卫在释放前一直持有目标全部保留入站投递的来源及来源根门，使任何对端都无法在编辑发布
/// 期间提交新的结算。
pub(crate) struct EditGenerationAdmission<'a> {
    /// Canonical indexed root whose history is being replaced.
    /// 正在替换历史的规范索引根。
    _target: &'a SessionHandle,
    /// Ordered peer gates retained until the edit publication completes.
    /// 保留至编辑发布完成的有序对端门。
    _peers: Vec<SessionLifecycleTransitionGuard>,
}

impl std::fmt::Debug for EditGenerationAdmission<'_> {
    /// Reports only the retained protected root identity, never an internal lock value.
    /// 仅报告保留的受保护根身份，绝不报告内部锁值。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditGenerationAdmission")
            .field("session_id", &self._target.session_id())
            .field("peer_gate_count", &self._peers.len())
            .finish()
    }
}

/// Takes the exact cross-session admission for one indexed root history edit.
/// 为一次索引根历史编辑取得精确的跨会话准入。
///
/// # Parameters
/// 参数
/// `controller` owns SQLite locator existence, `registry` owns canonical process handles, and
/// `target` is the exact claimed owner of the indexed root being edited.
/// `controller` 持有 SQLite 定位存在性，`registry` 持有规范进程句柄，`target` 是正在编辑的
/// 索引根的精确声明所有者。
///
/// # Returns
/// 返回值
/// Returns only after every settlement peer gate is held, the target is still the canonical owner,
/// and no retained receipt lacks its durable source acknowledgement.
/// 仅在持有全部结算对端门、目标仍是规范所有者，且没有任何保留回执缺少可靠来源确认后返回。
pub(crate) async fn admit_edit_generation<'a>(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    target: &'a Arc<SessionHandle>,
) -> Result<EditGenerationAdmission<'a>, SessionV1LifecycleError> {
    let session_id = target.session_id();
    // SQLite remains the only accepted directory authority, exactly as physical deletion requires.
    // 与物理删除一致，SQLite 仍是唯一被接受的目录权威。
    let root = locate_session_v1_storage(controller, &session_id.to_string()).await?;
    // An embedded child is located through its parent, so only the indexed root owns edit admission.
    // 嵌入子会话经由父会话定位，因此只有索引根持有编辑准入。
    if root.session_id() != session_id.to_string() || target.storage_locator()? != root {
        return Err(SessionV1LifecycleError::CanonicalOwnerUnavailable { session_id });
    }
    let tree = deletion_members(&root, session_id)?;
    // The target and every delivery peer share one ordered identity set, so a concurrent target
    // write cannot publish a new receipt between this admission and the edit publication.
    // 目标与全部投递对端共享同一有序身份集合，使并发目标写入无法在本次准入与编辑发布之间
    // 提交新回执。
    let gate_ids = write_gate_ids(registry, &tree)?;
    let guards = acquire_lifecycle_guards(registry, &gate_ids).await?;
    // A delivery admitted while the gates were awaited would escape the retained lock set.
    // 在等待门期间获准的投递会逃出保留的锁集合。
    let current_gate_ids = write_gate_ids(registry, &tree)?;
    if current_gate_ids != gate_ids {
        return Err(invalid(
            session_id,
            "Settlement write membership changed before edit admission; retry the edit.",
        ));
    }
    // Registrations and physical ownership may have changed while the peer gates were awaited.
    // 对端门等待期间，注册归属或物理所有权可能已经变化。
    let current = match registry.get(session_id) {
        Ok(handle) => handle,
        Err(SessionRuntimeError::SessionNotRegistered(_)) => {
            return Err(SessionV1LifecycleError::CanonicalOwnerUnavailable { session_id });
        }
        Err(error) => return Err(error.into()),
    };
    if !Arc::ptr_eq(&current, target) || current.storage_locator()? != target.storage_locator()? {
        return Err(SessionV1LifecycleError::CanonicalOwnerUnavailable { session_id });
    }
    // Every physically owned descendant is edited with the root, so each member must be settled.
    // 每个物理归属的后代都与根一起被编辑，因此每个成员都必须已结清。
    // Checking only the root would discard an embedded child's only recovery entry: the edit resets
    // `child_tasks`, and startup recovery discovers embedded children through that table.
    // 只检查根会丢掉嵌入子会话唯一的恢复入口：编辑会重置 `child_tasks`，而启动恢复正是通过该表
    // 发现嵌入子会话。
    let tree_handles = settled_tree_owner_handles(registry, &tree)?;
    // Retained terminal receipts must carry a durable source acknowledgement, unchanged from deletion.
    // 保留的终态回执必须携带可靠来源确认，与删除一致。
    require_source_acknowledgements(controller, registry, &tree_handles).await?;
    Ok(EditGenerationAdmission {
        _target: target,
        _peers: guards,
    })
}

/// Publishes one indexed root history edit while its cross-session admission remains held.
/// 在跨会话准入保持期间发布一次索引根历史编辑。
///
/// # Parameters
/// 参数
/// `controller` and `registry` are the exact lifecycle authorities used by deletion, `target` is the
/// claimed canonical root owner, and `expected_segment_generation` plus `input` are the unchanged
/// Core edit request.
/// `controller` 与 `registry` 是删除使用的精确生命周期权威，`target` 是声明的规范根所有者，
/// `expected_segment_generation` 与 `input` 是未作改动的 Core 编辑请求。
///
/// # Returns
/// 返回值
/// Returns the exact durable edit receipt; the write gates are released before this function returns,
/// so later workbench refresh and provider admission start without any lifecycle lock held.
/// 返回精确的可靠编辑回执；写入门在本函数返回前释放，后续工作台刷新与供应商准入不持有任何
/// 生命周期锁。
pub(crate) async fn publish_guarded_edit_generation(
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    target: &Arc<SessionHandle>,
    expected_segment_generation: u64,
    input: EditGenerationInput,
) -> Result<SessionEditGenerationReceipt, SessionV1LifecycleError> {
    // The admission is retained until the synchronous publication below has fully returned.
    // 准入保持到下方同步发布完整返回。
    let _admission = admit_edit_generation(controller, registry, target).await?;
    Ok(target.publish_edit_generation(expected_segment_generation, input)?)
}

/// Retains one lifecycle transition gate for deterministic concurrency tests.
/// 为确定性并发测试保留一个生命周期转换门。
/// The gate is the same identity that `admit_edit_generation` must acquire for its edit target.
/// 该门与 `admit_edit_generation` 必须为其编辑目标取得的身份相同。
#[cfg(test)]
pub(crate) struct HeldLifecycleGate {
    /// Owned gate retained until this guard is dropped.
    /// 保留至本守卫释放的已持有门。
    _gate: SessionLifecycleTransitionGuard,
}

/// Acquires and retains one lifecycle transition gate for the exact session identity.
/// 为精确会话身份取得并保留一个生命周期转换门。
#[cfg(test)]
pub(crate) async fn hold_lifecycle_gate(
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<HeldLifecycleGate, SessionV1LifecycleError> {
    Ok(HeldLifecycleGate {
        _gate: registry.acquire_lifecycle_transition(session_id).await?,
    })
}
