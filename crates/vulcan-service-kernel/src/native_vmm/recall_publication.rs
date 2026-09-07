//! Revalidates native recall at the durable host-cache publication boundary.
//! 在耐久宿主缓存发布边界重新校验原生召回。

use super::*;
use vulcan_agent_runtime::agent_runtime::conversation_hooks::{
    ConversationPrecheckOutcome, HOOK_METADATA_VMM_RECALL_BATCH_EFFECT,
    HOOK_METADATA_VMM_SESSION_STATUS_EFFECT, VmmSessionStatusHookEffect,
    vmm_recall_batch_effect_from_report, vmm_session_status_effect_from_report,
};
use vulcan_memorymesh_core::{InMemoryStorage, StorageSessionStatus};
use vulcan_vmm::{SessionMemoryStatus, VmmSessionMemoryState};

/// Holds the native mutation barrier until the host finishes publishing its exact session command.
/// 持有原生变更屏障，直至宿主完成发布准确会话命令。
pub(crate) struct NativeVmmRecallPublication {
    /// No native write, rebind, recycle or restore can cross this publication.
    /// 原生写入、重绑、回收或恢复均不能跨越此次发布。
    _operation_guard: tokio::sync::OwnedRwLockWriteGuard<()>,
    /// Core workers may outlive cancelled request guards; freeze their actual durable commit gate as well.
    /// Core 工作器可能长于已取消请求守卫存活；同时冻结其实际持久提交门。
    _content_guard: tokio::sync::OwnedMutexGuard<()>,
    /// Shared authoritative Core storage reader, not a copied full vector snapshot.
    /// 共享权威 Core 存储读取方，而非复制的完整向量快照。
    runtime: Arc<NativeVmmCoreRuntime>,
    /// Fixed native subject resolved from the established host identity.
    /// 从既有宿主身份解析的固定原生主体。
    subject: SubjectId,
    /// Current owner and explicitly linked readable project scopes.
    /// 当前归属及明确连带可读项目作用域。
    access: ScopeAccess,
    /// Exact source session; display titles are never used.
    /// 准确来源会话；绝不使用显示标题。
    session: SessionSourceId,
}

impl NativeVmmMaintenance {
    /// Resolves current native access under a short post-inference publication barrier.
    /// 在推理后的短时发布屏障内解析当前原生访问权。
    /// Takes the optional host project and exact session; projectless calls use protected project 1.
    /// 接收可选宿主项目及准确会话；无项目调用使用受保护项目 1。
    pub(crate) async fn prepare_recall_publication(
        &self,
        host_project_id: Option<&str>,
        session_id: &str,
    ) -> Result<NativeVmmRecallPublication, String> {
        // Acquire only after remote/AI preparation, never while waiting for a model response.
        // 只在远端或 AI 准备完成后获取，绝不在等待模型响应时持有。
        let operation_guard = self.operation_gate.clone().write_owned().await;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err("原生记忆已关闭，不能发布此前的召回结果。".to_owned());
        }
        // Operation admission alone cannot stop a detached Core worker finishing after caller cancellation.
        // 仅靠操作准入不能阻止脱离的 Core 工作器在调用方取消后完成。
        let content_guard = self.runtime.freeze_committed_content().await;
        // The optional host identity has exactly the same projectless rule as native request admission.
        // 可选宿主身份与原生请求准入使用完全相同的无项目规则。
        let access = match host_project_id {
            Some(id) => {
                // Resolve the persisted binding, including its explicit linked projects.
                // 解析持久绑定，包含其明确连带项目。
                let binding = self.data_plane.resolve_project_access(id).await?;
                // The binding's scope, not the shared request envelope, owns native writes.
                // 绑定作用域而非共享请求封套决定原生写入归属。
                let write_scope =
                    ScopeId::new(binding.owner.scope_id).map_err(|error| error.to_string())?;
                // Preserve the owner-first declared scope order.
                // 保留声明的归属优先作用域顺序。
                let mut visible_scopes = vec![write_scope.clone()];
                for linked in binding.linked {
                    visible_scopes
                        .push(ScopeId::new(linked.scope_id).map_err(|error| error.to_string())?);
                }
                ScopeAccess {
                    write_scope,
                    visible_scopes,
                }
            }
            None => {
                // VmmEffectiveConfig freezes native projectless envelope identity to protected ID 1.
                // VmmEffectiveConfig 将原生无项目封套身份冻结为受保护 ID 1。
                let scope = project_scope(1)?;
                ScopeAccess {
                    write_scope: scope.clone(),
                    visible_scopes: vec![scope],
                }
            }
        };
        access.validate().map_err(|error| error.to_string())?;
        Ok(NativeVmmRecallPublication {
            _operation_guard: operation_guard,
            _content_guard: content_guard,
            runtime: self.runtime.clone(),
            subject: native_vmm_subject_id()?,
            access,
            session: SessionSourceId::new(session_id).map_err(|error| error.to_string())?,
        })
    }
}

