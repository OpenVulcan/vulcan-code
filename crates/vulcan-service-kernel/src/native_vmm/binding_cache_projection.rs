//! Resident memory projection after native host-project binding changes.
//! 原生宿主项目绑定变化后的驻留记忆投影。

use super::*;
use crate::state::SharedServiceBridgeState;
use vulcan_service_api::{WorkbenchDelta, WorkbenchDeltaEnvelope};
use vulcan_vmm::{VmmMemoryReadSource, VmmMemorySource};
use vulcan_workbench_model::VmmProfileSyncState;

impl NativeVmmMaintenance {
    /// Publishes the current binding to resident recall and profile caches after a management action.
    /// 在管理动作后将当前绑定发布到驻留召回及画像缓存。
    /// Takes the shared host and exact host project ID; returns UI deltas without loading cold sessions or running AI.
    /// 接收共享宿主及准确宿主项目 ID；返回界面增量，不加载冷会话或执行 AI。
    pub(crate) async fn reconcile_host_binding_caches(
        &self,
        state: &SharedServiceBridgeState,
        host_project_id: &str,
    ) -> Result<Vec<WorkbenchDeltaEnvelope>, String> {
        // Freeze binding and native publication before taking the service/registry/Hook locks.
        // 在获取服务、注册表及 Hook 锁前冻结绑定及原生发布。
        let _operation_guard = self.operation_gate.write().await;
        // Absence is an explicit unbind, not an error fallback or a guessed default project.
        // 缺失代表明确解绑，不是错误兜底或猜测的默认项目。
        let source = if self.bound_project(host_project_id).await?.is_some() {
            let binding = self
                .data_plane
                .resolve_project_access(host_project_id)
                .await?;
            // Derive ordered visibility from the same stable IDs used by normal native admission.
            // 从正常原生准入使用的相同稳定 ID 派生有序可见范围。
            let write_scope =
                ScopeId::new(binding.owner.scope_id).map_err(|error| error.to_string())?;
            let mut visible_scopes = vec![write_scope.clone()];
            for linked in binding.linked {
                visible_scopes
                    .push(ScopeId::new(linked.scope_id).map_err(|error| error.to_string())?);
            }
            Some(
                super::super::client::memory_read_source_from_access(&ScopeAccess {
                    write_scope,
                    visible_scopes,
                })
                .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        // Metadata and retained mirrors are inspected atomically; no path guessing or V1 hydration occurs.
        // 原子检查元数据及保留镜像；不猜测路径，也不水合 V1。
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        let registry = guard.vmm_controller_registry.clone();
        let hooks = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.vmm_hook_runtime.clone());
        // UI projections can outlive evicted runtime rows; capture only already displayed identities.
        // 界面投影可能长于已驱逐运行态行存活；只捕获已展示身份。
        let displayed_ids = guard
            .workbench
            .context_injection_by_conversation
            .iter()
            .map(|entry| entry.conversation_id.clone())
            .collect::<BTreeSet<_>>();
        // The selected panel may exist independently from the retained conversation list.
        // 选中面板可能独立于保留会话列表存在。
        let selected_id = guard.workbench.selected_conversation_id.clone();
        let Some(workspace) = guard.workspace_snapshot.as_mut() else {
            return Ok(Vec::new());
        };
        // Both collections are declared workspace owners; archived sessions are never relabeled as active.
        // 两个集合均为声明的工作区归属；不将归档会话重新标记为活动。
        let session_ids = workspace
            .sessions
            .iter()
            .chain(workspace.archived_sessions.iter())
            .filter(|session| {
                crate::runtime_workspace_helpers::session_summary_project_id(session)
                    == Some(host_project_id)
            })
            .map(|session| session.id)
            .collect::<BTreeSet<_>>();
        let updated_at = crate::workbench_refresh::current_unix_millis_text();
        let snapshots = registry.reconcile_resident_native_sources(
            &session_ids,
            source.as_ref(),
            &updated_at,
        )?;
        // Collect exact-session deltas while borrowing only retained runtime rows.
        // 仅借用保留运行态行时收集准确会话增量。
        let mut deltas = Vec::new();
        // Distinguish real retained rows from metadata-only owners without creating adapters.
        // 区分真实保留行和仅有元数据的归属，不创建适配器。
        let mut projected_ids = BTreeSet::new();
        for (session_id, runtime) in workspace
            .session_runtime_state
            .iter_mut()
            .chain(workspace.archived_session_runtime_state.iter_mut())
            .filter(|(session_id, _)| session_ids.contains(session_id))
        {
            projected_ids.insert(*session_id);
            if reconcile_profile_source(runtime, source.as_ref())
                && let Some(hooks) = &hooks
            {
                hooks.store_profile_sync_state(*session_id, runtime.vmm_profile_sync.clone())?;
            }
            deltas.push(
                crate::workbench_session_panels::context_injection_changed_delta_from_runtime_state(
                    *session_id,
                    Some(runtime),
                    snapshots.get(session_id),
                ),
            );
        }
        for session_id in session_ids.difference(&projected_ids) {
            // Evicted rows have no live projection; clear only stale visible cards, never fan out to all cold chats.
            // 已驱逐行没有活动投影；只清除过时可见卡片，不向全部冷聊天广播。
            let id = session_id.to_string();
            if displayed_ids.contains(&id) || selected_id.as_deref() == Some(id.as_str()) {
                deltas.push(crate::workbench_session_panels::context_injection_changed_delta_from_runtime_state(
                    *session_id, None, None,
                ));
            }
        }
        // Publish only context projections; never replace other domains' concurrently maintained workbench data.
        // 只发布上下文投影；不替换其他领域并行维护的工作台数据。
        let mut events = Vec::new();
        for delta in deltas {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let generation = guard.workbench_generation;
            guard.workbench.revision = generation;
            if guard.workbench.selected_conversation_id.as_deref()
                == Some(delta.conversation_id.as_str())
            {
                guard.workbench.context_injection = delta.context_injection.clone();
            }
            crate::workbench_session_panels::replace_conversation_scoped_projection_entry(
                &mut guard.workbench.context_injection_by_conversation,
                &delta.conversation_id,
                delta.context_injection.is_some().then(|| delta.clone()),
                |entry| entry.conversation_id.as_str(),
            );
            events.push(WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::ContextInjectionChanged(delta),
            });
        }
        Ok(events)
    }
}

