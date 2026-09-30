//! Original update ownership follows predecessor Lua resources through real retirement.
//! 原更新所有权跟随前驱 Lua 资源直至实际退役。

use super::{VulcanPluginManagerState, VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use crate::{
    VulcanPluginActivationCandidate, VulcanPluginActivationTransactionRecord,
    VulcanPluginCatalogSnapshot, VulcanPluginDescriptor, VulcanPluginDrainReport,
    VulcanPluginPackageOrigin, VulcanPluginPredecessorDrainIdentity, VulcanPluginRuntimeState,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

/// Validate frozen plugin and identity against the same manager's still-draining old catalog.
/// 按相同管理器仍处于排空中的旧目录校验固定 plugin 及 identity。
/// No publication or capability widening occurs; returns only an exact original callback grant.
/// 不执行发布或能力扩权；仅返回精确原回调授权。
pub(super) fn validate_predecessor_callback_current(
    state: &VulcanPluginManagerState,
    plugin: &VulcanPluginDescriptor,
    identity: &VulcanPluginPredecessorDrainIdentity,
) -> Result<(), VulcanPluginRuntimeError> {
    // Current routing stays closed while the frozen old descriptor supplies its configuration and credentials.
    // 固定旧描述符提供配置及凭据时，当前路由保持关闭。
    let snapshot = state
        .snapshot
        .read()
        .map_err(|_| error("Plugin catalog lock is poisoned."))?;
    // The old package must remain selected through its explicit disable callback.
    // 旧包必须在显式停用回调期间保持选中。
    let current = snapshot
        .plugins
        .get(&plugin.id)
        .ok_or_else(|| error("Original predecessor is no longer installed."))?;
    if identity.service_epoch != state.manager_id
        || plugin.origin != VulcanPluginPackageOrigin::UserInstalled
        || current.origin != plugin.origin
        || plugin.generation != identity.predecessor_generation
        || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
        || current.generation != identity.draining_generation
        || current.runtime_state != VulcanPluginRuntimeState::Draining
        || plugin.package_root != current.package_root
        || plugin.version != current.version
        || plugin.version.as_deref() != Some(identity.version.as_str())
        || plugin.content_fingerprint != identity.content_fingerprint
        || current.content_fingerprint != identity.content_fingerprint
    {
        return Err(error("Original predecessor callback authority changed."));
    }
    Ok(())
}

/// Exclude ordinary lifecycle admission while an original request remains unfinished.
/// 原请求未完成时排除普通生命周期入场。
/// state and plugin_id select the actual store; scope separates old business from a selected unfinished target.
/// state 与 plugin_id 选择实际存储；scope 区分旧业务与已选中但未完成的目标。
/// Returns a precise conflict for pending lifecycle effects or target business before full handoff.
/// 对待完成生命周期副作用或完整交接前的目标业务返回精确冲突。
pub(super) fn validate_ordinary_lifecycle_admission(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
    scope: crate::VulcanPluginBridgeLeaseScope,
) -> Result<(), VulcanPluginRuntimeError> {
    // Current disk selection, not discovery state alone, fences a committed target awaiting full handoff.
    // 当前磁盘选择而非仅发现状态，隔离等待完整交接的已提交目标。
    let Some(store) = crate::VulcanPluginGenerationStore::open_existing(&state.user_plugin_root)
        .map_err(|failure| error(failure.to_string()))?
    else {
        return Ok(());
    };
    // Inspect all original obligations without choosing a latest request or allocating execution authority.
    // 检查全部原义务，不选择最新请求，也不分配执行权威。
    let pending = store
        .activation_recovery_records(plugin_id)
        .map_err(|failure| error(failure.to_string()))?;
    if scope == crate::VulcanPluginBridgeLeaseScope::Enabled {
        // Preparation may leave the exact old selection serving business until explicit predecessor closure.
        // 准备可使精确旧选择继续提供业务，直到明确关闭前驱。
        let selected = store
            .activation(plugin_id)
            .map_err(|failure| error(failure.to_string()))?;
        if pending.iter().any(|record| {
            matches!(record, crate::VulcanPluginActivationRequestRecord::Activation(original)
                if selected.as_ref() == Some(original.target()))
        }) {
            return Err(error(
                "Selected activation target is awaiting its original route handoff.",
            ));
        }
        return Ok(());
    }
    if !pending.is_empty() {
        return Err(error(
            "Plugin has an unfinished original activation request.",
        ));
    }
    Ok(())
}

/// Opaque original-request handle; obtaining it grants no candidate publication authority.
/// 不透明原请求句柄；取得它不授予候选发布权限。
#[derive(Clone)]
pub struct VulcanPluginPredecessorDrain {
    /// Exact manager-issued owner shared with physical core domains.
    /// 与物理核心域共享的精确管理器签发所有者。
    authority: Arc<UpdateDrainAuthority>,
}

impl std::fmt::Debug for VulcanPluginPredecessorDrain {
    /// Format only non-secret identity for formatter; return its formatting result.
    /// 为 formatter 仅格式化无秘密身份；返回格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginPredecessorDrain")
            .field("request_id", &self.authority.original.request_id())
            .field("identity", &self.authority.identity)
            .finish()
    }
}

impl VulcanPluginPredecessorDrain {
    /// Return the original package for candidate only after actual and durable Lua retirement agree.
    /// 仅在实际及持久 Lua 退役一致后，为 candidate 返回原包。
    /// state must own this handle; mismatched requests, epochs or live ownership are explicit errors.
    /// state 必须拥有此句柄；请求、实例不匹配或所有权仍存活均为显式错误。
    pub(super) fn predecessor_for_callback(
        &self,
        state: &VulcanPluginManagerState,
        candidate: &VulcanPluginActivationCandidate<'_>,
    ) -> Result<VulcanPluginDescriptor, VulcanPluginRuntimeError> {
        if self.authority.identity.service_epoch != state.manager_id
            || &self.authority.original != candidate.transaction_record()
            || self
                .authority
                .attempt
                .lock()
                .map_err(|_| error("Predecessor drain observation lock is poisoned."))?
                .is_some()
        {
            return Err(error(
                "Predecessor callback requires its own completed original Lua drain.",
            ));
        }
        // Read-only disk evidence is additional proof; it never recreates the retired in-memory owner.
        // 只读磁盘证据为额外证明；它绝不重建已退役内存所有者。
        let store = crate::VulcanPluginGenerationStore::open_existing(&state.user_plugin_root)
            .map_err(|failure| error(failure.to_string()))?
            .ok_or_else(|| error("Original update store is missing."))?;
        // The original identity and successful resource checkpoint must remain unchanged.
        // 原身份及成功资源检查点必须保持未变。
        let effects = store
            .activation_effects(&self.authority.predecessor.id, self.request_id())
            .map_err(|failure| error(failure.to_string()))?
            .ok_or_else(|| error("Original predecessor drain evidence is missing."))?;
        if effects.predecessor_drain_identity() != Some(self.identity())
            || !effects.observations().contains(&(
                crate::VulcanPluginActivationEffect::DrainPredecessorLua,
                crate::VulcanPluginEffectObservation::Succeeded,
            ))
        {
            return Err(error("Predecessor Lua retirement is not confirmed."));
        }
        validate_predecessor_callback_current(state, &self.authority.predecessor, self.identity())?;
        Ok(self.authority.predecessor.clone())
    }

    /// Return the original request UUID retained by this handle.
    /// 返回此句柄保留的原请求 UUID。
    pub fn request_id(&self) -> Uuid {
        self.authority.original.request_id()
    }

    /// Borrow the frozen predecessor and host incarnation identity.
    /// 借用固定的前驱与宿主实例身份。
    pub fn identity(&self) -> &VulcanPluginPredecessorDrainIdentity {
        &self.authority.identity
    }
}

/// No strong manager reference is allowed here: native ownership must survive manager shutdown without a cycle.
/// 此处不允许强管理器引用：原生所有权必须跨管理器关闭存活而不形成循环。
pub(super) struct UpdateDrainAuthority {
    /// Original descriptor retains its physical package until the drain owner disappears.
    /// 原描述符保留物理包，直至排空所有者消失。
    predecessor: VulcanPluginDescriptor,
    /// Full original request selection, independent from mutable public catalog refreshes.
    /// 完整原请求选择，独立于可变公开目录刷新。
    original: VulcanPluginActivationTransactionRecord,
    /// Exact host incarnation and old/new routing generations.
    /// 精确宿主实例及新旧路由代次。
    identity: VulcanPluginPredecessorDrainIdentity,
    /// Sole durable observer also retains the actual original file lock.
    /// 唯一持久观察者同时保留实际原文件锁。
    attempt: Mutex<Option<crate::generation_store::EffectAttempt>>,
    /// Serializes cleanup observations without retaining a synchronous lock across await.
    /// 串行清理观测，不跨 await 保留同步锁。
    gate: AsyncMutex<()>,
    /// A failed earlier observation cannot become successful merely because a later poll is quiet.
    /// 先前失败的观测不能仅因后续轮询安静而变成成功。
    failed: AtomicBool,
}

impl UpdateDrainAuthority {
    /// Persist Started for candidate and predecessor at successor; return the manager-retained owner.
    /// 为 successor 下的 candidate 与 predecessor 持久保存 Started；返回管理器保留所有者。
    /// The caller holds registration and generation allocation; no Lua work starts here.
    /// 调用方持有注册与代次分配锁；此处不开始 Lua 工作。
    pub(super) fn begin(
        state: &VulcanPluginManagerState,
        candidate: &VulcanPluginActivationCandidate<'_>,
        predecessor: &VulcanPluginDescriptor,
        successor: u64,
    ) -> Result<Arc<Self>, VulcanPluginRuntimeError> {
        // Use the exact configured store and unpublished candidate, never a path inferred from plugin ID.
        // 使用精确配置存储及未发布候选，绝不从插件 ID 推断路径。
        let _readiness = super::candidates::retain_candidate_readiness(state, candidate)?;
        // Registry ownership starts before durable intent, so duplicate attempts cannot race.
        // 注册表所有权先于持久意图开始，避免重复尝试竞争。
        let mut owners = state
            .update_drains
            .lock()
            .map_err(|_| error("Predecessor drain registry is poisoned."))?;
        if owners.contains_key(&predecessor.id) {
            return Err(error("Another original request owns predecessor drain."));
        }
        // Validated catalog version is required; absence is an error rather than a fabricated identity.
        // 必须有已校验目录版本；缺失是错误，不生成虚构身份。
        let identity = VulcanPluginPredecessorDrainIdentity {
            service_epoch: state.manager_id.clone(),
            predecessor_generation: predecessor.generation,
            draining_generation: successor,
            version: predecessor
                .version
                .clone()
                .ok_or_else(|| error("Predecessor version is missing."))?,
            content_fingerprint: predecessor.content_fingerprint.clone(),
        };
        // The attempt takes the original lock before the local readiness reference is released.
        // 在局部就绪引用释放前，尝试取得原锁。
        let attempt = candidate
            .begin_predecessor_drain_attempt(predecessor, identity.clone())
            .map_err(|failure| error(failure.to_string()))?;
        // Manager and actual core domains share this owner without retaining each other cyclically.
        // 管理器及实际核心域共享此所有者，不形成相互保留循环。
        let authority = Arc::new(Self {
            predecessor: predecessor.clone(),
            original: candidate.transaction_record().clone(),
            identity,
            attempt: Mutex::new(Some(attempt)),
            gate: AsyncMutex::new(()),
            failed: AtomicBool::new(false),
        });
        owners.insert(predecessor.id.clone(), Arc::clone(&authority));
        Ok(authority)
    }
}

/// Verify expected belongs to state and still denotes current; candidate supplies the complete managed selection.
/// 验证 expected 属于 state 且仍指向 current；candidate 提供完整受管选择。
/// Return a conflict before persistent evidence or lifecycle generation changes.
/// 在持久证据或生命周期代次变化之前返回冲突。
pub(super) fn validate_predecessor(
    state: &VulcanPluginManagerState,
    candidate: &VulcanPluginActivationCandidate<'_>,
    expected: &VulcanPluginCatalogSnapshot,
    current: &VulcanPluginDescriptor,
) -> Result<(), VulcanPluginRuntimeError> {
    // Frozen snapshots bind manager incarnation as well as package and routing identity.
    // 固定快照绑定管理器实例以及插件包与路由身份。
    let original = expected
        .plugins
        .get(&candidate.package().id)
        .ok_or_else(|| error("Original predecessor was not discovered."))?;
    if expected.service_epoch != state.manager_id
        || original.id != current.id
        || original.origin != VulcanPluginPackageOrigin::UserInstalled
        || current.origin != original.origin
        || original.runtime_state != VulcanPluginRuntimeState::Enabled
        || current.runtime_state != original.runtime_state
        || original.generation != current.generation
        || original.package_root != current.package_root
        || original.version != current.version
        || original.content_fingerprint != current.content_fingerprint
        || original.desired_enabled != current.desired_enabled
    {
        return Err(error("Original predecessor runtime identity changed."));
    }
    // Both frozen and current descriptors must own the full journal predecessor, not merely equal bytes.
    // 固定及当前描述符均须拥有完整日志前驱，而非仅有相同字节。
    for plugin in [original, current] {
        if !match (
            plugin.managed_package(),
            candidate.transaction_record().predecessor(),
        ) {
            (Some(package), Some(selected)) => package.matches_activation(selected),
            (None, None) => true,
            _ => false,
        } {
            return Err(error("Original predecessor selection changed."));
        }
    }
    Ok(())
}

impl VulcanPluginRuntimeManager {
    /// Close old Lua admission for candidate using expected's exact predecessor; return its opaque drain owner.
    /// 使用 expected 的精确前驱为 candidate 关闭旧 Lua 入场；返回其不透明排空所有者。
    /// Candidate stays private; MCP retirement and old disable callbacks remain separate coordinator obligations.
    /// 候选保持私有；MCP 退役及旧停用回调仍为协调器的独立义务。
    pub fn begin_activation_predecessor_drain(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
        expected: &VulcanPluginCatalogSnapshot,
    ) -> Result<VulcanPluginPredecessorDrain, VulcanPluginRuntimeError> {
        // The normal publication gate has one exact original-owner exception.
        // 普通发布门仅提供一个精确原所有者例外。
        let authority = self
            .begin_lifecycle_transition_with_update(
                &candidate.package().id,
                VulcanPluginRuntimeState::Draining,
                &[VulcanPluginRuntimeState::Enabled],
                Some((candidate, expected)),
            )?
            .ok_or_else(|| error("Predecessor transition did not retain its original owner."))?;
        Ok(VulcanPluginPredecessorDrain { authority })
    }

    /// Recover the live handle for request_id in this manager; return None after actual completion.
    /// 在此管理器内恢复 request_id 的活动句柄；实际完成后返回 None。
    /// This only observes the retained original and cannot recreate or replay a started drain.
    /// 此操作仅观察已保留原请求，不能重建或重放已开始排空。
    pub fn activation_predecessor_drain(
        &self,
        request_id: Uuid,
    ) -> Result<Option<VulcanPluginPredecessorDrain>, VulcanPluginRuntimeError> {
        Ok(self
            .state
            .update_drains
            .lock()
            .map_err(|_| error("Predecessor drain registry is poisoned."))?
            .values()
            .find(|owner| owner.original.request_id() == request_id)
            .map(|owner| VulcanPluginPredecessorDrain {
                authority: Arc::clone(owner),
            }))
    }

    /// Retire drain's actual predecessor resources; caller cancellation does not cancel the owned supervisor.
    /// 退役 drain 的实际前驱资源；调用方取消不会取消自有监督任务。
    /// Return real cleanup counts and failures; unfinished resources keep the original request busy.
    /// 返回真实清理计数及失败；未完成资源使原请求保持忙碌。
    pub async fn drain_activation_predecessor(
        &self,
        drain: &VulcanPluginPredecessorDrain,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        // Own both the supervisor and original authority across asynchronous caller cancellation.
        // 跨异步调用方取消拥有监督器与原权威。
        let manager = self.clone();
        // Cloning this opaque handle does not create a new durable attempt.
        // 克隆此不透明句柄不创建新的持久尝试。
        let authority = Arc::clone(&drain.authority);
        tokio::spawn(async move { manager.drain_activation_predecessor_owned(authority).await })
            .await
            .map_err(|failure| error(format!("Predecessor drain supervisor failed: {failure}")))?
    }

    /// Observe one serialized cleanup for authority and checkpoint only after tracked and native owners retire.
    /// 为 authority 观察一次串行清理，仅在受跟踪及原生所有者退役后保存检查点。
    /// Return the report; errors retain the registry and request lock for explicit original-request follow-up.
    /// 返回报告；错误保留注册表与请求锁，供显式原请求后续处理。
    async fn drain_activation_predecessor_owned(
        &self,
        authority: Arc<UpdateDrainAuthority>,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        // Only one observer may advance the original receipt.
        // 仅一个观察者可以推进原回执。
        let _observer = authority.gate.lock().await;
        // Resolve the actual original namespace rather than any caller-provided plugin ID.
        // 解析实际原命名空间，而非任何调用方提供的插件 ID。
        let plugin_id = &authority.predecessor.id;
        {
            // Exact in-memory ownership rejects handles from another manager or completed attempt.
            // 精确内存所有权拒绝来自另一管理器或已完成尝试的句柄。
            let owners = self
                .state
                .update_drains
                .lock()
                .map_err(|_| error("Predecessor drain registry is poisoned."))?;
            if authority.identity.service_epoch != self.state.manager_id
                || !owners
                    .get(plugin_id)
                    .is_some_and(|owner| Arc::ptr_eq(owner, &authority))
            {
                return Err(error("Predecessor drain is not owned by this manager."));
            }
        }
        // Existing cleanup owns operation handles and does not repeat completed finalizers on polling.
        // 既有清理拥有操作句柄，轮询时不会重复已完成的关闭函数。
        let mut report = match self.drain_plugin_sessions_owned(plugin_id).await {
            Ok(report) => report,
            Err(failure) => {
                authority.failed.store(true, Ordering::Release);
                return Err(failure);
            }
        };
        if !report.failures.is_empty() {
            authority.failed.store(true, Ordering::Release);
        }
        // Final proof shares registration exclusion with the initial admission closure.
        // 最终证明与初始入场关闭共享注册互斥。
        let registration = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| error("Plugin registration gate is poisoned."))?;
        // Physical authority lookup failures are explicit and cannot be mistaken for a completed drain.
        // 物理权威查询失败显式呈现，不得误认为已完成排空。
        let native_retained = match self.formal_runtime_owner()? {
            Some(owner) => owner
                .host
                .retains_predecessor_drain(&authority)
                .map_err(|failure| error(failure.to_string()))?,
            None => false,
        };
        if self.has_unretired_plugin_resources_locked(plugin_id, &registration)? || native_retained
        {
            report
                .failures
                .push("Original predecessor Lua resources are still retiring.".to_owned());
            return Ok(report);
        }
        // A changed public predecessor cannot be certified as a successful original transition.
        // 已变化的公开前驱不能被认证为成功的原转换。
        let snapshot = self.snapshot();
        if !snapshot.plugins.get(plugin_id).is_some_and(|plugin| {
            plugin.runtime_state == VulcanPluginRuntimeState::Draining
                && plugin.generation == authority.identity.draining_generation
                && plugin.package_root == authority.predecessor.package_root
                && plugin.content_fingerprint == authority.identity.content_fingerprint
        }) {
            authority.failed.store(true, Ordering::Release);
            report
                .failures
                .push("Predecessor runtime identity changed during drain.".to_owned());
        }
        // Keep the observer in place if persistence fails, including an ambiguous atomic-write result.
        // 持久化失败时保留观察者，包括不明确的原子写入结果。
        let mut attempt = authority
            .attempt
            .lock()
            .map_err(|_| error("Predecessor drain observation lock is poisoned."))?;
        attempt
            .as_ref()
            .ok_or_else(|| error("Predecessor drain already completed."))?
            .record_observation(!authority.failed.load(Ordering::Acquire))
            .map_err(|failure| error(failure.to_string()))?;
        attempt.take();
        self.state
            .update_drains
            .lock()
            .map_err(|_| error("Predecessor drain registry is poisoned."))?
            .remove(plugin_id);
        if authority.failed.load(Ordering::Acquire) && report.failures.is_empty() {
            report.failures.push(
                "Original predecessor drain retained an earlier failed observation.".to_owned(),
            );
        }
        Ok(report)
    }
}

/// Build an explicit runtime error from message without fallback or hidden recovery.
/// 从 message 构造显式运行时错误，不执行兜底或隐藏恢复。
fn error(message: impl Into<String>) -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new(message)
}