impl NativeVmmRecallPublication {
    /// Reconciles durable cached visibility with or without an actual incoming PreCheck result.
    /// 无论是否有真实传入 PreCheck 结果，都对账持久缓存可见性。
    /// Takes the sole controller, an optional real outcome and timestamp; returns current persisted state.
    /// 接收唯一控制器、可选真实结果及时间戳；返回当前持久状态。
    /// A missing outcome is a cache-only boundary, never a fabricated Hook success or failure.
    /// 无结果表示纯缓存边界，绝不伪造 Hook 成功或失败。
    pub(crate) fn reconcile_controller_cache(
        &self,
        controller: &crate::vmm_session_controller::SessionVmmController,
        mut outcome: Option<&mut ConversationPrecheckOutcome>,
        updated_at: &str,
    ) -> Result<VmmSessionMemoryState, String> {
        // Observe the live controller instead of using an earlier detached receipt snapshot.
        // 观察活动控制器，而非使用早先脱离的回执快照。
        let before = controller.snapshot()?;
        // Do not generate a durable command for an unchanged read-only check.
        // 不为无变化的只读检查生成持久命令。
        let mut observed = before.clone();
        self.reconcile_state(&mut observed, outcome.as_deref_mut(), updated_at)?;
        if observed == before {
            return Ok(before);
        }
        // Retain the established command namespace and revision-based idempotency lineage.
        // 保留既有命令命名空间及基于修订号的幂等沿革。
        let command_id = format!("native_recall_reconcile:{}", before.revision);
        // Recompute under the controller lock instead of overwriting intervening TTL or receipt changes.
        // 在控制器锁内重新计算，避免覆盖期间的 TTL 或回执变化。
        let committed = controller.execute_command(&command_id, updated_at, |candidate| {
            self.reconcile_state(candidate, outcome, updated_at)
        })?;
        Ok(committed.state)
    }

    /// Uses the publication barrier's exact storage view for a candidate and optional real Hook output.
    /// 使用发布屏障的准确存储视图处理候选及可选真实 Hook 输出。
    /// Returns an explicit error on ownership or contract mismatch; no model or vector recomputation occurs.
    /// 归属或契约不匹配时返回明确错误；不调用模型或重新计算向量。
    fn reconcile_state(
        &self,
        state: &mut VmmSessionMemoryState,
        outcome: Option<&mut ConversationPrecheckOutcome>,
        updated_at: &str,
    ) -> Result<(), String> {
        self.runtime.with_recall_publication_storage(|storage| {
            reconcile_recall(
                storage,
                &self.subject,
                &self.access,
                &self.session,
                state,
                outcome,
                updated_at,
            )
        })
    }

    /// Returns the exact profile source frozen by this publication's native mutation barrier.
    /// 返回由本次发布的原生变更屏障冻结的准确画像来源。
    /// Returns validated owner-first visibility without a second mutable binding lookup.
    /// 返回已校验且归属在首的可见范围，不再次查询可变绑定。
    pub(crate) fn profile_read_source(&self) -> Result<vulcan_vmm::VmmMemoryReadSource, String> {
        crate::native_vmm::client::memory_read_source_from_access(&self.access)
            .map_err(|error| error.to_string())
    }

