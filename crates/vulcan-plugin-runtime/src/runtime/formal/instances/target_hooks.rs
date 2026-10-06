//! Original target initialization and completion transfer the existing persistent Hook under publication exclusion.
//! 原目标初始化及完成在发布互斥下转交既有持久 Hook。

use super::*;
use crate::runtime::candidates::ActivationTargetHookAuthority;

/// A cancelled private initializer closes original admission while existing supervision retains physical work.
/// 已取消私有初始化方关闭原入场，同时既有监督保留物理工作。
struct TargetHookObservation {
    /// Same registered instance survives the waiting future.
    /// 相同已注册实例在等待 future 之后存活。
    runtime: Arc<PersistentRuntime>,
    /// Successful initialization disarms cancellation without releasing the private origin.
    /// 初始化成功解除取消，不释放私有来源。
    delivered: bool,
}

impl Drop for TargetHookObservation {
    /// Request closure only while this same initializer still owns private work; never cancel a transferred instance.
    /// 仅此相同初始化方仍拥有私有工作时请求关闭；绝不取消已转交实例。
    /// No inputs or result; the actual instance state serializes cancellation with publication and transfer.
    /// 无输入及结果；实际实例状态将取消与发布及转交串行化。
    fn drop(&mut self) {
        if !self.delivered {
            // Confirm the original private ownership under the same guard held throughout actual transfer.
            // 在实际转交全程持有的相同守卫下，确认原私有所有权。
            match self.runtime.lock() {
                Ok(state) => {
                    if state.private_origin.is_some() {
                        self.runtime.closing.store(true, Ordering::Release);
                    }
                }
                // Poison cannot prove that ownership transferred; retain the existing explicit closed behavior.
                // 锁中毒不能证明所有权已转交；保留既有明确关闭行为。
                Err(_) => self.runtime.closing.store(true, Ordering::Release),
            }
        }
    }
}

/// Publication evidence is locked before the external synchronous publisher, then bound without fallible work.
/// 外部同步发布器执行前锁定发布证据，随后绑定且不执行可能失败的工作。
pub(in crate::runtime) struct PrivateHookPublication<'a>(
    std::sync::MutexGuard<'a, PersistentState>,
);

impl PrivateHookPublication<'_> {
    /// Record the same final Arc actually accepted by the extension publisher; no I/O or registration is performed.
    /// 记录扩展发布器实际接受的相同最终 Arc；不执行 I/O 或注册。
    pub(in crate::runtime) fn published(
        mut self,
        catalog: &Arc<crate::VulcanPluginCatalogSnapshot>,
    ) {
        self.0.private_catalog = Some(Arc::downgrade(catalog));
    }
}

impl PersistentRuntime {
    /// Observe original core identities for focused handoff tests without retaining the native domain.
    /// 为定向交接测试观测原核心身份，不保留原生域。
    #[cfg(test)]
    pub(in crate::runtime) fn target_test_identity(
        &self,
    ) -> (Option<String>, Option<String>, Option<String>, bool, bool) {
        // Hold only the original persistent evidence for this synchronous observation.
        // 仅为此同步观察持有原持久证据。
        let state = self.lock().expect("test persistent state");
        (
            state.pool.clone(),
            state.session.clone(),
            state
                .initialization
                .as_ref()
                .map(|operation| operation.id().to_owned()),
            state.private_origin.is_some(),
            state.retired,
        )
    }

    /// Reserve this original instance's publication checkpoint before the publisher; return a short evidence guard.
    /// 发布器执行前预留此原实例发布检查点；返回短证据守卫。
    pub(in crate::runtime) fn private_publication_guard(
        &self,
    ) -> Result<PrivateHookPublication<'_>, VulcanPluginRuntimeError> {
        Ok(PrivateHookPublication(self.lock()?))
    }
}

