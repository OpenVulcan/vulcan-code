//! Frozen plugin registrations over the shared conversation state.
//! 共享会话状态之上的冻结插件注册项。

use super::ConversationHookRuntime;
use crate::agent_runtime::vulcan_plugin_hooks::{
    VULCAN_PLUGIN_HOOK_REGISTRATION_PREFIX, synchronize_owned_vulcan_plugin_hooks,
};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, MutexGuard};
use vulcan_hooks::{HookDispatcher, HookError, HookRegistry};
use vulcan_plugin_runtime::{VulcanPluginCatalogSnapshot, VulcanPluginRuntimeManager};

#[cfg(test)]
mod tests;

/// Owns plugin handlers and the exact catalog used to prepare them.
/// 持有插件处理器及准备它们时使用的精确目录。
pub struct VulcanPluginConversationHooks {
    /// Catalog ownership keeps handlers and their package leases in one generation.
    /// 目录所有权使处理器及其包租约归属于同一代次。
    catalog: Arc<VulcanPluginCatalogSnapshot>,
    /// Only plugin registrations are retained; host listeners and handlers remain live.
    /// 仅保留插件注册项；宿主监听器与处理器保持实时状态。
    registry: HookRegistry,
}

impl VulcanPluginConversationHooks {
    /// Prepares inert handlers from `catalog`, validating them against `host` and `runtime`.
    /// 从 `catalog` 准备尚未执行的处理器，依据 `host` 与 `runtime` 校验。
    /// Returns a frozen partition without invoking Lua, publishing routes, or notifying listeners.
    /// 返回冻结分区，不调用 Lua、不发布路由，也不通知监听器。
    pub fn prepare(
        host: &HookDispatcher,
        runtime: VulcanPluginRuntimeManager,
        catalog: Arc<VulcanPluginCatalogSnapshot>,
    ) -> Result<Self, HookError> {
        // Import host policies without cloning live registrations or event listeners.
        // 导入宿主策略，不克隆实时注册项或事件监听器。
        let mut staged = HookDispatcher::new(HookRegistry::new());
        staged.register_points(host.registry().point_descriptors())?;
        synchronize_owned_vulcan_plugin_hooks(&mut staged, runtime, Arc::clone(&catalog))?;
        Ok(Self {
            catalog,
            registry: staged.registry().clone(),
        })
    }

    /// Returns the catalog that owns this partition; no catalog lookup is performed.
    /// 返回持有本分区的目录，不重新查找目录。
    pub fn catalog(&self) -> Arc<VulcanPluginCatalogSnapshot> {
        Arc::clone(&self.catalog)
    }
}

impl fmt::Debug for VulcanPluginConversationHooks {
    /// Writes only catalog identity to `formatter`, returning formatting success or failure.
    /// 仅将目录身份写入 `formatter`，返回格式化成功或失败。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VulcanPluginConversationHooks")
            .field("catalog_revision", &self.catalog.revision)
            .finish_non_exhaustive()
    }
}

/// Temporarily projects captured plugin handlers while retaining the original shared turn state.
/// 临时投影捕获的插件处理器，同时保留原共享回合状态。
/// Dispatcher changes inside this scope are temporary; permanent host registration uses the base lock.
/// 本作用域内的调度器修改是临时的；持久宿主注册使用基础锁。
pub struct ConversationHookGenerationGuard<'a> {
    /// Original mutex ownership prevents another caller from observing the temporary projection.
    /// 原互斥锁所有权防止其他调用方观察到临时投影。
    runtime: MutexGuard<'a, ConversationHookRuntime>,
    /// Saved dispatcher restored on normal return and unwind; absent for standalone callers.
    /// 正常返回或栈展开时恢复的原调度器；独立调用方未设置时为空。
    original: Option<HookDispatcher>,
}

impl<'a> ConversationHookGenerationGuard<'a> {
    /// Scopes `runtime` to `plugins`; `None` explicitly retains standalone dispatcher behavior.
    /// 将 `runtime` 限定到 `plugins`；`None` 显式保留独立调度器行为。
    /// Returns a guard after full validation; failure leaves the shared dispatcher untouched.
    /// 完成全部校验后返回守卫；失败保持共享调度器不变。
    pub fn new(
        mut runtime: MutexGuard<'a, ConversationHookRuntime>,
        plugins: Option<&VulcanPluginConversationHooks>,
    ) -> Result<Self, HookError> {
        // Build the complete overlay before replacing the live dispatcher.
        // 在替换实时调度器前构建完整投影。
        let original = if let Some(plugins) = plugins {
            // Keep live host listeners, point policies, and non-plugin registrations.
            // 保留实时宿主监听器、点位策略及非插件注册项。
            let mut projected = runtime.dispatcher_mut().clone();
            *projected.registry_mut() = projected.registry().with_registration_prefix_from(
                VULCAN_PLUGIN_HOOK_REGISTRATION_PREFIX,
                &plugins.registry,
            )?;
            Some(std::mem::replace(runtime.dispatcher_mut(), projected))
        } else {
            None
        };
        Ok(Self { runtime, original })
    }
}

impl Deref for ConversationHookGenerationGuard<'_> {
    /// Shared conversation state exposed by the guard.
    /// 守卫暴露的共享会话状态。
    type Target = ConversationHookRuntime;

    /// Returns a shared view of the original runtime with the scoped dispatcher.
    /// 返回具有作用域调度器的原运行时共享视图。
    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl DerefMut for ConversationHookGenerationGuard<'_> {
    /// Returns a mutable view; turn changes persist after this guard is released.
    /// 返回可变视图；释放守卫后回合变更仍然保留。
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runtime
    }
}

impl Drop for ConversationHookGenerationGuard<'_> {
    /// Restores the original dispatcher before releasing the mutex, including during unwind.
    /// 在释放互斥锁前恢复原调度器，包括栈展开期间。
    fn drop(&mut self) {
        if let Some(original) = self.original.take() {
            *self.runtime.dispatcher_mut() = original;
        }
    }
}
