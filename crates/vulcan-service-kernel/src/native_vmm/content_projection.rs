//! Synchronous resident recall projection after authoritative native content commits.
//! 权威原生内容提交后的同步驻留召回投影。

use crate::state::{ServiceBridgeState, SharedServiceBridgeState};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, Weak};
use uuid::Uuid;
use vulcan_memorymesh_core::{InMemoryStorage, ScopeAccess, SessionSourceId};
use vulcan_service_api::{
    ContextInjectionChangedDelta, ContextInjectionSnapshot, ServiceEvent, ServiceEventEmitter,
    WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_vmm::{VmmMemorySource, VmmSessionMemoryState};

/// Updates existing host caches without owning the host or opening cold session documents.
/// 更新既有宿主缓存，不拥有宿主，也不打开冷会话文档。
#[derive(Clone)]
pub(in crate::native_vmm) struct NativeVmmContentProjection {
    /// Weak service ownership prevents a runtime-to-service reference cycle.
    /// 弱服务归属防止运行时到服务的循环引用。
    state: Weak<Mutex<ServiceBridgeState>>,
    /// Existing service event channel used only after all synchronous state guards are released.
    /// 仅在全部同步状态守卫释放后使用的既有服务事件通道。
    events: ServiceEventEmitter,
}

impl NativeVmmContentProjection {
    /// Binds the existing service and event channel; returns a non-owning projection sink.
    /// 绑定既有服务及事件通道；返回不拥有服务的投影接收器。
    pub(in crate::native_vmm) fn new(
        state: &SharedServiceBridgeState,
        events: &ServiceEventEmitter,
    ) -> Self {
        Self {
            state: Arc::downgrade(state),
            events: events.clone(),
        }
    }

    /// Reconciles resident caches against borrowed committed storage and returns exact UI deltas.
    /// 根据借用的已提交存储对账驻留缓存，并返回准确界面增量。
    /// The caller holds the native mutation gate and a storage read guard, never its write guard.
    /// 调用方持有原生变更门及存储读守卫，绝不持有存储写守卫。
    pub(in crate::native_vmm) fn reconcile(
        &self,
        storage: &InMemoryStorage,
    ) -> Result<Vec<WorkbenchDeltaEnvelope>, String> {
        // A stopped service has no live cache consumer and must not be resurrected by a worker.
        // 已停止服务没有活动缓存消费方，不能由工作器将其复活。
        let Some(state) = self.state.upgrade() else {
            return Ok(Vec::new());
        };
        // Keep lifecycle and UI updates in the established service-to-registry-to-controller order.
        // 按既有服务、注册表、控制器顺序维持生命周期及界面更新。
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        // One timestamp is shared by every cache changed by this committed transition.
        // 本次已提交转换变更的全部缓存共用一个时间戳。
        let updated_at = crate::workbench_refresh::current_unix_millis_text();
        // The registry visits only resident entries and holds its lifecycle lock through each command.
        // 注册表只访问驻留项，并在每个命令期间持有生命周期锁。
        let snapshots = guard
            .vmm_controller_registry
            .map_resident(|session_id, controller| {
                // Inspect a detached candidate to avoid persisting receipts for unchanged content.
                // 检查脱离候选，避免为未变化内容持久化回执。
                let before = controller.snapshot()?;
                // Only the bounded per-session cache is copied; no Core dataset or embedding is cloned.
                // 只复制有界每会话缓存；不克隆 Core 数据集或嵌入。
                let mut observed = before.clone();
                reconcile_resident(storage, session_id, &mut observed, &updated_at)?;
                if observed == before {
                    return Ok((before, false));
                }
                // Re-evaluate the current admitted source inside the controller lock, never restore an old scope.
                // 在控制器锁内重新评估当前准入来源，绝不恢复旧作用域。
                let command_id = format!(
                    "native_content_commit:{}:{}",
                    storage.storage_version(),
                    before.revision
                );
                // Persist through the sole session writer, preserving intervening receipt and TTL changes.
                // 通过唯一会话写入方持久化，保留期间的回执及 TTL 变化。
                let committed =
                    controller.execute_command(&command_id, &updated_at, |candidate| {
                        reconcile_resident(storage, session_id, candidate, &updated_at)
                    })?;
                Ok((committed.state, true))
            })?;
        // Profile-only consumers need not have a recall controller, but still own retained cache rows.
        // 仅画像消费方不必拥有召回控制器，但仍拥有保留缓存行。
        let mut changed_ids = snapshots
            .iter()
            .filter_map(|(id, (_, changed))| changed.then_some(*id))
            .collect::<BTreeSet<_>>();
        // Publish cleared retained profiles back to the existing Hook mirror without changing its configuration.
        // 将已清除保留画像发布回既有 Hook 镜像，不改变其配置。
        let hooks = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.vmm_hook_runtime.clone());
        // Use one authoritative expiration boundary for all retained profile roles.
        // 对全部保留画像角色使用同一个权威到期边界。
        let now_ns = super::native_vmm_now_ns()?;
        if let Some(workspace) = guard.workspace_snapshot.as_mut() {
            for (session_id, runtime) in workspace
                .session_runtime_state
                .iter_mut()
                .chain(workspace.archived_session_runtime_state.iter_mut())
            {
                if super::profile_cache::reconcile_cached_profile(storage, runtime, now_ns)? {
                    if let Some(hooks) = &hooks {
                        hooks.store_profile_sync_state(
                            *session_id,
                            runtime.vmm_profile_sync.clone(),
                        )?;
                    }
                    changed_ids.insert(*session_id);
                }
            }
        }
        // Build only affected projections; retained and archived maps are separate declared owners.
        // 只构建受影响投影；保留及归档映射是分别声明的归属。
        let mut deltas = Vec::new();
        for session_id in changed_ids {
            // Profiles may change without a controller; absence is not permission to open a cold document.
            // 画像可以在没有控制器时改变；缺失不意味着允许打开冷文档。
            let snapshot = snapshots.get(&session_id).map(|(snapshot, _)| snapshot);
            // Workspace runtime collections are explicit alternatives, not guessed field locations.
            // 工作区运行态集合是明确的备选归属，并非猜测字段位置。
            let runtime = guard.workspace_snapshot.as_ref().and_then(|workspace| {
                workspace
                    .session_runtime_state
                    .get(&session_id)
                    .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
            });
            if let Some(runtime) = runtime {
                deltas.push(crate::workbench_session_panels::context_injection_changed_delta_from_runtime_state(
                    session_id, Some(runtime), snapshot,
                ));
                continue;
            }
            // Evicted runtime rows may retain a UI panel, whose unrelated groups must survive.
            // 已驱逐运行态行可能保留界面面板，其中无关分组必须保留。
            let conversation_id = session_id.to_string();
            // The selected mirror is authoritative for the selected ID; other IDs use their named list row.
            // 选中镜像对选中身份具有权威性；其他身份使用其命名列表行。
            let existing = if guard.workbench.selected_conversation_id.as_deref()
                == Some(conversation_id.as_str())
            {
                guard.workbench.context_injection.as_ref()
            } else {
                guard
                    .workbench
                    .context_injection_by_conversation
                    .iter()
                    .find(|entry| entry.conversation_id == conversation_id)
                    .and_then(|entry| entry.context_injection.as_ref())
            };
            if let (Some(existing), Some(snapshot)) = (existing, snapshot) {
                // Replace only the two dynamic cards produced by the authoritative VMM cache.
                // 只替换权威 VMM 缓存生产的两个动态卡片。
                let mut context_injection = existing.clone();
                replace_recall_cards(&mut context_injection, snapshot);
                deltas.push(ContextInjectionChangedDelta {
                    conversation_id,
                    context_injection: Some(context_injection),
                });
            }
        }
        // Mirror deltas before emission without replacing any unrelated workbench domain.
        // 发送前镜像增量，不替换任何无关工作台领域。
        let mut events = Vec::new();
        for delta in deltas {
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            // Each event carries the same revision already installed in the service mirror.
            // 每个事件携带已安装到服务镜像的同一修订号。
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
                Some(delta.clone()),
                |entry| entry.conversation_id.as_str(),
            );
            events.push(WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::ContextInjectionChanged(delta),
            });
        }
        Ok(events)
    }

    /// Publishes prepared deltas after service and Core storage guards have been released.
    /// 在服务及 Core 存储守卫释放后发布已准备增量。
    /// Takes owned envelopes and returns after the existing synchronous event sinks have consumed them.
    /// 接收增量封套所有权，在既有同步事件接收器消费后返回。
    pub(in crate::native_vmm) fn emit(&self, events: Vec<WorkbenchDeltaEnvelope>) {
        for event in events {
            crate::settings_runtime_projection::emit_service_event_with_trace(
                &self.events,
                "native_vmm_content_commit",
                ServiceEvent::WorkbenchDelta(Box::new(event)),
            );
        }
    }
}