impl VulcanPluginRuntimeManager {
    /// Persist the actual original terminal initialization observation before cleanup can forget its evidence.
    /// 清理可以遗忘凭据之前，持久保存实际原终态初始化观测。
    /// owner/state identify the original live resources; absence or unfinished work leaves Started intact.
    /// owner／state 标识原存活资源；缺失或未完成工作保持 Started 不变。
    pub(super) fn observe_target_hook_initialization(
        &self,
        owner: &ManagedFormalRuntime,
        state: &mut PersistentState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if state
            .target_attempt
            .as_ref()
            .is_none_or(|attempt| attempt.is_observed())
            || state.preparing
        {
            return Ok(());
        }
        if let Some(error) = &state.target_receipt_error {
            return Err(instance_error(error));
        }
        // Submission uncertainty preserves Started without inventing operation or session identifiers.
        // 提交不确定时保留 Started，不虚构操作或会话标识。
        let Some(initialization) = &state.initialization else {
            return Ok(());
        };
        // Only the original core handle decides whether actual initialization has terminated.
        // 仅原核心句柄决定实际初始化是否已终止。
        let snapshot = initialization.snapshot().map_err(core_error)?;
        if !snapshot.phase.is_terminal() {
            return Ok(());
        }
        // Read the actual session before any forget acknowledgement can remove this evidence.
        // 在任何遗忘确认可以移除此证据之前，读取实际会话。
        let session_id = state
            .session
            .as_ref()
            .ok_or_else(|| instance_error("Original target Hook session evidence is missing."))?;
        // The registered original pool must agree with core's retained session identity.
        // 已注册原池必须与核心保留会话身份一致。
        let pool = state
            .pool
            .as_ref()
            .ok_or_else(|| instance_error("Original target Hook pool evidence is missing."))?;
        // Query only the recorded actual session; no replacement identity lookup is allowed.
        // 仅查询已记录实际会话；不允许查询替代身份。
        let session = owner
            .host
            .runtime()
            .session(session_id)
            .map_err(core_error)?;
        if session.pool_id != *pool || snapshot.operation_id != initialization.id() {
            return Err(instance_error(
                "Original target Hook terminal evidence differs from its actual admission.",
            ));
        }
        state
            .target_attempt
            .as_mut()
            .expect("checked original attempt")
            .finish(
                pool,
                session_id,
                initialization.id(),
                snapshot.phase == luaskills::runtime::embedded::OperationPhase::Succeeded
                    && snapshot.error.is_none()
                    && session.error.is_none(),
            )
            .map_err(|error| instance_error(&error.to_string()))
    }

    /// Spawn preparation once for owner/runtime using its already-registered evidence and original expires budget.
    /// 使用已登记证据及原 expires 预算，为 owner／runtime 仅派发一次准备。
    /// preparation owns interruption evidence; returns after supervision is installed, without waiting for Lua.
    /// preparation 拥有中断证据；安装监督后返回，不等待 Lua。
    pub(super) fn spawn_instance_preparation(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        runtime: &Arc<PersistentRuntime>,
        preparation: PersistentPreparation,
        expires: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.start_instance_monitor(owner, runtime)?;
        // Retain the initiating manager through actual blocking preparation.
        // 在实际阻塞准备期间保留发起管理器。
        let manager = self.clone();
        // Retain the sole formal parent that owns the original native resources.
        // 保留拥有原原生资源的唯一正式父级。
        let owner = Arc::clone(owner);
        // Retain the original registry instance throughout preparation and observation.
        // 在准备及观察全程保留原注册表实例。
        let runtime = Arc::clone(runtime);
        self.supervisor_handle()?.spawn_blocking(move || {
            // Keep interruption ownership until the actual blocking closure finishes.
            // 保留中断所有权至实际阻塞闭包结束。
            let _preparation = preparation;
            // Preserve the actual preparation or business result without inventing completion.
            // 保留实际准备或业务结果，不虚构完成。
            let result = manager.prepare_formal_instance(&owner, &runtime, expires);
            if result.is_err() {
                runtime.closing.store(true, Ordering::Release);
            }
            if let Ok(mut state) = runtime.lock() {
                state.prepared = Some(result);
                state.preparing = false;
            }
        });
        Ok(())
    }

    /// Validate the exact explicit preparation origin; ordinary routes retain their existing generation checks.
    /// 校验精确显式准备来源；普通路由保留既有代次检查。
    pub(super) fn validate_persistent_preparation(
        &self,
        runtime: &PersistentRuntime,
        private: Option<&crate::runtime::private_hooks::PrivateHookOrigin>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        match private {
            Some(authority) => authority.validate(&self.state, &runtime.plugin),
            None => self.ensure_current_persistent_generation(
                &runtime.plugin.id,
                runtime.plugin.generation,
            ),
        }
    }

