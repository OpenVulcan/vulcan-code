//! Body-free VMM lifecycle audit derivation and runtime-log persistence.
//! 不含正文的 VMM 生命周期审计推导与运行时日志持久化。

use std::collections::{BTreeMap, BTreeSet};

use vulcan_context::{RuntimeLogEvent, RuntimeVmmLifecycleAction, RuntimeVmmLifecycleEvent};
use vulcan_vmm::{
    VmmMemoryLease, VmmPromptAnchorKind, VmmSessionMemoryState, render_vmm_context_item_model,
};
use vulcan_workbench_model::SessionId;

/// Semantic reason assigned to leases removed while deriving one state transition.
/// 推导单次状态转换时赋予已移除租约的语义原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VmmRemovedLeaseAuditAction {
    /// Terminal TTL consumption crossed the lease expiry boundary, or compaction reclaimed it later.
    /// 终态 TTL 消费跨越租约到期边界，或压缩随后将其回收。
    Expired,
    /// Explicit VMM disablement revoked the lease.
    /// 显式禁用 VMM 撤销了租约。
    Revoked,
}

impl VmmRemovedLeaseAuditAction {
    /// Returns the runtime lifecycle action represented by this removal reason.
    /// 返回该移除原因表示的运行时生命周期动作。
    ///
    /// # Returns
    /// 返回值
    /// The exact serialized runtime action.
    /// 精确的序列化运行时动作。
    fn runtime_action(self) -> RuntimeVmmLifecycleAction {
        match self {
            Self::Expired => RuntimeVmmLifecycleAction::LeaseExpired,
            Self::Revoked => RuntimeVmmLifecycleAction::LeaseRevoked,
        }
    }
}

/// Body-free aggregate collected for one VMM lifecycle action.
/// 为单个 VMM 生命周期动作收集的不含正文聚合信息。
#[derive(Debug, Default)]
struct VmmAuditAggregate {
    /// Stable affected memory identities.
    /// 受影响的稳定记忆身份。
    memory_keys: BTreeSet<String>,
    /// Semantic hashes keyed by stable memory identity.
    /// 按稳定记忆身份索引的语义哈希。
    semantic_content_hashes: BTreeMap<String, String>,
    /// Aggregate Provider-visible bytes affected by the action.
    /// 本动作影响的 Provider 可见聚合字节数。
    body_bytes: usize,
}

impl VmmAuditAggregate {
    /// Adds one lease snapshot without retaining its provider-visible text.
    /// 添加一份租约快照且不保留其 Provider 可见正文。
    ///
    /// # Parameters
    /// 参数
    /// `lease` is the exact structured lease affected by the transition.
    /// `lease` 是本次转换影响的精确结构化租约。
    fn add_lease(&mut self, lease: &VmmMemoryLease) {
        self.memory_keys.insert(lease.memory_key.clone());
        self.semantic_content_hashes.insert(
            lease.memory_key.clone(),
            lease.semantic_content_hash.clone(),
        );
        self.body_bytes = self
            .body_bytes
            .saturating_add(render_vmm_context_item_model(&lease.item).len());
    }

    /// Adds stable keys, semantic hashes, and aggregate bytes from one immutable anchor.
    /// 从一条不可变锚点添加稳定键、语义哈希与聚合字节数。
    ///
    /// # Parameters
    /// 参数
    /// `memory_keys` are the exact identities covered by the anchor.
    /// `memory_keys` 是锚点覆盖的精确身份。
    /// `semantic_content_hashes` are the body-free semantic hashes frozen by the anchor.
    /// `semantic_content_hashes` 是锚点冻结的不含正文语义哈希。
    /// `body_bytes` is the exact immutable prefix byte count.
    /// `body_bytes` 是不可变前缀的精确字节数。
    fn add_anchor(
        &mut self,
        memory_keys: &[String],
        semantic_content_hashes: &BTreeMap<String, String>,
        body_bytes: usize,
    ) {
        self.memory_keys.extend(memory_keys.iter().cloned());
        self.semantic_content_hashes.extend(
            semantic_content_hashes
                .iter()
                .map(|(key, hash)| (key.clone(), hash.clone())),
        );
        self.body_bytes = self.body_bytes.saturating_add(body_bytes);
    }
}