/// Revokes unavailable native recalls using the candidate's own already-admitted source.
/// 使用候选自身已准入来源撤销不可用原生召回。
/// Takes current storage, exact host UUID, mutable cache and timestamp; returns explicit validation errors.
/// 接收当前存储、准确宿主 UUID、可变缓存及时间戳；返回明确校验错误。
fn reconcile_resident(
    storage: &InMemoryStorage,
    session_id: Uuid,
    candidate: &mut VmmSessionMemoryState,
    updated_at: &str,
) -> Result<(), String> {
    // Missing and external sources are declared states and are never adopted by native commits.
    // 缺失及外置来源是声明的状态，绝不由原生提交接管。
    let Some(source) = &candidate.read_source else {
        return Ok(());
    };
    // Native ownership carries the exact bound project ID; external namespaces are intentionally ignored.
    // 原生归属携带准确绑定项目 ID；刻意忽略外置命名空间。
    let VmmMemorySource::Native { project_id } = source.owner else {
        return Ok(());
    };
    source.validate().map_err(|error| error.to_string())?;
    // Reconstruct only exact canonical scopes; no asynchronous binding lookup or default owner is allowed.
    // 仅重建准确规范作用域；不允许异步绑定查询或默认归属。
    let access = ScopeAccess {
        write_scope: super::project_scope(project_id)?,
        visible_scopes: source
            .visible_project_ids
            .iter()
            .copied()
            .map(super::project_scope)
            .collect::<Result<Vec<_>, _>>()?,
    };
    // The sole native subject is shared with all production commands and profile maintenance.
    // 唯一原生主体与全部生产命令及画像维护共用。
    let subject = super::native_vmm_subject_id()?;
    // Registry ownership, not a memory record's source session, identifies the consuming session.
    // 消费会话由注册表归属标识，而非某条记忆记录的来源会话。
    let session =
        SessionSourceId::new(session_id.to_string()).map_err(|error| error.to_string())?;
    super::recall_publication::reconcile_recall(
        storage, &subject, &access, &session, candidate, None, updated_at,
    )
}

/// Refreshes only cache-produced dynamic cards, preserving profiles and all other injection groups.
/// 只刷新缓存生产的动态卡片，保留画像及全部其他注入分组。
/// Takes an existing panel and current cache; updates it in place with active-first and expired-last ordering.
/// 接收既有面板及当前缓存；按活动在前、过期在后顺序原位更新。
fn replace_recall_cards(panel: &mut ContextInjectionSnapshot, snapshot: &VmmSessionMemoryState) {
    panel
        .dynamic_cards
        .retain(|card| !matches!(card.tag.as_str(), "VMM_MEMORY" | "VMM_MEMORY_EXPIRED"));
    // Reuse the canonical card formatter rather than fabricating a second content schema.
    // 复用规范卡片格式器，不伪造第二套内容结构。
    let mut active = crate::workbench_session_panels::context_injection_cards_to_summary(
        snapshot.active_panel_card().into_iter().collect(),
    );
    active.append(&mut panel.dynamic_cards);
    active.extend(
        crate::workbench_session_panels::context_injection_cards_to_summary(
            snapshot.expired_panel_card().into_iter().collect(),
        ),
    );
    panel.dynamic_cards = active;
}