    /// Initialize the exact Published transaction's staged target while every ordinary business route remains closed.
    /// 全部普通业务路由保持关闭时，初始化精确 Published 事务的暂存目标。
    /// transaction retains the original exclusive request; staged must be the current staging Arc.
    /// transaction 保留原排他请求；staged 必须为当前暂存 Arc。
    /// Returns false for an actual no-Hook target; true confirms the original fixed session initialized successfully.
    /// 实际无 Hook 目标返回 false；true 确认原固定会话初始化成功。
    pub async fn prewarm_activation_target_hook(
        &self,
        transaction: &crate::VulcanPluginActivationTransaction,
        staged: &Arc<crate::VulcanPluginCatalogSnapshot>,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        // Freeze the sole existing Hook budget before admission and preparation.
        // 在入场及准备前固定唯一既有 Hook 预算。
        let expires = deadline(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS)?;
        // Select the exact plugin namespace from the original target.
        // 从原目标选择精确插件命名空间。
        let plugin_id = &transaction.record().target().plugin_id;
        // Serialize original persistent initialization with its existing drain entry.
        // 使用既有排空入口串行化原持久初始化。
        let gate =
            self.persistent_runtime_gate(plugin_id, VulcanPluginPersistentRuntimeKind::Hook)?;
        // Keep initialization admission inside the original finite deadline.
        // 使初始化入场保持在原有限截止时间内。
        let _initialization = tokio::time::timeout(remaining(expires)?, gate.lock_owned())
            .await
            .map_err(|_| {
                instance_error("Private target Hook initialization admission timed out.")
            })?;
        // Capture original ownership and its first-preparation decision under one registry observation.
        // 在单个注册表观察下捕获原所有权及其首次准备决定。
        let (runtime, created) = {
            // Exclude route publication during the original ownership transition.
            // 在原所有权转换期间排除路由发布。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
            // Retain the original request and full selected target as one private authority.
            // 将原请求及完整已选目标保留为单个私有权威。
            let authority = ActivationTargetHookAuthority::new(&self.state, transaction, staged)?;
            if authority.plugin.hooks.is_empty() {
                return Ok(false);
            }
            // Retain the sole formal parent that owns the original native resources.
            // 保留拥有原原生资源的唯一正式父级。
            let owner = self
                .formal_runtime_owner()?
                .ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
            // Use the existing bounded Hook registry for original ownership.
            // 使用既有有界 Hook 注册表保存原所有权。
            let mut hooks = owner
                .hooks
                .lock()
                .map_err(|_| instance_error("Persistent registry lock is poisoned."))?;
            if let Some(runtime) = hooks.get(plugin_id) {
                // Hold only the original persistent evidence for this synchronous observation.
                // 仅为此同步观察持有原持久证据。
                let state = runtime.lock()?;
                // Retain the exact original instance or authority rather than creating a replacement.
                // 保留精确原实例或权威，不创建替代物。
                let original = state
                    .private_origin
                    .as_ref()
                    .ok_or_else(|| {
                        instance_error("Existing Hook is not this original private target.")
                    })?
                    .target()?;
                if original.request_id != authority.request_id
                    || original.target != authority.target
                    || runtime.plugin.generation != authority.plugin.generation
                    || !Arc::ptr_eq(&original.staged, staged)
                    || runtime.closing.load(Ordering::Acquire)
                    || state.retired
                {
                    return Err(instance_error(
                        "Original private target Hook is changed, closing or retired; explicit drain is required.",
                    ));
                }
                (Arc::clone(runtime), false)
            } else {
                authority
                    .initialization_owner
                    .require_unattempted()
                    .map_err(|error| instance_error(&error.to_string()))?;
                if hooks.len() >= owner.settings.parent.max_sessions {
                    return Err(instance_error(
                        "Formal persistent ownership capacity is exhausted.",
                    ));
                }
                // Persist the one-attempt boundary before preparation can allocate or initialize actual resources.
                // 在准备可以分配或初始化实际资源之前，持久保存单次尝试边界。
                let attempt = authority
                    .initialization_owner
                    .begin()
                    .map_err(|error| instance_error(&error.to_string()))?;
                // Retain the original registry instance throughout preparation and observation.
                // 在准备及观察全程保留原注册表实例。
                let runtime = Arc::new(PersistentRuntime {
                    kind: VulcanPluginPersistentRuntimeKind::Hook,
                    operation_gate: Arc::new(tokio::sync::Mutex::new(())),
                    plugin: authority.plugin.clone(),
                    closing: AtomicBool::new(false),
                    faulted: AtomicBool::new(false),
                    state: Mutex::new(PersistentState {
                        preparing: true,
                        private_origin: Some(
                            crate::runtime::private_hooks::PrivateHookOrigin::Target(authority),
                        ),
                        target_attempt: Some(attempt),
                        ..PersistentState::default()
                    }),
                });
                hooks.insert(plugin_id.clone(), Arc::clone(&runtime));
                (runtime, true)
            }
        };
        self.wait_private_hook_initialization(&runtime, created, expires)
            .await
    }