/// Derives body-free lifecycle events from two exact VMM state snapshots.
/// 根据两份精确 VMM 状态快照推导不含正文的生命周期事件。
///
/// # Parameters
/// 参数
/// `before` is the validated state immediately before the transition boundary.
/// `before` 是转换边界之前的已验证状态。
/// `after` is the validated state immediately after the transition boundary.
/// `after` 是转换边界之后的已验证状态。
/// `turn_id` is the logical turn identity associated with the transition.
/// `turn_id` 是与转换关联的逻辑回合身份。
/// `removed_action` identifies whether missing leases expired or were explicitly revoked.
/// `removed_action` 标识缺失租约属于到期还是显式撤销。
///
/// # Returns
/// 返回值
/// Ordered, aggregated lifecycle events containing no provider-visible memory text.
/// 返回有序聚合且不包含 Provider 可见记忆正文的生命周期事件。
pub(super) fn build_vmm_lifecycle_events_for_state_diff(
    before: &VmmSessionMemoryState,
    after: &VmmSessionMemoryState,
    turn_id: Option<&str>,
    removed_action: VmmRemovedLeaseAuditAction,
) -> Vec<RuntimeVmmLifecycleEvent> {
    // aggregates keeps serialized event ordering stable by the action enum declaration order.
    // aggregates 按动作枚举声明顺序保持序列化事件顺序稳定。
    let mut aggregates = BTreeMap::<RuntimeVmmLifecycleAction, VmmAuditAggregate>::new();
    for (memory_key, current_lease) in &after.active_leases {
        match before.active_leases.get(memory_key) {
            None => aggregates
                .entry(RuntimeVmmLifecycleAction::LeaseCreated)
                .or_default()
                .add_lease(current_lease),
            Some(previous_lease)
                if previous_lease.semantic_content_hash != current_lease.semantic_content_hash =>
            {
                aggregates
                    .entry(RuntimeVmmLifecycleAction::LeaseReplaced)
                    .or_default()
                    .add_lease(current_lease);
            }
            Some(previous_lease)
                if lease_crossed_expiry_boundary(before, after, previous_lease, current_lease) =>
            {
                aggregates
                    .entry(RuntimeVmmLifecycleAction::LeaseExpired)
                    .or_default()
                    .add_lease(current_lease);
            }
            Some(previous_lease) if lease_was_refreshed(previous_lease, current_lease) => {
                aggregates
                    .entry(RuntimeVmmLifecycleAction::LeaseRefreshed)
                    .or_default()
                    .add_lease(current_lease);
            }
            Some(_) => {}
        }
    }
    for (memory_key, previous_lease) in &before.active_leases {
        if !after.active_leases.contains_key(memory_key) {
            // Previously expired residents are reclaimed by compaction, not expired a second time.
            // 先前已过期的驻留项由压缩回收，不得重复记录为再次到期。
            let action = if removed_action == VmmRemovedLeaseAuditAction::Expired
                && previous_lease.expires_after_turn_sequence <= before.completed_turn_sequence
            {
                RuntimeVmmLifecycleAction::CompactionReclaimed
            } else {
                removed_action.runtime_action()
            };
            aggregates
                .entry(action)
                .or_default()
                .add_lease(previous_lease);
        }
    }
    // New revocation anchors prove pending body-free notices were bound to an exact message.
    // 新失效锚点证明待处理无正文通知已绑定到精确消息。
    let previous_anchor_ids = before
        .cache_epoch
        .anchors
        .iter()
        .map(|anchor| anchor.anchor_id.as_str())
        .collect::<BTreeSet<_>>();
    for anchor in &after.cache_epoch.anchors {
        if anchor.kind == VmmPromptAnchorKind::Revocation
            && !previous_anchor_ids.contains(anchor.anchor_id.as_str())
        {
            aggregates
                .entry(RuntimeVmmLifecycleAction::RevocationBound)
                .or_default()
                .add_anchor(
                    &anchor.memory_keys,
                    &anchor.semantic_content_hashes,
                    anchor.prefix.len(),
                );
        }
    }
    if before.cache_epoch.epoch_id != after.cache_epoch.epoch_id {
        // rebuilt describes the new baseline; reclaimed accounts for all old immutable bytes removed.
        // rebuilt 描述新基线；reclaimed 统计全部已移除旧不可变字节。
        let rebuilt = aggregates
            .entry(RuntimeVmmLifecycleAction::CacheEpochRebuilt)
            .or_default();
        for anchor in &after.cache_epoch.anchors {
            rebuilt.add_anchor(
                &anchor.memory_keys,
                &anchor.semantic_content_hashes,
                anchor.prefix.len(),
            );
        }
        let reclaimed = aggregates
            .entry(RuntimeVmmLifecycleAction::CompactionReclaimed)
            .or_default();
        for anchor in &before.cache_epoch.anchors {
            reclaimed.add_anchor(
                &anchor.memory_keys,
                &anchor.semantic_content_hashes,
                anchor.prefix.len(),
            );
        }
    }
    aggregates
        .into_iter()
        .enumerate()
        .map(|(index, (action, aggregate))| RuntimeVmmLifecycleEvent {
            event_id: format!(
                "vmm-{}-{}-{}-{index}",
                runtime_vmm_action_label(action),
                after.revision,
                after.completed_turn_sequence
            ),
            turn_id: turn_id.map(str::to_string),
            action,
            cache_epoch_id: Some(after.cache_epoch.epoch_id.clone()),
            anchor_revision: Some(after.cache_epoch.anchor_revision),
            completed_turn_sequence: after.completed_turn_sequence,
            memory_keys: aggregate.memory_keys.into_iter().collect(),
            semantic_content_hashes: aggregate.semantic_content_hashes,
            body_bytes: aggregate.body_bytes,
            reason: Some(runtime_vmm_action_reason_for_state(action, after).to_string()),
        })
        .collect()
}

