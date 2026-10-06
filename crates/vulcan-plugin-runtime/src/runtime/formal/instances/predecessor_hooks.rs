//! An aborted original prepares one actual old Hook through the same persistent core machinery.
//! 已中止原请求通过相同持久核心机械逻辑准备单个实际旧 Hook。

use super::*;
use crate::VulcanPluginPredecessorRestorationSource;
use crate::runtime::private_hooks::PrivateHookOrigin;

impl VulcanPluginRuntimeManager {
    /// Retain source's same actually Ready old Hook for publication without creating or initializing another instance.
    /// 为发布保留 source 相同实际就绪旧 Hook，不创建或初始化另一实例。
    /// Caller holds registration exclusion; no-Hook declarations return None, while missing declared owners fail.
    /// 调用方持有注册互斥；无 Hook 声明返回 None，缺失已声明所有者则失败。
    pub(in crate::runtime) fn activation_predecessor_hook_for_publication(
        &self,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
    ) -> Result<Option<Arc<PersistentRuntime>>, VulcanPluginRuntimeError> {
        source.validate(&self.state)?;
        if source.plugin().hooks.is_empty() {
            return Ok(None);
        }
        // The actual formal parent is required only for an explicitly declared old Hook.
        // 仅显式声明旧 Hook 时才要求实际正式父级。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
        // Retain the existing registry allocation without minting preparation or fallback authority.
        // 保留既有注册表分配，不签发准备或回退权威。
        let runtime = owner
            .hooks
            .lock()
            .map_err(|_| instance_error("Persistent registry lock is poisoned."))?
            .get(&source.plugin().id)
            .cloned()
            .ok_or_else(|| {
                instance_error("Declared predecessor Hook has no original prepared instance.")
            })?;
        {
            // This same original source allocation must have produced the recorded core owners.
            // 此相同原来源分配必须已产生已记录核心所有者。
            let state = runtime.lock()?;
            if !matches!(&state.private_origin, Some(PrivateHookOrigin::Predecessor(original)) if Arc::ptr_eq(original, source))
                || runtime.plugin.generation != source.plugin().generation
            {
                return Err(instance_error(
                    "Private predecessor Hook belongs to another original source.",
                ));
            }
            self.validate_private_hook_ready(&owner, &runtime, &state)?;
        }
        Ok(Some(runtime))
    }

    /// Initialize source's exact old Hook while its independent Pending owner keeps ordinary business closed.
    /// 独立 Pending 所有者保持普通业务关闭时，初始化 source 的精确旧 Hook。
    /// source must be the same original manager-issued physical restoration Arc, with immutable old configuration.
    /// source 必须为相同原管理器签发物理恢复 Arc，携带不可变旧配置。
    /// Returns false for no Hook or true for the same actual successful initialization; never authorizes transfer.
    /// 无 Hook 返回 false，相同实际初始化成功返回 true；绝不授权转交。
    pub async fn prewarm_activation_predecessor_hook(
        &self,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        // Preserve the sole existing Hook budget across admission, preparation and actual observation.
        // 在入场、准备及实际观测期间保留唯一既有 Hook 预算。
        let expires = deadline(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS)?;
        // The retained successor descriptor supplies the only old plugin namespace and routing generation.
        // 保留后继描述符提供唯一旧插件命名空间及路由代次。
        let plugin_id = &source.plugin().id;
        // Serialize restoration initialization with actual Hook drain using the existing namespace gate.
        // 使用既有命名空间门，将恢复初始化与实际 Hook 排空串行化。
        let gate =
            self.persistent_runtime_gate(plugin_id, VulcanPluginPersistentRuntimeKind::Hook)?;
        // The same finite admission borrow remains live until actual initialization observation finishes.
        // 相同有限入场借用保持存活，直至实际初始化观测完成。
        let _initialization = tokio::time::timeout(remaining(expires)?, gate.lock_owned())
            .await
            .map_err(|_| {
                instance_error("Private predecessor Hook initialization admission timed out.")
            })?;
        // Decide first ownership under registration exclusion before any filesystem or Lua preparation.
        // 在任何文件系统或 Lua 准备前，在注册互斥下决定首次所有权。
        let (runtime, created) = {
            // No publisher may replace the exact closed route during original registry admission.
            // 原注册表入场期间，发布器不能替换精确关闭路由。
            let _registration = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
            source.validate(&self.state)?;
            if source.plugin().hooks.is_empty() {
                return Ok(false);
            }
            // The actual formal parent remains the sole owner of the original core resources.
            // 实际正式父级保持为原核心资源唯一所有者。
            let owner = self
                .formal_runtime_owner()?
                .ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
            // Use the existing bounded Hook registry instead of a parallel recovery registry.
            // 使用既有有界 Hook 注册表，不使用并行恢复注册表。
            let mut hooks = owner
                .hooks
                .lock()
                .map_err(|_| instance_error("Persistent registry lock is poisoned."))?;
            if let Some(runtime) = hooks.get(plugin_id) {
                // This same registered instance retains its explicit original physical source.
                // 此相同已登记实例保留其显式原物理来源。
                let state = runtime.lock()?;
                if !matches!(&state.private_origin, Some(PrivateHookOrigin::Predecessor(original)) if Arc::ptr_eq(original, source))
                    || runtime.plugin.generation != source.plugin().generation
                    || runtime.closing.load(Ordering::Acquire)
                    || state.retired
                {
                    return Err(instance_error(
                        "Original predecessor Hook is changed, closing or retired; explicit drain is required.",
                    ));
                }
                (Arc::clone(runtime), false)
            } else {
                if hooks.len() >= owner.settings.parent.max_sessions {
                    return Err(instance_error(
                        "Formal persistent ownership capacity is exhausted.",
                    ));
                }
                // Consume before resource creation; every later error, cancellation or actual retirement retains this fact.
                // 资源创建前消费；每个后续错误、取消或实际退役均保留此事实。
                source.begin_hook_initialization(&self.state)?;
                // Register the same actual persistent instance before returning to any cancellable observation.
                // 返回任何可取消观测前，登记相同实际持久实例。
                let runtime = Arc::new(PersistentRuntime {
                    kind: VulcanPluginPersistentRuntimeKind::Hook,
                    operation_gate: Arc::new(tokio::sync::Mutex::new(())),
                    plugin: source.plugin().clone(),
                    closing: AtomicBool::new(false),
                    faulted: AtomicBool::new(false),
                    state: Mutex::new(PersistentState {
                        preparing: true,
                        private_origin: Some(PrivateHookOrigin::Predecessor(Arc::clone(source))),
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
}