    /// Observe the registered original private runtime without replaying preparation or core initialization.
    /// 观测已登记原私有运行时，不重放准备或核心初始化。
    /// runtime/created identify its sole registry admission; expires is the caller's original finite Hook budget.
    /// runtime／created 标识其唯一注册表入场；expires 为调用方原有限 Hook 预算。
    /// Returns true only for actual successful core initialization; cancellation requests original physical closure.
    /// 仅实际核心初始化成功返回 true；取消请求原物理关闭。
    pub(super) async fn wait_private_hook_initialization(
        &self,
        runtime: &Arc<PersistentRuntime>,
        created: bool,
        expires: Instant,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        // Retain the sole formal parent that owns the original native resources.
        // 保留拥有原原生资源的唯一正式父级。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
        // Tie waiter cancellation to the original registered preparation.
        // 将等待方取消绑定至原已登记准备。
        let mut observation = TargetHookObservation {
            runtime: Arc::clone(runtime),
            delivered: false,
        };
        if created {
            // Install interruption ownership before spawning actual preparation.
            // 派发实际准备前安装中断所有权。
            let preparation = PersistentPreparation(Arc::clone(runtime));
            self.spawn_instance_preparation(&owner, runtime, preparation, expires)?;
        } else {
            self.start_instance_monitor(&owner, runtime)?;
        }
        loop {
            {
                // Hold only the original persistent evidence for this synchronous observation.
                // 仅为此同步观察持有原持久证据。
                let mut state = runtime.lock()?;
                self.observe_target_hook_initialization(&owner, &mut state)?;
                if let Some(result) = &state.prepared {
                    result.clone()?;
                    // Observe the one original core initialization operation.
                    // 观测唯一原核心初始化操作。
                    let initialization = state.initialization.as_ref().ok_or_else(|| {
                        instance_error("Private target Hook has no initialization operation.")
                    })?;
                    // Observe original core or catalog evidence without replaying work.
                    // 观测原核心或目录证据，不重放工作。
                    let snapshot = initialization.snapshot().map_err(core_error)?;
                    if snapshot.phase.is_terminal() {
                        if let Some(error) = snapshot.error {
                            return Err(core_error(error));
                        }
                        self.validate_private_hook_ready(&owner, runtime, &state)?;
                        observation.delivered = true;
                        return Ok(true);
                    }
                }
                if runtime.closing.load(Ordering::Acquire) || state.retired {
                    return Err(instance_error(
                        "Private target Hook initialization was cancelled; original ownership remains retained.",
                    ));
                }
            }
            remaining(expires).map_err(|_| instance_error("Private target Hook initialization timed out; original ownership remains retained."))?;
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }

    /// Check real initialization and the same original idle session before publication or transfer.
    /// 发布或转交前检查真实初始化及相同原空闲会话。
    /// owner is the sole formal parent; runtime/state retain exact original pool, session and private authority.
    /// owner 为唯一正式父级；runtime／state 保留精确原池、会话及私有权威。
    pub(super) fn validate_private_hook_ready(
        &self,
        owner: &ManagedFormalRuntime,
        runtime: &PersistentRuntime,
        state: &PersistentState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Retain the original request and full selected target as one private authority.
        // 将原请求及完整已选目标保留为单个私有权威。
        let authority = state
            .private_origin
            .as_ref()
            .ok_or_else(|| instance_error("Original private target Hook authority is missing."))?;
        authority.validate(&self.state, &runtime.plugin)?;
        self.validate_private_hook_actual_ready(owner, runtime, state)
    }

    /// Validate the same actual private core owners independently of source-specific catalog eligibility.
    /// 独立于特定来源目录资格，校验相同实际私有核心所有者。
    /// owner/runtime/state retain the original pool, session and origin; return physical readiness or explicit failure.
    /// owner／runtime／state 保留原池、会话及来源；返回物理就绪或明确失败。
    pub(super) fn validate_private_hook_actual_ready(
        &self,
        owner: &ManagedFormalRuntime,
        runtime: &PersistentRuntime,
        state: &PersistentState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Pure mechanical readiness still requires this exact retained private origin and its actual domain.
        // 纯机械就绪仍要求此精确保留私有来源及其实际域。
        let authority = state
            .private_origin
            .as_ref()
            .ok_or_else(|| instance_error("Original private Hook authority is missing."))?;
        if runtime.kind != VulcanPluginPersistentRuntimeKind::Hook
            || runtime.closing.load(Ordering::Acquire)
            || state.preparing
            || state.retired
            || !state.operations.is_empty()
            || !matches!(&state.prepared, Some(Ok(())))
            || state.target_receipt_error.is_some()
        {
            return Err(instance_error(
                "Original private target Hook is not ready for handoff.",
            ));
        }
        // Observe the one original core initialization operation.
        // 观测唯一原核心初始化操作。
        let initialization = state
            .initialization
            .as_ref()
            .ok_or_else(|| instance_error("Original private Hook initialization is missing."))?
            .snapshot()
            .map_err(core_error)?;
        if initialization.phase != luaskills::runtime::embedded::OperationPhase::Succeeded
            || initialization.error.is_some()
        {
            return Err(instance_error(
                "Original private Hook initialization did not succeed.",
            ));
        }
        // Select the recorded immutable original core pool identity.
        // 选择已记录不可变原核心池身份。
        let pool = state
            .pool
            .as_ref()
            .ok_or_else(|| instance_error("Original private Hook pool is missing."))?;
        // Target receipts and predecessor's single consumed source remain distinct authorization branches.
        // 目标回执与前驱单次已消费来源保持不同授权分支。
        match authority {
            crate::runtime::private_hooks::PrivateHookOrigin::Target(_) => {
                if !state.target_attempt.as_ref().is_some_and(|attempt| {
                    state.session.as_ref().is_some_and(|session| {
                        attempt.succeeded_for(pool, session, &initialization.operation_id)
                    })
                }) {
                    return Err(instance_error(
                        "Original private Hook has no confirmed durable initialization success.",
                    ));
                }
            }
            crate::runtime::private_hooks::PrivateHookOrigin::Predecessor(_) => {
                if state.target_attempt.is_some() {
                    return Err(instance_error(
                        "Predecessor Hook cannot borrow a target initialization receipt.",
                    ));
                }
            }
        }
        // Address the same original native domain through its restricted weak handle.
        // 通过受限弱句柄指向相同原原生域。
        let handoff = state
            .private_handoff
            .as_ref()
            .ok_or_else(|| instance_error("Original private Hook domain is missing."))?;
        // Query the same original core session instead of inferring readiness.
        // 查询相同原核心会话，不推断就绪。
        let session = owner
            .host
            .runtime()
            .session(
                state
                    .session
                    .as_ref()
                    .ok_or_else(|| instance_error("Original private Hook session is missing."))?,
            )
            .map_err(core_error)?;
        if &session.pool_id != pool
            || &handoff.pool_id != pool
            || session.phase != EmbeddedSessionPhase::Ready
            || session.active_operation.is_some()
            || session.queued_calls != 0
            || session.error.is_some()
        {
            return Err(instance_error(
                "Original private Hook session is not idle and open.",
            ));
        }
        handoff
            .retain_domain()
            .map_err(core_error)?
            .prepare_transfer(authority)
            .map_err(core_error)?;
        Ok(())
    }

    /// Require the explicit target origin before validating common actual Ready evidence for target publication.
    /// 目标发布的共用实际就绪凭据校验前，要求显式目标来源。
    /// owner/runtime/state address the same registered original; returns a conflict for predecessor authority.
    /// owner／runtime／state 指向相同已登记原实例；前驱权威返回冲突。
    fn validate_target_hook_ready(
        &self,
        owner: &ManagedFormalRuntime,
        runtime: &PersistentRuntime,
        state: &PersistentState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        state
            .private_origin
            .as_ref()
            .ok_or_else(|| instance_error("Original private target Hook authority is missing."))?
            .target()?;
        self.validate_private_hook_ready(owner, runtime, state)
    }

    /// Retain an existing original private target for synchronous publication; no instance is fabricated when absent.
    /// 为同步发布保留既有原私有目标；缺失时不虚构实例。
    pub(in crate::runtime) fn activation_target_hook_for_publication(
        &self,
        transaction: &crate::VulcanPluginActivationTransaction,
    ) -> Result<Option<Arc<PersistentRuntime>>, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        // Retain the original registry instance throughout preparation and observation.
        // 在准备及观察全程保留原注册表实例。
        let runtime = owner
            .hooks
            .lock()
            .map_err(|_| instance_error("Persistent registry lock is poisoned."))?
            .get(&transaction.record().target().plugin_id)
            .cloned();
        if let Some(runtime) = &runtime {
            // Hold only the original persistent evidence for this synchronous observation.
            // 仅为此同步观察持有原持久证据。
            let state = runtime.lock()?;
            self.validate_target_hook_ready(&owner, runtime, &state)?;
            // Retain the exact original instance or authority rather than creating a replacement.
            // 保留精确原实例或权威，不创建替代物。
            let original = state
                .private_origin
                .as_ref()
                .expect("validated private target")
                .target()?;
            if original.request_id != transaction.record().request_id()
                || &original.target != transaction.record().target()
            {
                return Err(instance_error(
                    "Private target Hook belongs to another original transaction.",
                ));
            }
        }
        Ok(runtime)
    }