/// Records ordered VMM lifecycle audit events in one session runtime JSONL log.
/// 将有序 VMM 生命周期审计事件记录到单个会话运行时 JSONL 日志。
///
/// # Parameters
/// 参数
/// `data_root` is the exact Vulcan Code data root owning the session directory.
/// `data_root` 是拥有会话目录的精确 Vulcan Code 数据根目录。
/// `session_id` identifies the runtime log owner.
/// `session_id` 标识运行时日志所有者。
/// `timestamp` is the shared event timestamp.
/// `timestamp` 是共享事件时间戳。
/// `events` are ordered body-free lifecycle payloads.
/// `events` 是有序且不含正文的生命周期载荷。
///
/// # Returns
/// 返回值
/// Success after every event has been appended or the session is consistently absent.
/// 在全部事件完成追加或会话一致缺失后返回成功。
pub(super) fn record_vmm_lifecycle_events(
    session_directory: &std::path::Path,
    session_id: SessionId,
    timestamp: &str,
    events: Vec<RuntimeVmmLifecycleEvent>,
) -> Result<(), String> {
    for event in events {
        vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
            session_directory,
            session_id,
            RuntimeLogEvent::VmmLifecycle(event),
            timestamp,
        )?;
    }
    Ok(())
}

/// Returns whether the existing lease changed in a way caused by a semantic recall refresh.
/// 返回既有租约是否发生了由语义相同召回刷新引起的变化。
///
/// # Parameters
/// 参数
/// `before` is the lease snapshot before the request transition.
/// `before` 是请求转换前的租约快照。
/// `after` is the lease snapshot after the request transition.
/// `after` 是请求转换后的租约快照。
///
/// # Returns
/// 返回值
/// `true` when recall timing, expiry, or structured display data changed without semantic replacement.
/// 当召回时间、到期序号或结构化展示数据变化且未发生语义替换时返回 `true`。
fn lease_was_refreshed(before: &VmmMemoryLease, after: &VmmMemoryLease) -> bool {
    before.last_recalled_turn_sequence != after.last_recalled_turn_sequence
        || before.expires_after_turn_sequence != after.expires_after_turn_sequence
        || before.item != after.item
        || before.updated_at != after.updated_at
}

/// Returns whether one retained lease crossed from active to expired between exact state snapshots.
/// 返回一条保留租约是否在两份精确状态快照之间从活动状态跨越到过期状态。
///
/// # Parameters
/// 参数
/// `before_state` is the authoritative state before the transition.
/// `before_state` 是转换前的权威状态。
/// `after_state` is the authoritative state after the transition.
/// `after_state` 是转换后的权威状态。
/// `before_lease` is the resident lease before the transition.
/// `before_lease` 是转换前的驻留租约。
/// `after_lease` is the same resident lease after the transition.
/// `after_lease` 是转换后的同一条驻留租约。
///
/// # Returns
/// 返回值
/// `true` only for the first TTL boundary crossing while the lease remains physically resident.
/// 仅当租约保持物理驻留并首次跨越 TTL 边界时返回 `true`。
fn lease_crossed_expiry_boundary(
    before_state: &VmmSessionMemoryState,
    after_state: &VmmSessionMemoryState,
    before_lease: &VmmMemoryLease,
    after_lease: &VmmMemoryLease,
) -> bool {
    before_lease.expires_after_turn_sequence > before_state.completed_turn_sequence
        && after_lease.expires_after_turn_sequence <= after_state.completed_turn_sequence
}