    /// Rejects stale native profile nodes before either cached or delayed profile content enters a request.
    /// 在缓存或迟到画像内容进入请求之前拒绝过时原生画像节点。
    /// Takes the retained candidate and real profile outcome; returns whether the Hook cache mirror needs repair.
    /// 接收保留候选及真实画像结果；返回 Hook 缓存镜像是否需要修复。
    pub(crate) fn reconcile_profile(
        &self,
        runtime: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
        outcome: &mut vulcan_agent_runtime::agent_runtime::conversation_hooks::ConversationProfileOutcome,
    ) -> Result<bool, String> {
        if outcome.session_id.to_string() != self.session.as_str() {
            return Err("Native profile publication session identity mismatch.".to_owned());
        }
        self.runtime.with_recall_publication_storage(|storage| {
            // One current time covers the resident cache and the result prepared before this barrier.
            // 一个当前时间覆盖驻留缓存及此屏障之前准备的结果。
            let now_ns = native_vmm_now_ns()?;
            // The detached cache can be stale even if preparation failed or returned newer nodes.
            // 即使准备失败或返回较新节点，脱离缓存仍可能过时。
            let changed = super::profile_cache::reconcile_cached_profile(storage, runtime, now_ns)?;
            // Failed dispatches contain no successful report, but still require fallback cache validation.
            // 失败派发不包含成功报告，但仍需校验复用缓存。
            let Ok(report) = outcome.report.as_mut() else { return Ok(changed) };
            // Typed metadata is the sole producer of cached node identities; malformed effects remain errors.
            // 类型化元数据是缓存节点身份的唯一生产方；畸形效果保持为错误。
            let effect = vulcan_agent_runtime::agent_runtime::conversation_hooks::vmm_profile_sync_effect_from_report(report)?;
            if let Some(effect) = effect {
                if super::profile_cache::profile_cache_is_stale(storage, &effect.state, now_ns)? {
                    vulcan_agent_runtime::agent_runtime::vmm_hooks::clear_vmm_profile_report(report);
                    return Ok(true);
                }
                if effect.state.read_source.as_ref() == Some(&self.profile_read_source()?) {
                    // A fresh successful Hook already owns the correct mirror; do not overwrite it with a cleared old candidate.
                    // 新鲜成功 Hook 已拥有正确镜像；不能用已清空旧候选覆盖它。
                    return Ok(false);
                }
            }
            Ok(changed)
        })
    }
    /// Persists a changed recall reconciliation even when the provider boundary already has a receipt.
    /// 即使 Provider 边界已有回执，也持久化发生变化的召回对账。
    /// Takes the sole session controller, delayed result and timestamp; returns the durable current state.
    /// 接收唯一会话控制器、迟到结果及时间戳；返回耐久当前状态。
    pub(crate) fn reconcile_replayed_boundary(
        &self,
        controller: &crate::vmm_session_controller::SessionVmmController,
        outcome: &mut ConversationPrecheckOutcome,
        updated_at: &str,
    ) -> Result<VmmSessionMemoryState, String> {
        self.reconcile_controller_cache(controller, Some(outcome), updated_at)
    }

    /// Reconciles both resident leases and incoming typed recall before the host builds a request.
    /// 在宿主构建请求前，同时对账驻留租约及传入类型化召回。
    /// Takes the isolated controller candidate and precheck outcome; unrelated hook outputs remain intact.
    /// 接收隔离控制器候选及预检查结果；无关 Hook 输出保持不变。
    pub(crate) fn reconcile(
        &self,
        state: &mut VmmSessionMemoryState,
        outcome: &mut ConversationPrecheckOutcome,
        updated_at: &str,
    ) -> Result<(), String> {
        self.runtime.with_recall_publication_storage(|storage| {
            reconcile_recall(
                storage,
                &self.subject,
                &self.access,
                &self.session,
                state,
                Some(outcome),
                updated_at,
            )
        })
    }
}