/// Clears only a native profile baseline whose declared read source changed.
/// 仅清除声明读取来源已变化的原生画像基线。
/// Takes one retained row and current binding; returns whether the Hook mirror must be updated.
/// 接收一条保留行及当前绑定；返回是否需要更新 Hook 镜像。
fn reconcile_profile_source(
    runtime: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    source: Option<&VmmMemoryReadSource>,
) -> bool {
    if !runtime
        .vmm_profile_sync
        .read_source
        .as_ref()
        .is_some_and(|previous| {
            matches!(previous.owner, VmmMemorySource::Native { .. }) && Some(previous) != source
        })
    {
        return false;
    }
    runtime.vmm_profile_sync = VmmProfileSyncState {
        read_source: source.cloned(),
        ..Default::default()
    };
    // Remove only VMM-owned profile injections, preserving knowledge-base and user-owned groups.
    // 只移除 VMM 自有画像注入，保留知识库及用户自有分组。
    vulcan_agent_runtime::agent_runtime::vmm_hooks::clear_vmm_profile_injections(
        &mut runtime.context_injection_state,
        &mut Vec::new(),
    );
    true
}

/// Checks native profile source reset without changing stored profile data or unrelated runtime fields.
/// 检查原生画像来源重置，不更改存储画像数据或无关运行态字段。
/// Uses explicit owner/link changes and unbind; returns after unchanged-source metadata remains intact.
/// 使用明确归属、连带变化及解绑；同来源元数据保持完整后返回。
#[test]
fn resident_native_profile_binding_reconciliation_preserves_same_source() {
    // Use stable source IDs rather than display names to exercise linked-project changes.
    // 使用稳定来源 ID 而非显示名称，覆盖连带项目变化。
    let old = VmmMemoryReadSource::new(VmmMemorySource::native(1).expect("owner"), vec![1])
        .expect("source");
    let linked = VmmMemoryReadSource::new(VmmMemorySource::native(1).expect("owner"), vec![1, 2])
        .expect("linked");
    // Initialize the complete test projection explicitly while preserving all unrelated defaults.
    // 明确初始化完整测试投影，同时保留全部无关默认值。
    let mut runtime = vulcan_workbench_model::SessionRuntimeStateEntry {
        active_model_id: Some("unrelated-model".to_owned()),
        vmm_profile_sync: VmmProfileSyncState {
            read_source: Some(old.clone()),
            base_profile_content: Some("old profile baseline".to_owned()),
            delta_profile_content: Some("old delta".to_owned()),
            covered_compaction_id: Some("compact-1".to_owned()),
            completed_turns_since_sync: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let before = runtime.vmm_profile_sync.clone();
    assert!(!reconcile_profile_source(&mut runtime, Some(&old)));
    assert_eq!(runtime.vmm_profile_sync, before);
    assert!(reconcile_profile_source(&mut runtime, Some(&linked)));
    assert_eq!(
        runtime.vmm_profile_sync,
        VmmProfileSyncState {
            read_source: Some(linked),
            ..Default::default()
        }
    );
    assert_eq!(runtime.active_model_id.as_deref(), Some("unrelated-model"));
    assert!(reconcile_profile_source(&mut runtime, None));
    assert!(runtime.vmm_profile_sync.is_empty());
    assert!(!reconcile_profile_source(&mut runtime, None));
}
