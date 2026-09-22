//! Exact embedded-tree discovery from durable parent reservations.
//! 根据可靠父任务预留发现精确嵌入树。

use super::*;
pub(super) mod creation;
pub(in crate::agent_collaboration) mod resumption;
use std::collections::BTreeMap;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskRecord, CollaborationChildTaskState,
};
use vulcan_workspace_store::workspace_store::SessionV1CollaborationRecoverySnapshot;
use vulcan_workspace_store::workspace_store::{SessionV1StorageErrorCode, SessionV1StoreError};

/// Distinguishes an absent publication from corrupt or unreadable metadata without parsing diagnostics.
/// 不解析诊断文案，区分尚未发布与损坏或不可读的元数据。
/// `locator` is exact storage authority; returns None only for the typed missing-session error.
/// `locator` 是精确存储权威；仅对类型化会话缺失错误返回 None。
fn snapshot_if_present(
    locator: &SessionV1StorageLocator,
) -> Result<Option<SessionV1CollaborationRecoverySnapshot>, String> {
    match locator.read_collaboration_recovery_snapshot() {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(SessionV1StoreError::Storage(error))
            if error.code == SessionV1StorageErrorCode::SessionMissing =>
        {
            Ok(None)
        }
        Err(error) => Err(error.to_string()),
    }
}

/// One metadata-selected session, with its exact discovery edge retained for canonical revalidation.
/// 一份元数据筛选会话，保留精确发现边用于规范重校验。
pub(super) struct RecoveryNode {
    /// Exact physical locator derived from the indexed root.
    /// 从索引根派生的精确物理定位器。
    pub locator: SessionV1StorageLocator,
    /// Generation and collaboration read in one metadata publication.
    /// 从一次元数据发布读取的代次和协作状态。
    pub snapshot: SessionV1CollaborationRecoverySnapshot,
    /// Direct parent's preceding node index and every matching execution reservation.
    /// 直接父节点的前序索引以及全部匹配执行预留。
    pub parent: Option<(usize, Vec<CollaborationChildTaskRecord>)>,
}

/// Discoverable participants and independent unreadable-child diagnostics from one tree scan.
/// 一次树扫描中可发现的参与者及独立不可读子节点诊断。
pub(super) struct RecoveryTree {
    /// Parent-first nodes whose metadata was successfully validated.
    /// 元数据成功校验的父先子后节点。
    pub nodes: Vec<RecoveryNode>,
    /// Exact failed child identities; these errors do not suppress unrelated delivery repair.
    /// 精确失败子身份；这些错误不抑制无关投递修复。
    pub failures: Vec<String>,
}