    /// Confirm the original full extension handoff and transfer the same actual Hook pool/session/domain to business.
    /// 确认原完整扩展交接，并将相同实际 Hook 池／会话／域转交给业务。
    /// transaction is the mutable original owner; publication is its sealed successful full publisher witness.
    /// transaction 为可变原所有者；publication 为其封闭成功完整发布器凭据。
    /// Completion uncertainty retains private ownership and closes the original route; confirmed completion performs no further I/O.
    /// 完成不确定时保留私有所有权并关闭原路由；已确认完成后不再执行 I/O。
    pub fn complete_activation_target_handoff(
        &self,
        transaction: &mut crate::VulcanPluginActivationTransaction,
        publication: &crate::VulcanPluginActivationTargetPublication,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.complete_activation_target_handoff_with(
            transaction,
            publication,
            &NoTargetHandoffAdapter,
        )
    }

    /// Complete all original target resources under registration exclusion, preparing the participant inside this gate.
    /// 在注册互斥下完成全部原目标资源，并在此门内准备参与者。
    /// transaction/publication identify the exact original full publisher; adapter borrows the registration scope for every guard.
    /// transaction／publication 标识精确原完整发布器；adapter 为每个守卫借用注册作用域。
    /// Unknown completion preserves every private owner; only confirmed completion invokes infallible memory transfer.
    /// 未知完成保留每个私有所有者；仅已确认完成调用不可失败内存转交。
    pub fn complete_activation_target_handoff_with<A>(
        &self,
        transaction: &mut crate::VulcanPluginActivationTransaction,
        publication: &crate::VulcanPluginActivationTargetPublication,
        adapter: &A,
    ) -> Result<(), VulcanPluginRuntimeError>
    where
        A: VulcanPluginNativeMcpHandoffAdapter,
    {
        // Exclude route publication during the original ownership transition.
        // 在原所有权转换期间排除路由发布。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
        // Require this manager and exact full original record before trusting the successful publication.
        // 信任成功发布之前，要求此管理器及精确完整原记录。
        if !publication.matches_owner(self, transaction) {
            return Err(instance_error(
                "Target publication belongs to another manager or original request.",
            ));
        }
        // Only this sealed publisher witness supplies the final catalog and the actual closed rollback Arc.
        // 仅此封闭发布器凭据提供最终目录及实际关闭回退 Arc。
        let catalog = publication.catalog();
        // Open the original store without allocating another request owner.
        // 打开原存储，不分配另一个请求所有者。
        let store = crate::VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|error| instance_error(&error.to_string()))?
            .ok_or_else(|| instance_error("Original activation store is missing."))?;
        // Freeze the complete original selected target.
        // 固定完整原已选目标。
        let target = transaction.record().target();
        // Retain the descriptor selected by the exact current catalog.
        // 保留精确当前目录选择的描述符。
        let plugin = catalog
            .plugins
            .get(&target.plugin_id)
            .ok_or_else(|| instance_error("Final target catalog is missing."))?;
        if !Arc::ptr_eq(catalog, &self.snapshot())
            || publication.generation() != plugin.generation
            || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || !plugin
                .managed_package()
                .is_some_and(|package| package.matches_activation(target))
            || !transaction
                .record()
                .update_policy()
                .is_some_and(|policy| policy.desired_enabled())
        {
            return Err(instance_error(
                "Completion requires the exact final published original target catalog.",
            ));
        }
        // Borrow the actual existing registration guard; participant acquisition cannot precede this manager gate.
        // 借用实际既有注册守卫；参与者取得不可早于此管理器注册门。
        let registration =
            VulcanPluginActivationTargetRegistration::new(&_publication, &self.state);
        // Explicit native registration binds preparation to this exact manager before accepting backend evidence.
        // 接受后端证据之前，显式原生注册将准备绑定至此精确管理器。
        let has_global_mcp = plugin
            .mcp_servers
            .iter()
            .any(|server| server.manifest.scope == crate::VulcanPluginMcpScopeManifest::Global);
        match (has_global_mcp, adapter.authority()) {
            (false, None) => {}
            (true, Some(authority)) if authority.matches_manager(self) => {}
            _ => {
                return Err(instance_error(
                    "Target MCP handoff requires its explicitly registered native host authority.",
                ));
            }
        }
        // Acquire participant guards after registration exclusion and before source/catalog write guards.
        // 在注册互斥之后、来源／目录写守卫之前取得参与者守卫。
        let mut participant = adapter.prepare(self, transaction, catalog, &registration)?;
        // Global declarations require sealed same-owner evidence; an empty participant cannot counterfeit readiness.
        // 全局声明要求封闭同所有者证据；空参与者不可伪造就绪。
        // Keep the participant's evidence borrowed while its actual backend/source guards remain held.
        // 实际后端／来源守卫仍持有时，保持借用参与者证据。
        let expected_mcp = participant.mcp_evidence();
        match (has_global_mcp, expected_mcp) {
            (false, None) => {}
            (true, Some(evidence))
                if evidence.request_id() == transaction.record().request_id()
                    && evidence.target() == transaction.record().target()
                    && evidence.generation() == plugin.generation => {}
            _ => {
                return Err(instance_error(
                    "Target MCP completion requires exact sealed original owner evidence.",
                ));
            }
        }
        // Seal original selection and the validated actual MCP owner UUIDs before any durable completion write.
        // 在任何持久完成写入之前，封闭原选择及已校验实际 MCP 所有者 UUID。
        let confirmation = VulcanPluginActivationTargetCompletion::new(
            transaction,
            plugin.generation,
            expected_mcp,
        );
        // Retain the original registry instance throughout preparation and observation.
        // 在准备及观察全程保留原注册表实例。
        let runtime = self.activation_target_hook_for_publication(transaction)?;
        if plugin.hooks.is_empty() {
            if runtime.is_some() {
                return Err(instance_error(
                    "No-Hook target has an unexpected private instance.",
                ));
            }
            // Even an empty Hook path closes ordinary target surfaces after an unconfirmed durable outcome.
            // 即使 Hook 路径为空，也在未确认持久结果后关闭普通目标暴露面。
            let closed = Arc::clone(publication.closed_catalog());
            // Reserve the existing lifecycle authority before writing completion.
            // 写入完成前预留既有生命周期权威。
            let mut lifecycle = self
                .state
                .lifecycle_overrides
                .lock()
                .map_err(|_| instance_error("Plugin lifecycle lock is poisoned."))?;
            // Retain the exact current catalog observation for this checked transition.
            // 为此已校验转换保留精确当前目录观察。
            let mut current = self
                .state
                .snapshot
                .write()
                .map_err(|_| instance_error("Plugin catalog lock is poisoned."))?;
            return match store
                .validate_published_target_handoff_owner(transaction)
                .and_then(|()| {
                    store.complete_activation_target_handoff_owned(transaction, None, expected_mcp)
                }) {
                Ok(()) => {
                    participant.confirm_completed(&confirmation);
                    Ok(())
                }
                Err(error) => {
                    lifecycle.insert(plugin.id.clone(), VulcanPluginRuntimeState::Enabling);
                    *current = closed;
                    Err(instance_error(&error.to_string()))
                }
            };
        }
        // Retain the sole formal parent that owns the original native resources.
        // 保留拥有原原生资源的唯一正式父级。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
        // Retain the original registry instance throughout preparation and observation.
        // 在准备及观察全程保留原注册表实例。
        let runtime = runtime.ok_or_else(|| {
            instance_error("Original target Hook must initialize before completion.")
        })?;
        // Hold only the original persistent evidence for this synchronous observation.
        // 仅为此同步观察持有原持久证据。
        let mut state = runtime.lock()?;
        self.validate_target_hook_ready(&owner, &runtime, &state)?;
        // Retain the explicit private origin until confirmed durable transfer.
        // 保留显式私有来源至已确认持久转交。
        let origin = state
            .private_origin
            .as_ref()
            .expect("validated private target");
        // Require the exact successful publisher catalog pointer.
        // 要求精确成功发布器目录指针。
        let published = state
            .private_catalog
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                instance_error("Original complete-extension publication evidence is missing.")
            })?;
        if !Arc::ptr_eq(&published, catalog) || plugin.generation != origin.plugin().generation {
            return Err(instance_error(
                "Original target Hook differs from the actual final extension catalog.",
            ));
        }
        // Acquire every fallible transfer and rollback guard before the durable completion point.
        // 在持久完成点前取得全部可能失败的转交及回退守卫。
        let domain = state
            .private_handoff
            .as_ref()
            .expect("validated handoff")
            .retain_domain()
            .map_err(core_error)?;
        // Prepare the original source change before any durable completion write.
        // 在任何持久完成写入前准备原来源变更。
        let transfer = domain.prepare_transfer(origin).map_err(core_error)?;
        // Prepare original closed route restoration before the durable boundary.
        // 在持久边界前准备原关闭路由恢复。
        let closed = Arc::clone(publication.closed_catalog());
        // Reserve the existing lifecycle authority before writing completion.
        // 写入完成前预留既有生命周期权威。
        let mut lifecycle = self
            .state
            .lifecycle_overrides
            .lock()
            .map_err(|_| instance_error("Plugin lifecycle lock is poisoned."))?;
        // Retain the exact current catalog observation for this checked transition.
        // 为此已校验转换保留精确当前目录观察。
        let mut current = self
            .state
            .snapshot
            .write()
            .map_err(|_| instance_error("Plugin catalog lock is poisoned."))?;
        match store
            .validate_published_target_handoff_owner(transaction)
            .and_then(|()| {
                store.complete_activation_target_handoff_owned(
                    transaction,
                    Some((
                        origin.plugin().generation,
                        state.pool.as_deref().expect("validated pool"),
                        state.session.as_deref().expect("validated session"),
                        state
                            .initialization
                            .as_ref()
                            .expect("validated initialization")
                            .id(),
                    )),
                    expected_mcp,
                )
            }) {
            Ok(()) => {
                // All remaining operations are infallible memory changes on the same original owners.
                // 剩余全部操作均为相同原所有者上的不可失败内存变更。
                transfer.publish();
                state.private_origin.take();
                state.target_attempt.take();
                state.private_handoff.take();
                state.private_catalog.take();
                participant.confirm_completed(&confirmation);
                Ok(())
            }
            Err(error) => {
                lifecycle.insert(plugin.id.clone(), VulcanPluginRuntimeState::Enabling);
                *current = closed;
                Err(instance_error(&error.to_string()))
            }
        }
    }
}
