//! Exact native profile cache validation against committed nodes.
//! 对已提交节点执行准确的原生画像缓存校验。

use vulcan_memorymesh_core::{InMemoryStorage, ProfileNodeId, ProfileNodeState};
use vulcan_vmm::{ProfileTarget, VmmMemorySource};
use vulcan_workbench_model::{SessionRuntimeStateEntry, VmmProfileSyncState};

/// Checks whether any cached native node is unavailable or its prompt semantics have changed.
/// 检查是否有缓存原生节点已不可用，或其提示词语义已经改变。
/// Takes borrowed committed storage, the exact cache and one business time; returns validation errors explicitly.
/// 接收借用的已提交存储、准确缓存及一个业务时间；明确返回校验错误。
pub(super) fn profile_cache_is_stale(
    storage: &InMemoryStorage,
    cached: &VmmProfileSyncState,
    now_ns: i64,
) -> Result<bool, String> {
    // Unknown and external sources are not native cache ownership and must not be relabeled here.
    // 未知及外置来源不属于原生缓存，不能在此重新标记。
    let Some(source) = &cached.read_source else {
        return Ok(false);
    };
    if !matches!(source.owner, VmmMemorySource::Native { .. }) {
        return Ok(false);
    }
    source.validate().map_err(|error| error.to_string())?;
    // Native personal profiles share the single host user, independent of project association.
    // 原生个人画像共用唯一宿主用户，独立于项目关联。
    let subject = super::native_vmm_subject_id()?;
    // Personal visibility is the fixed subject scope, never the current project scope.
    // 个人可见性使用固定主体作用域，绝不使用当前项目作用域。
    let personal_scope = super::native_vmm_subject_scope()?;
    for (node_id, cached_node) in &cached.nodes_by_id {
        if *node_id == 0 || *node_id != cached_node.profile_node_id {
            return Err("Native profile cache contains an inconsistent node identity.".to_owned());
        }
        // The numeric protocol identity maps reversibly to one canonical Core node ID.
        // 数字协议身份可逆映射到一个规范 Core 节点 ID。
        let core_id = ProfileNodeId::new(format!("vmm-profile-{node_id}"))
            .map_err(|error| error.to_string())?;
        // Missing nodes are genuinely removed from the active store, not guessed from old UI text.
        // 缺失节点确实已从活动存储移除，而非根据旧界面文字猜测。
        let Some(node) = storage.profile_node_record(&core_id) else {
            return Ok(true);
        };
        // Match the same environment-versus-subject rules as the production profile query.
        // 匹配生产画像查询使用的相同环境与主体规则。
        let visible = match cached_node.target {
            ProfileTarget::ProfileTargetUser => {
                cached_node.bind_id == super::NATIVE_VMM_USER_ID
                    && node.scope_id == personal_scope
                    && node.subject_id.as_ref() == Some(&subject)
            }
            ProfileTarget::ProfileTargetProject => {
                source.visible_project_ids.contains(&cached_node.bind_id)
                    && node.scope_id == super::project_scope(cached_node.bind_id)?
            }
            _ => {
                return Err(
                    "Native profile cache contains an unsupported profile target.".to_owned(),
                );
            }
        };
        if !visible
            || node.state != ProfileNodeState::Active
            || node.effective_date_ns.is_some_and(|date| date > now_ns)
            || node.expires_at_ns.is_some_and(|expiry| expiry <= now_ns)
        {
            return Ok(true);
        }
        // Use the canonical client projection for prompt fields, not a second date/priority encoding.
        // 提示词字段使用规范客户端投影，不增加第二套日期或优先级编码。
        let current =
            super::super::client::profile_node_entry(node).map_err(|error| error.to_string())?;
        // Routine weight renewal and still-valid expiry changes follow the configured refresh cadence.
        // 常规权重续期及仍有效的期限变化遵循已配置刷新节奏。
        // Content, ownership, date grouping, priority and lifetime-level changes invalidate old constraints.
        // 正文、归属、日期分组、优先级及生命周期等级改变会使旧约束失效。
        if current.content != cached_node.content
            || current.target != cached_node.target
            || current.bind_id != cached_node.bind_id
            || current.profile_date != cached_node.profile_date
            || current.priority != cached_node.priority
            || current.level != cached_node.level
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Clears an invalid combined profile cache without deleting any durable personal or project node.
/// 清除失效的合并画像缓存，不删除任何持久个人或项目节点。
/// Takes current storage, one retained runtime and business time; returns whether its cache changed.
/// 接收当前存储、一条保留运行态及业务时间；返回其缓存是否改变。
pub(super) fn reconcile_cached_profile(
    storage: &InMemoryStorage,
    runtime: &mut SessionRuntimeStateEntry,
    now_ns: i64,
) -> Result<bool, String> {
    if !profile_cache_is_stale(storage, &runtime.vmm_profile_sync, now_ns)? {
        return Ok(false);
    }
    runtime.vmm_profile_sync = VmmProfileSyncState {
        read_source: runtime.vmm_profile_sync.read_source.clone(),
        ..Default::default()
    };
    // The next ordinary preparation rebuilds all still-valid roles from their unchanged durable owners.
    // 下次普通准备从未改变的持久归属重建全部仍有效角色。
    vulcan_agent_runtime::agent_runtime::vmm_hooks::clear_vmm_profile_injections(
        &mut runtime.context_injection_state,
        &mut Vec::new(),
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_memorymesh_core::{ProfileLevel, ProfileNode, ProfileSourceKind, StorageSnapshot};

    /// Builds one valid native project node and its canonical cached client representation.
    /// 构建一个有效原生项目节点及其规范缓存客户端表示。
    /// Returns a typed storage snapshot and cache with nonempty baseline and refresh metadata.
    /// 返回类型化存储快照及具有非空基线和刷新元数据的缓存。
    fn fixture() -> (StorageSnapshot, VmmProfileSyncState) {
        // System-seed provenance avoids inventing any source turn or instruction record.
        // 系统种子来源避免虚构任何来源回合或指令记录。
        let node = ProfileNode {
            id: ProfileNodeId::new("vmm-profile-11").expect("node"),
            subject_id: Some(super::super::native_vmm_subject_id().expect("subject")),
            scope_id: super::super::project_scope(7).expect("scope"),
            content: "Use transactional writes.".to_owned(),
            confidence: Some(0.9),
            state: ProfileNodeState::Active,
            source_turn_ids: Vec::new(),
            source_kind: ProfileSourceKind::SystemSeed,
            source_id: None,
            evidence_source: None,
            priority: 60,
            profile_level: ProfileLevel::Stable,
            level_reason: None,
            refresh_weight: 1,
            effective_date_ns: Some(1),
            expires_at_ns: Some(2_000),
            status_reason: None,
            superseded_by: None,
            version: 1,
            created_at_ns: 1,
            updated_at_ns: 1,
        };
        // Only production projection defines the cached dates, priority and lifetime labels.
        // 只有生产投影定义缓存日期、优先级及生命周期标签。
        let entry = super::super::super::client::profile_node_entry(&node).expect("entry");
        // Hydration consumes an otherwise empty real storage snapshot.
        // 水合消费其他部分为空的真实存储快照。
        let mut storage = InMemoryStorage::new().snapshot();
        storage.dataset.profile_nodes.push(node);
        // Owner-first visibility includes a linked scope to detect accidental project reassignment.
        // 归属优先可见范围包含连带作用域，以检测意外项目重新归属。
        let cached = VmmProfileSyncState {
            read_source: Some(
                vulcan_vmm::VmmMemoryReadSource::new(
                    VmmMemorySource::native(7).expect("owner"),
                    vec![7, 9],
                )
                .expect("source"),
            ),
            nodes_by_id: std::collections::BTreeMap::from([(11, entry)]),
            base_profile_content: Some("cached active baseline".to_owned()),
            delta_profile_content: Some("cached delta".to_owned()),
            covered_compaction_id: Some("compaction-a".to_owned()),
            completed_turns_since_sync: 2,
            updated_at: Some("1".to_owned()),
        };
        (storage, cached)
    }

    /// Tests concrete invalidity cases without treating normal renewal as a baseline reset.
    /// 验证具体失效情况，不将普通续期视为基线重置。
    /// Uses the same public storage hydration and native validator as production commits.
    /// 使用与生产提交相同的公共存储水合及原生校验器。
    #[test]
    fn native_profile_cache_invalidity_preserves_normal_renewal() {
        // Baseline validity is proven before introducing any individual change.
        // 引入任何单独变化前先证明基线有效。
        let (before, cached) = fixture();
        assert!(
            !profile_cache_is_stale(
                &InMemoryStorage::from_snapshot(before.clone()).expect("baseline"),
                &cached,
                1_000
            )
            .expect("valid baseline")
        );
        for case in 0..8 {
            // Each case starts from the same valid snapshot and changes only its declared field.
            // 每种情况从同一有效快照开始，只改变其声明字段。
            let mut changed = before.clone();
            match case {
                0 => changed.dataset.profile_nodes.clear(),
                1 => changed.dataset.profile_nodes[0].state = ProfileNodeState::Inactive,
                2 => changed.dataset.profile_nodes[0].state = ProfileNodeState::Expired,
                3 => changed.dataset.profile_nodes[0].expires_at_ns = Some(1_000),
                4 => changed.dataset.profile_nodes[0].effective_date_ns = Some(1_001),
                5 => changed.dataset.profile_nodes[0].content = "Changed constraint.".to_owned(),
                6 => {
                    changed.dataset.profile_nodes[0].scope_id =
                        super::super::project_scope(9).expect("other scope")
                }
                7 => changed.dataset.profile_nodes[0].priority = 100,
                _ => unreachable!(),
            }
            // Validate each durable fixture before checking its cached representation.
            // 检查缓存表示之前，先校验每个持久夹具。
            let storage = InMemoryStorage::from_snapshot(changed).expect("valid changed storage");
            assert!(
                profile_cache_is_stale(&storage, &cached, 1_000).expect("stale query"),
                "case {case}"
            );
        }
        // Weight and still-valid absolute expiry may advance without defeating profile_refresh_turns.
        // 权重及仍有效的绝对期限可以推进，不破坏 profile_refresh_turns。
        let mut renewed = before;
        renewed.dataset.profile_nodes[0].refresh_weight = 9;
        renewed.dataset.profile_nodes[0].expires_at_ns = Some(4_000);
        // Renewal is valid durable storage, not a simulated validator return value.
        // 续期是有效持久存储，不是模拟的校验器返回值。
        let storage = InMemoryStorage::from_snapshot(renewed).expect("renewed storage");
        // Retained counters, baseline and delta must all remain unchanged after renewal.
        // 续期后保留计数器、基线及增量必须全部保持不变。
        let mut runtime = SessionRuntimeStateEntry {
            vmm_profile_sync: cached.clone(),
            ..Default::default()
        };
        assert!(
            !reconcile_cached_profile(&storage, &mut runtime, 1_000)
                .expect("renewal keeps baseline")
        );
        assert_eq!(runtime.vmm_profile_sync, cached);
    }
}
