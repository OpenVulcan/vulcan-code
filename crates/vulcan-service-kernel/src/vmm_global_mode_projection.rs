//! Derived memory cache invalidation at the global configuration publication boundary.
//! 全局配置发布边界的派生记忆缓存失效。

use super::*;
use vulcan_service_api::ContextInjectionSnapshot;

/// Removes stale VMM-only projections while preserving all stored memory and other feature domains.
/// 移除过时的 VMM 专属投影，保留全部存储记忆及其他功能领域。
/// Takes the already locked service state; performs no directory lookup, hydration, RPC or inference.
/// 接收已加锁服务状态；不查找目录、水合会话、执行 RPC 或推理。
pub(super) fn clear_global_mode_caches(guard: &mut ServiceBridgeState) -> Result<(), String> {
    // The registry is the only durable owner; this clears derived cache files, not either backend's data.
    // 注册表是唯一持久归属；这里清除派生缓存文件，不清除任一后端的数据。
    let now = crate::workbench_refresh::current_unix_millis_text();
    guard
        .vmm_controller_registry
        .clear_resident_memory_sources(&now)?;
    if let Some(workspace) = guard.workspace_snapshot.as_mut() {
        for runtime in workspace
            .session_runtime_state
            .values_mut()
            .chain(workspace.archived_session_runtime_state.values_mut())
        {
            runtime.vmm_profile_sync = Default::default();
            vulcan_agent_runtime::agent_runtime::vmm_hooks::clear_vmm_profile_injections(
                &mut runtime.context_injection_state,
                &mut Vec::new(),
            );
        }
    }
    // A cold row's previously rendered card can outlive its adapter; strip only known VMM producers.
    // 冷会话此前渲染的卡片可能长于适配器存活；只移除已知 VMM 生产者。
    for entry in &mut guard.workbench.context_injection_by_conversation {
        if let Some(snapshot) = entry.context_injection.as_mut() {
            clear_vmm_cards(snapshot);
        }
    }
    if let Some(snapshot) = guard.workbench.context_injection.as_mut() {
        clear_vmm_cards(snapshot);
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    Ok(())
}

/// Clears the fixed cards emitted by the VMM state and profile producers, preserving unrelated groups.
/// 清除 VMM 状态及画像生产者发出的固定卡片，保留无关分组。
/// Takes one retained UI snapshot; does not infer ownership from content or user-defined tag fragments.
/// 接收一个保留界面快照；不根据正文或用户定义标签片段推断归属。
fn clear_vmm_cards(snapshot: &mut ContextInjectionSnapshot) {
    // Exact tags are defined by VmmSessionMemoryState::active_panel_card and expired_panel_card.
    // 准确标签由 VmmSessionMemoryState::active_panel_card 和 expired_panel_card 定义。
    snapshot
        .dynamic_cards
        .retain(|card| !matches!(card.tag.as_str(), "VMM_MEMORY" | "VMM_MEMORY_EXPIRED"));
    // This fixed group is owned by the Agent VMM profile-delta producer, not by generic persistent injections.
    // 此固定分组属于 Agent VMM 画像增量生产者，而非通用持久注入。
    snapshot
        .static_cards
        .retain(|card| card.tag != "vmm_profile_delta");
    snapshot.static_profile = None;
}

/// Verifies the actual configuration callback clears derived caches without deleting other feature injections.
/// 验证实际配置回调清除派生缓存，不删除其他功能注入。
/// Uses a durable session controller plus active, archived and cold UI mirrors; no provider or user files are accessed.
/// 使用持久会话控制器及活动、归档、冷界面镜像；不访问供应商或用户文件。
#[test]
fn global_memory_mode_clears_resident_and_retained_caches_without_hydration() {
    use crate::tests::test_support::{
        TestControllerRuntimeRoot, test_agent_runtime_bridge,
        test_service_state_with_parent_session,
    };
    use vulcan_context_injection::{InjectionInsert, InjectionKind};
    use vulcan_vmm::{VmmBackendMode, VmmConfigLayer, VmmMemoryReadSource, VmmMemorySource};
    use vulcan_workbench_model::VmmProfileSyncState;

    // The only durable cache belongs to an explicitly created isolated session root.
    // 唯一持久缓存属于明确创建的隔离会话目录。
    let session_id = SessionId::new_v4();
    let root = TestControllerRuntimeRoot::new(session_id);
    let mut service = test_service_state_with_parent_session(session_id);
    let controller = service
        .vmm_controller_registry
        .get_or_open(root.v1_storage_locator(), session_id, "1")
        .expect("resident controller");
    let source = VmmMemoryReadSource::new(VmmMemorySource::native(1).expect("owner"), vec![1])
        .expect("source");
    controller
        .execute_command("real-cache-seed", "2", |candidate| {
            candidate.select_read_source(&session_id.to_string(), Some(&source), "2")?;
            candidate.apply_recall_batch(
                &session_id.to_string(),
                "user-1",
                &vulcan_vmm::VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: "vmm".to_owned(),
                    injection_description: "memory".to_owned(),
                    items: vec![vulcan_vmm::ContextItem {
                        memory_id: 1,
                        text: "old memory".to_owned(),
                        ..Default::default()
                    }],
                },
                "2",
            )?;
            Ok(())
        })
        .expect("seed");
    // The global Hook owner is active before the configuration callback disables it.
    // 配置回调关闭前，全局 Hook 归属处于活动状态。
    let mut agent = test_agent_runtime_bridge();
    agent.provider_configuration.vmm = VmmConfigLayer {
        enabled: Some(true),
        backend_mode: Some(VmmBackendMode::Native),
        ..Default::default()
    };
    agent
        .vmm_hook_runtime
        .replace_snapshot(VmmHookConfigSnapshot {
            global: agent.provider_configuration.vmm.clone(),
            ..Default::default()
        })
        .expect("initial hook config");
    let mut configuration = agent.provider_configuration.clone();
    service.agent_runtime = Some(agent);
    let profile = VmmProfileSyncState {
        read_source: Some(source),
        base_profile_content: Some("old baseline".to_owned()),
        ..Default::default()
    };
    let mut row = SessionRuntimeStateEntry {
        vmm_profile_sync: profile.clone(),
        ..Default::default()
    };
    for (tag, body) in [
        ("vmm_profile_delta", "old profile delta"),
        ("knowledge", "keep knowledge"),
    ] {
        row.context_injection_state.apply_insert(
            InjectionInsert::single(InjectionKind::Persistent, tag, tag, body, None),
            "2",
        );
    }
    // Keep a genuine nonempty VMM card alongside another domain's persistent group.
    // 将真实非空 VMM 卡片与另一领域的持久分组一起保留。
    let snapshot = crate::workbench_session_panels::context_injection_snapshot_from_runtime_state(
        Some(&row),
        Some(&controller.snapshot().expect("cached memory")),
    )
    .expect("panel");
    let archived_id = SessionId::new_v4();
    let cold_id = SessionId::new_v4();
    let workspace = service.workspace_snapshot.as_mut().expect("workspace");
    workspace
        .session_runtime_state
        .insert(session_id, row.clone());
    workspace
        .archived_session_runtime_state
        .insert(archived_id, row);
    service.workbench.context_injection = Some(snapshot.clone());
    service.workbench.context_injection_by_conversation =
        vec![vulcan_service_api::ContextInjectionChangedDelta {
            conversation_id: cold_id.to_string(),
            context_injection: Some(snapshot),
        }];
    configuration.vmm.enabled = Some(false);
    refresh_guard_vmm_hook_snapshot(&mut service, &configuration).expect("disable publication");
    let disabled = controller.snapshot().expect("disabled durable cache");
    assert!(disabled.read_source.is_none());
    assert!(disabled.active_leases.is_empty());
    assert!(disabled.cache_epoch.anchors.is_empty());
    assert!(disabled.has_committed_controller_command("real-cache-seed"));
    assert!(
        !service
            .vmm_controller_registry
            .contains(cold_id)
            .expect("cold unopened")
    );
    assert!(
        !service
            .vmm_controller_registry
            .contains(archived_id)
            .expect("archived unopened")
    );
    for panel in [
        service
            .workbench
            .context_injection
            .as_ref()
            .expect("selected"),
        service.workbench.context_injection_by_conversation[0]
            .context_injection
            .as_ref()
            .expect("cold card"),
    ] {
        assert!(panel.dynamic_cards.is_empty());
        assert_eq!(panel.static_cards.len(), 1);
        assert_eq!(panel.static_cards[0].tag, "knowledge");
    }
    let workspace = service.workspace_snapshot.as_ref().expect("workspace");
    for runtime in [
        workspace
            .session_runtime_state
            .get(&session_id)
            .expect("active"),
        workspace
            .archived_session_runtime_state
            .get(&archived_id)
            .expect("archived"),
    ] {
        assert!(runtime.vmm_profile_sync.is_empty());
        let mut other = runtime.context_injection_state.clone();
        assert!(!other.remove_group(Some(InjectionKind::Persistent), "vmm_profile_delta"));
        assert!(other.remove_group(Some(InjectionKind::Persistent), "knowledge"));
    }
    // Re-enable and tune without reconstructing old leases or consuming another controller receipt.
    // 重新启用并调参，不重建旧租约，也不消费另一控制器回执。
    configuration.vmm.enabled = Some(true);
    refresh_guard_vmm_hook_snapshot(&mut service, &configuration).expect("re-enable");
    assert_eq!(controller.snapshot().expect("no resurrection"), disabled);
    service
        .workspace_snapshot
        .as_mut()
        .expect("workspace")
        .session_runtime_state
        .get_mut(&session_id)
        .expect("row")
        .vmm_profile_sync = profile.clone();
    configuration.vmm.implicit_memory_turns = Some(7);
    refresh_guard_vmm_hook_snapshot(&mut service, &configuration).expect("ordinary tuning");
    assert_eq!(
        service
            .workspace_snapshot
            .as_ref()
            .expect("workspace")
            .session_runtime_state[&session_id]
            .vmm_profile_sync,
        profile
    );
    configuration.vmm.backend_mode = Some(VmmBackendMode::ExternalGrpc);
    refresh_guard_vmm_hook_snapshot(&mut service, &configuration).expect("backend switch");
    assert!(
        service
            .workspace_snapshot
            .as_ref()
            .expect("workspace")
            .session_runtime_state[&session_id]
            .vmm_profile_sync
            .is_empty()
    );
    service
        .vmm_controller_registry
        .begin_lifecycle_transition([session_id])
        .expect("close")
        .release()
        .expect("release");
    let reopened = service
        .vmm_controller_registry
        .get_or_open(root.v1_storage_locator(), session_id, "3")
        .expect("reopen");
    assert_eq!(reopened.snapshot().expect("durable reset"), disabled);
}