/// Returns the stable event-id label for one VMM lifecycle action.
/// 返回单个 VMM 生命周期动作的稳定事件标识标签。
///
/// # Parameters
/// 参数
/// `action` is the typed runtime lifecycle action.
/// `action` 是类型化运行时生命周期动作。
///
/// # Returns
/// 返回值
/// A stable kebab-case identifier component.
/// 返回稳定的短横线命名标识组成部分。
fn runtime_vmm_action_label(action: RuntimeVmmLifecycleAction) -> &'static str {
    match action {
        RuntimeVmmLifecycleAction::LeaseCreated => "lease-created",
        RuntimeVmmLifecycleAction::LeaseRefreshed => "lease-refreshed",
        RuntimeVmmLifecycleAction::LeaseReplaced => "lease-replaced",
        RuntimeVmmLifecycleAction::LeaseExpired => "lease-expired",
        RuntimeVmmLifecycleAction::LeaseRevoked => "lease-revoked",
        RuntimeVmmLifecycleAction::RevocationBound => "revocation-bound",
        RuntimeVmmLifecycleAction::CacheEpochRebuilt => "cache-epoch-rebuilt",
        RuntimeVmmLifecycleAction::CompactionReclaimed => "compaction-reclaimed",
    }
}

/// Returns a body-free stable reason for one VMM lifecycle action.
/// 返回单个 VMM 生命周期动作的不含正文稳定原因。
///
/// # Parameters
/// 参数
/// `action` is the typed runtime lifecycle action.
/// `action` 是类型化运行时生命周期动作。
///
/// # Returns
/// 返回值
/// A stable diagnostic reason suitable for runtime JSONL.
/// 返回适合运行时 JSONL 的稳定诊断原因。
fn runtime_vmm_action_reason(action: RuntimeVmmLifecycleAction) -> &'static str {
    match action {
        RuntimeVmmLifecycleAction::LeaseCreated => "typed_precheck_created_active_lease",
        RuntimeVmmLifecycleAction::LeaseRefreshed => "typed_precheck_refreshed_matching_lease",
        RuntimeVmmLifecycleAction::LeaseReplaced => "typed_precheck_appended_semantic_replacement",
        RuntimeVmmLifecycleAction::LeaseExpired => "terminal_turn_reached_absolute_ttl",
        RuntimeVmmLifecycleAction::LeaseRevoked => "effective_vmm_configuration_disabled",
        RuntimeVmmLifecycleAction::RevocationBound => {
            "pending_revocation_bound_to_exact_user_message"
        }
        RuntimeVmmLifecycleAction::CacheEpochRebuilt => "context_compaction_created_cache_epoch",
        RuntimeVmmLifecycleAction::CompactionReclaimed => {
            "context_compaction_removed_old_anchor_epoch"
        }
    }
}