/// Reads one metadata selection under its lifecycle gate; no canonical document is hydrated.
/// 在生命周期锁内读取一份元数据筛选快照；不水合规范文档。
/// `registry` serializes replacement of `locator`; returns its snapshot or None before publication.
/// `registry` 串行化 `locator` 的替换；返回快照，尚未发布时返回 None。
async fn read_snapshot(
    registry: &Arc<SessionRuntimeRegistry>,
    locator: &SessionV1StorageLocator,
) -> Result<Option<SessionV1CollaborationRecoverySnapshot>, String> {
    // The UUID comes from the exact locator, not a directory scan or tool payload.
    // UUID 来自精确定位器，而非目录扫描或工具载荷。
    let session_id = Uuid::parse_str(locator.session_id()).map_err(|error| error.to_string())?;
    // Keep the exclusion inside blocking IO even if its async caller is cancelled.
    // 即使异步调用方取消，仍在阻塞 IO 内保留互斥。
    let lifecycle = registry
        .acquire_lifecycle_transition(session_id)
        .await
        .map_err(|error| error.to_string())?;
    // Own the opaque locator for the blocking read.
    // 为阻塞读取持有不透明定位器。
    let locator = locator.clone();
    crate::spawn_service_blocking_task(move || {
        let _lifecycle = lifecycle;
        snapshot_if_present(&locator)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Discovers current-generation descendants without selecting a latest execution or reading histories.
/// 发现当前代次后代，不选择最新执行，也不读取历史正文。
/// `root` is indexed authority; returns parents before children, or explicit corrupt-tree evidence.
/// `root` 是索引权威；按父先子后返回节点，或返回明确损坏树证据。
pub(super) async fn discover(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    root: SessionV1StorageLocator,
) -> Result<RecoveryTree, String> {
    // Begin with indexed-root metadata; each discovered edge must be revalidated before recovery writes.
    // 从索引根元数据开始；每条发现边在恢复写入之前都必须重新校验。
    let snapshot = read_snapshot(registry, &root)
        .await?
        .ok_or("Indexed recovery root publication is missing.")?;
    // Breadth-first traversal is iterative so a deep valid tree does not consume the call stack.
    // 广度优先遍历采用迭代形式，避免深层合法树消耗调用栈。
    let mut nodes = vec![RecoveryNode {
        locator: root.clone(),
        snapshot,
        parent: None,
    }];
    // A physical child generation has one direct parent; conflicting edges are never guessed away.
    // 一个物理子代次只有一个直接父节点；不能猜测并忽略冲突边。
    let mut owners = BTreeSet::from([root.session_id().to_owned()]);
    // Preserve unreadable child diagnostics while continuing siblings and the root's own outbox.
    // 保留不可读子节点诊断，同时继续兄弟节点和根自身的出站箱。
    let mut failures = Vec::new();
    // The cursor advances only after collecting that parent's complete set of physical children.
    // 只有收集完该父节点的完整物理子集合后才推进游标。
    let mut parent_index = 0;
    while parent_index < nodes.len() {
        // Group reusable sessions without ordering their immutable execution identities.
        // 按可复用会话分组，不对不可变执行身份排序以选择执行。
        let mut children: BTreeMap<String, Vec<CollaborationChildTaskRecord>> = BTreeMap::new();
        for record in nodes[parent_index]
            .snapshot
            .collaboration
            .child_tasks
            .values()
        {
            if record.parent_segment_generation != nodes[parent_index].snapshot.segment_generation {
                if record.state == CollaborationChildTaskState::Terminal {
                    continue;
                }
                return Err(format!(
                    "live child {} belongs to a replaced parent generation",
                    record.task_id.as_str()
                ));
            }
            if record.child.root_session_id != root.session_id()
                || record.child.project_id != root.project_id()
                || record.task_id.as_str() != record.child.session_id
            {
                return Err(format!(
                    "child {} has a conflicting recovery scope",
                    record.task_id.as_str()
                ));
            }
            children
                .entry(record.child.session_id.clone())
                .or_default()
                .push(record.clone());
        }
        for (child_id, records) in children {
            // Every nesting depth uses the indexed root's TASK partition.
            // 每个嵌套深度都使用索引根的 TASK 分区。
            let locator = locate_embedded_task_session_v1(&root, &child_id)
                .map_err(|error| error.to_string())?;
            // Read generation and records atomically from metadata, never through separate file reads.
            // 从元数据原子读取代次与记录，不通过两次文件读取拼接。
            let selected = match read_snapshot(registry, &locator).await {
                Ok(None) => {
                    Box::pin(creation::recover_missing(
                        state,
                        registry,
                        &nodes,
                        parent_index,
                        &locator,
                        &records,
                    ))
                    .await
                }
                result => result,
            };
            let snapshot = match selected {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => continue,
                Err(error) => {
                    failures.push(format!("embedded session {child_id}: {error}"));
                    continue;
                }
            };
            // Keep all executions in the current physical generation; none is declared the current run here.
            // 保留当前物理代次中的全部执行；此处不宣布其中任何一个为当前运行。
            let mut matching = Vec::new();
            for record in records {
                if record.child.segment_generation == snapshot.segment_generation {
                    matching.push(record);
                } else if record.state != CollaborationChildTaskState::Terminal {
                    return Err(format!(
                        "live child {child_id} belongs to a replaced child generation"
                    ));
                }
            }
            if matching.is_empty() {
                continue;
            }
            if !owners.insert(child_id.clone()) {
                return Err(format!(
                    "child {child_id} has cyclic or conflicting parent ownership"
                ));
            }
            nodes.push(RecoveryNode {
                locator,
                snapshot,
                parent: Some((parent_index, matching)),
            });
        }
        parent_index += 1;
    }
    Ok(RecoveryTree { nodes, failures })
}

/// Loads only the selected node's ancestry and revalidates durable edges before child hydration.
/// 只加载选中节点的祖先链，并在水合子文档前重新校验可靠边。
/// `nodes` supplies discovery hints; `selected` names the work owner; returns its canonical handle.
/// `nodes` 提供发现线索；`selected` 指定待办所有者；返回其规范句柄。
pub(super) async fn load_selected(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    nodes: &[RecoveryNode],
    selected: usize,
) -> Result<Arc<SessionHandle>, String> {
    // Ancestry indices were built in parent-first order, so this walk cannot select a newer run.
    // 祖先索引按父先子后构造，因此该遍历不会选择另一更新运行。
    let mut ancestry = vec![selected];
    while let Some((parent, _)) = &nodes[*ancestry.last().expect("selected node exists")].parent {
        ancestry.push(*parent);
    }
    ancestry.reverse();
    // Indexed ownership is resolved only for the actual root.
    // 只为实际根节点解析索引所有权。
    let root_id =
        Uuid::parse_str(nodes[0].locator.session_id()).map_err(|error| error.to_string())?;
    // The root remains the established usage owner without changing usage accounting algorithms.
    // 根保持既有用量所有者身份，不改变用量计算算法。
    let root = crate::session_goal_lifecycle::session_v1_handle_from_state(state, root_id).await?;
    if root.storage_locator().map_err(|error| error.to_string())? != nodes[0].locator
        || root
            .read(|document| document.segment_generation())
            .map_err(|error| error.to_string())?
            != nodes[0].snapshot.segment_generation
    {
        return Err(String::from(
            "indexed recovery root changed after discovery",
        ));
    }
    // Advance through the direct parent chain, not every earlier unrelated node.
    // 沿直接父链推进，不加载此前全部无关节点。
    let mut parent_handle = root.clone();
    for index in ancestry.into_iter().skip(1) {
        // Capture one exact edge for the blocking load's ownership checks.
        // 为阻塞加载的归属检查捕获一条精确边。
        let node = &nodes[index];
        // Embedded nodes always have a preceding parent by the discovery constructor.
        // 发现构造器保证嵌入节点始终具有前序父节点。
        let (parent_index, records) = node
            .parent
            .as_ref()
            .ok_or("embedded recovery edge is absent")?;
        // Freeze expected generations instead of querying a later active execution.
        // 冻结预期代次，不查询稍后的活动执行。
        let parent_generation = nodes[*parent_index].snapshot.segment_generation;
        let child_generation = node.snapshot.segment_generation;
        let child_id =
            Uuid::parse_str(node.locator.session_id()).map_err(|error| error.to_string())?;
        let locator = node.locator.clone();
        let records = records.clone();
        // The root, parent and child share the global UUID lock order used by message publication.
        // 根、父和子采用消息发布所用的全局 UUID 锁顺序。
        let identities = BTreeSet::from([root_id, parent_handle.session_id(), child_id]);
        let mut lifecycles = Vec::new();
        for identity in identities {
            lifecycles.push(
                registry
                    .acquire_lifecycle_transition(identity)
                    .await
                    .map_err(|error| error.to_string())?,
            );
        }
        // Move guards and canonical owners together into the blocking writer.
        // 将守卫与规范所有者一起移入阻塞写入器。
        let registry = registry.clone();
        let source_state = state.clone();
        let root_generation = nodes[0].snapshot.segment_generation;
        let parent = parent_handle.clone();
        let root = root.clone();
        parent_handle = crate::spawn_service_blocking_task(move || {
            let _lifecycles = lifecycles;
            for handle in [&root, &parent] {
                if !Arc::ptr_eq(
                    &registry
                        .get(handle.session_id())
                        .map_err(|error| error.to_string())?,
                    handle,
                ) {
                    return Err(String::from(
                        "recovery ancestor was replaced after discovery",
                    ));
                }
            }
            parent
                .read(|document| {
                    document.segment_generation() == parent_generation
                        && document.durable_revision() == document.memory_revision()
                        && records.iter().all(|record| {
                            document
                                .collaboration_state()
                                .child_tasks
                                .get(&record.execution_id)
                                .is_some_and(|current| {
                                    current.parent_segment_generation
                                        == record.parent_segment_generation
                                        && current.reservation_operation_id
                                            == record.reservation_operation_id
                                        && current.source_tool_name == record.source_tool_name
                                        && current.parent_turn_id == record.parent_turn_id
                                        && current.child == record.child
                                        && current.task_id == record.task_id
                                })
                        })
                })
                .map_err(|error| error.to_string())?
                .then_some(())
                .ok_or("recovery parent reservation changed or is unpublished")?;
            if locator
                .read_collaboration_recovery_snapshot()
                .map_err(|error| error.to_string())?
                .segment_generation
                != child_generation
            {
                return Err(String::from(
                    "embedded recovery generation changed after discovery",
                ));
            }
            // Only now may interruption recovery update the exact canonical child document.
            // 只有此时中断恢复才可更新精确规范子文档。
            // Active startup already owns publication; cold resumption must precede generic interruption projection.
            // 活动启动已经拥有发布；冷续跑必须先于通用中断投影。
            let runtime = source_state
                .lock()
                .map_err(|error| error.to_string())?
                .agent_runtime
                .clone();
            let running = match runtime {
                Some(runtime) => runtime.task_runtime.is_task_running(child_id)?,
                None => false,
            };
            let child = if running {
                load_embedded_canonical_handle(&registry, locator.clone(), child_id)?
            } else {
                resumption::load(
                    &registry,
                    &root,
                    root_generation,
                    &parent,
                    locator.clone(),
                    &records,
                )?
            };
            if child.storage_locator().map_err(|error| error.to_string())? != locator
                || child
                    .read(|document| document.segment_generation())
                    .map_err(|error| error.to_string())?
                    != child_generation
            {
                return Err(String::from(
                    "embedded recovery canonical owner does not match discovery",
                ));
            }
            child.bind_child_usage(&root, parent.session_id(), now_ms()?);
            Ok::<_, String>(child)
        })
        .await
        .map_err(|error| error.to_string())??;
    }
    Ok(parent_handle)
}