/// Rejects stale native sources without repeating ranking or extending their existing TTL.
/// 拒绝过期原生来源，不重复排序或延长已有 TTL。
/// Takes read-only storage, exact access and host candidates; modifies only VMM leases and typed metadata.
/// 接收只读存储、准确访问权及宿主候选；只修改 VMM 租约及类型化元数据。
/// `outcome` is absent at cache-only boundaries; present outcomes retain strict typed validation.
/// 纯缓存边界没有 `outcome`；存在结果时仍严格校验其类型化内容。
pub(super) fn reconcile_recall(
    storage: &InMemoryStorage,
    subject: &SubjectId,
    access: &ScopeAccess,
    session: &SessionSourceId,
    state: &mut VmmSessionMemoryState,
    outcome: Option<&mut ConversationPrecheckOutcome>,
    updated_at: &str,
) -> Result<(), String> {
    // Source session ownership is independently checked from the host UUID correlation.
    // 来源会话归属独立于宿主 UUID 关联接受检查。
    if state.session_id != session.as_str()
        || outcome
            .as_ref()
            .is_some_and(|outcome| outcome.session_id.to_string() != session.as_str())
    {
        return Err("Native VMM recall publication session identity mismatch.".to_owned());
    }
    // Absence is the existing native not-yet-registered rule, not inferred forgotten state.
    // 缺失遵循既有原生尚未登记规则，而非推断的遗忘状态。
    let status = match storage
        .memory_session(subject, &access.write_scope, session)
        .map(|record| record.status)
    {
        None | Some(StorageSessionStatus::Active) => SessionMemoryStatus::Active,
        Some(StorageSessionStatus::Archived) => SessionMemoryStatus::Archived,
        Some(StorageSessionStatus::Recycled) => SessionMemoryStatus::Recycled,
        Some(StorageSessionStatus::Forgotten) => SessionMemoryStatus::Forgotten,
    };
    // Parse typed hook metadata before mutating the isolated state, preserving explicit contract errors.
    // 在修改隔离状态前解析类型化 Hook 元数据，保留明确契约错误。
    let mut recall = match outcome.as_ref().map(|outcome| &outcome.report) {
        Some(Ok(report)) => {
            vmm_session_status_effect_from_report(report)?;
            vmm_recall_batch_effect_from_report(report)?
        }
        Some(Err(_)) | None => None,
    };
    // Only the same admitted source may contribute IDs; equal numeric IDs across sources are unrelated.
    // 只有同一准入来源可以提供身份集合；不同来源的相同数字 ID 没有关联。
    let read_source = crate::native_vmm::client::memory_read_source_from_access(access)
        .map_err(|error| error.to_string())?;
    state.select_read_source(session.as_str(), Some(&read_source), updated_at)?;
    if let Some(effect) = &recall {
        effect
            .read_source
            .validate()
            .map_err(|error| error.to_string())?;
        if effect.read_source != read_source {
            recall = None;
        }
    }
    // One business time controls expiration checks for both resident and incoming memories.
    // 同一业务时间控制驻留及传入记忆的到期检查。
    let now_ns = native_vmm_now_ns()?;
    // Native PreCheck always returns a positive memory ID; anonymous and turn-only leases are not native records.
    // 原生 PreCheck 始终返回正记忆 ID；匿名及仅回合租约不是原生记录。
    let requested_ids = state
        .active_leases
        .values()
        .map(|lease| lease.item.memory_id)
        .chain(
            recall
                .iter()
                .flat_map(|effect| effect.batch.items.iter().map(|item| item.memory_id)),
        )
        .collect::<BTreeSet<_>>();
    // Resolve each requested identity once; construction errors are explicit rather than treated as absence.
    // 每个请求身份只解析一次；构造错误明确返回，而非视为缺失。
    let mut visible_ids = BTreeSet::new();
    for id in requested_ids {
        if id == 0 || !status.allows_runtime_memory() {
            continue;
        }
        // The native numeric envelope is an exact reversible canonical mapping.
        // 原生数字封套是准确可逆的规范映射。
        let memory_id =
            MemoryId::new(format!("vmm-memory-{id}")).map_err(|error| error.to_string())?;
        if storage.memory_record(&memory_id).is_some_and(|record| {
            record.state == MemoryState::Active
                && record.is_visible_to_subject(subject)
                && access.visible_scopes.contains(&record.scope_id)
                && record.expires_at_ns.is_none_or(|expiry| expiry > now_ns)
        }) {
            visible_ids.insert(id);
        }
    }
    // Gather only bounded resident keys; do not clone unrelated records or any embedding.
    // 只收集有界驻留键；不复制无关记录或任何嵌入。
    let revoked = state
        .active_leases
        .iter()
        .filter(|(_, lease)| !visible_ids.contains(&lease.item.memory_id))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    state.apply_authoritative_session_status(session.as_str(), status, updated_at)?;
    for key in revoked {
        state.revoke_lease(&key, updated_at)?;
    }
    if let Some(effect) = &mut recall {
        effect
            .batch
            .items
            .retain(|item| visible_ids.contains(&item.memory_id));
    }
    if let Some(Ok(report)) = outcome.map(|outcome| &mut outcome.report) {
        report.metadata.insert(
            HOOK_METADATA_VMM_SESSION_STATUS_EFFECT.to_owned(),
            vec![
                serde_json::to_value(VmmSessionStatusHookEffect {
                    read_source,
                    status,
                })
                .map_err(|error| error.to_string())?,
            ],
        );
        if let Some(effect) = recall.filter(|effect| !effect.batch.items.is_empty()) {
            report.metadata.insert(
                HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_owned(),
                vec![serde_json::to_value(effect).map_err(|error| error.to_string())?],
            );
        } else {
            // A paused or now-empty recall must not trigger the shared runtime's recall admission at all.
            // 暂停或已为空的召回不能触发共享运行时的召回准入。
            report
                .metadata
                .remove(HOOK_METADATA_VMM_RECALL_BATCH_EFFECT);
        }
    }
    Ok(())
}