/// Returns an exact body-free reason after inspecting the authoritative post-transition epoch.
/// 检查权威转换后纪元并返回精确的不含正文原因。
///
/// # Parameters
/// 参数
/// `action` is the typed runtime lifecycle action.
/// `action` 是类型化运行时生命周期动作。
/// `after` is the authoritative post-transition VMM state.
/// `after` 是权威的转换后 VMM 状态。
///
/// # Returns
/// 返回值
/// A stable reason that distinguishes compaction epochs from rollout-mode epochs.
/// 返回可区分压缩纪元与灰度模式纪元的稳定原因。
fn runtime_vmm_action_reason_for_state(
    action: RuntimeVmmLifecycleAction,
    after: &VmmSessionMemoryState,
) -> &'static str {
    if after.cache_epoch.epoch_id.starts_with("stable-anchor-") {
        return match action {
            RuntimeVmmLifecycleAction::CacheEpochRebuilt => {
                "stable_anchor_feature_mode_created_cache_epoch"
            }
            RuntimeVmmLifecycleAction::CompactionReclaimed => {
                "stable_anchor_feature_mode_removed_old_anchor_epoch"
            }
            _ => runtime_vmm_action_reason(action),
        };
    }
    runtime_vmm_action_reason(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_vmm::{ContextItem, VmmRecallBatch};

    /// Builds one typed batch whose private body must never enter lifecycle audit JSON.
    /// 构建一批类型化结果，其私有正文绝不能进入生命周期审计 JSON。
    fn private_recall_batch(ttl_turns: u32) -> VmmRecallBatch {
        VmmRecallBatch {
            ttl_turns,
            injection_tag: String::from("VMM_MEMORY"),
            injection_description: String::from("Relevant memories."),
            items: vec![ContextItem {
                text: String::from("private-memory-body"),
                score: 0.8,
                turn_id: 3,
                has_dialogue: true,
                created_datetime: String::from("2026-07-20"),
                memory_id: 7,
            }],
        }
    }

    /// Verifies create and expiry audit payloads retain hashes and sizes but never memory text.
    /// 验证创建与到期审计载荷会保留哈希和大小，但绝不保留记忆正文。
    #[test]
    fn lifecycle_diff_is_body_free_for_create_and_expiry() {
        let pristine = VmmSessionMemoryState::default();
        let mut active = pristine.clone();
        active
            .apply_recall_batch("session-a", "user-1", &private_recall_batch(1), "1")
            .expect("typed recall should succeed");
        active
            .register_active_turn("turn-1", "1")
            .expect("logical turn should register before terminal expiry");
        let created = build_vmm_lifecycle_events_for_state_diff(
            &pristine,
            &active,
            Some("turn-1"),
            VmmRemovedLeaseAuditAction::Revoked,
        );
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].action, RuntimeVmmLifecycleAction::LeaseCreated);
        let created_json = serde_json::to_string(&created).expect("serialize create audit");
        assert!(!created_json.contains("private-memory-body"));
        assert!(created_json.contains("memory:7"));

        let before_expiry = active.clone();
        active
            .consume_turn("turn-1", "2")
            .expect("terminal TTL should expire the lease");
        let expired = build_vmm_lifecycle_events_for_state_diff(
            &before_expiry,
            &active,
            Some("turn-1"),
            VmmRemovedLeaseAuditAction::Expired,
        );
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].action, RuntimeVmmLifecycleAction::LeaseExpired);
        let expired_json = serde_json::to_string(&expired).expect("serialize expiry audit");
        assert!(!expired_json.contains("private-memory-body"));

        let before_compaction = active.clone();
        active
            .begin_cache_epoch("compact-expired", None, "3")
            .expect("compaction should reclaim the expired resident");
        let reclaimed = build_vmm_lifecycle_events_for_state_diff(
            &before_compaction,
            &active,
            None,
            VmmRemovedLeaseAuditAction::Expired,
        );
        assert!(
            reclaimed
                .iter()
                .all(|event| { event.action != RuntimeVmmLifecycleAction::LeaseExpired })
        );
        assert!(
            reclaimed
                .iter()
                .any(|event| { event.action == RuntimeVmmLifecycleAction::CompactionReclaimed })
        );
    }

    /// Verifies compaction emits both epoch rebuild and physical reclaim audit actions.
    /// 验证压缩会同时发出纪元重建与物理回收审计动作。
    #[test]
    fn lifecycle_diff_reports_cache_epoch_rebuild_and_reclaimed_bytes() {
        let mut before = VmmSessionMemoryState::default();
        before
            .apply_recall_batch("session-a", "user-1", &private_recall_batch(3), "1")
            .expect("typed recall should succeed");
        let mut after = before.clone();
        after
            .begin_cache_epoch("compact-1", Some("continuation-1"), "2")
            .expect("compaction epoch should succeed");

        let events = build_vmm_lifecycle_events_for_state_diff(
            &before,
            &after,
            None,
            VmmRemovedLeaseAuditAction::Expired,
        );

        assert!(events.iter().any(|event| {
            event.action == RuntimeVmmLifecycleAction::CacheEpochRebuilt && event.body_bytes > 0
        }));
        assert!(events.iter().any(|event| {
            event.action == RuntimeVmmLifecycleAction::CompactionReclaimed && event.body_bytes > 0
        }));
    }
}
