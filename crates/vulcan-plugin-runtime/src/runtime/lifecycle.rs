//! Plugin catalog projection, exact generation guards, and host lifecycle transitions.
//! 插件目录投影、精确代次守卫与宿主生命周期转换。

use super::authorization::plugin_is_authorized;
use super::bridge::VulcanPluginBridgeLeaseScope;
use super::environment::{ensure_plugin_execution_environment, plugin_execution_package_root};
use super::{
    RetainedVulcanPluginLease, VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS,
    VULCAN_PLUGIN_PROBE_TIMEOUT_MS, VulcanPluginRuntimeError, VulcanPluginRuntimeManager,
    call_engine_blocking, parse_engine_response,
};
use crate::{
    VulcanPluginAvailability, VulcanPluginCatalogSnapshot, VulcanPluginHookDescriptor,
    VulcanPluginPackageOrigin, VulcanPluginRuntimeState, VulcanPluginToolDescriptor,
    load_vulcan_plugin_catalog,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use uuid::Uuid;

/// Host-owned plugin lifecycle event that may invoke one manifest callback.
/// 可调用清单回调的宿主自有插件生命周期事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginLifecycleEvent {
    /// Package installation completed and its private environment is ready.
    /// 插件包安装完成且私有环境已就绪。
    Install,
    /// Plugin is about to become enabled.
    /// 插件即将进入启用状态。
    Enable,
    /// Compensate a failed enable transition through the declared disable callback.
    /// 通过声明的停用回调补偿失败的启用转换。
    EnableRollback,
    /// Plugin has stopped accepting new work and is being disabled.
    /// 插件已停止接收新任务并正在停用。
    Disable,
    /// User package is about to be removed.
    /// 用户插件包即将被移除。
    Uninstall,
}

impl VulcanPluginRuntimeManager {
    /// Atomically replaces the catalog snapshot after a full package scan.
    /// 在完整扫描插件包后原子替换目录快照。
    ///
    /// # Parameters
    /// 参数
    /// The two sets contain explicit disabled and enabled plugin identifiers.
    /// 两个集合分别包含显式停用与显式启用的插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns the newly published immutable catalog snapshot.
    /// 返回新发布的不可变目录快照。
    pub fn refresh_catalog(
        &self,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        let revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut snapshot = load_vulcan_plugin_catalog(
            &self.state.vulcan_plugin_root,
            Some(&self.state.user_plugin_root),
            disabled_plugin_ids,
            enabled_plugin_ids,
            revision,
        );
        self.project_runtime_state(&mut snapshot)?;
        let snapshot = Arc::new(snapshot);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::clone(&snapshot);
        Ok(snapshot)
    }

    /// Marks one valid plugin as enabling and immediately removes any stale provider surface.
    /// 将一个有效插件标记为正在启用，并立即移除任何陈旧 Provider 工具面。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact discovered plugin identifier.
    /// `plugin_id` 是精确的已发现插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after publishing the transient lifecycle state.
    /// 在发布临时生命周期状态后返回成功。
    pub fn begin_enabling(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Enabling,
            &[
                VulcanPluginRuntimeState::Disabled,
                VulcanPluginRuntimeState::DisabledWithCleanupWarning,
            ],
        )
    }

    /// Marks one enabled plugin as draining and immediately rejects every new invocation.
    /// 将一个已启用插件标记为正在排空，并立即拒绝所有新调用。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact discovered plugin identifier.
    /// `plugin_id` 是精确的已发现插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after tools and hooks leave the enabled surface.
    /// 在工具与 Hook 离开启用面后返回成功。
    pub fn begin_draining(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Draining,
            &[
                VulcanPluginRuntimeState::Enabled,
                VulcanPluginRuntimeState::DisabledWithCleanupWarning,
            ],
        )
    }

    /// Re-enters enabling from an uncommitted draining transition during service rollback.
    /// 在服务回滚期间从尚未提交的排空转换重新进入启用中状态。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact plugin whose disable transaction must be compensated.
    /// `plugin_id` 是必须补偿其停用事务的精确插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only after atomically replacing `Draining` with `Enabling`.
    /// 仅在原子地以 `Enabling` 替换 `Draining` 后返回成功。
    pub fn rollback_draining_to_enabling(
        &self,
        plugin_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Enabling,
            &[VulcanPluginRuntimeState::Draining],
        )
    }

    /// Clears a transient lifecycle transition and rebuilds the catalog from persisted policy.
    /// 清除临时生命周期转换，并根据持久化策略重建目录。
    ///
    /// # Parameters
    /// 参数
    /// The plugin id selects the transition, policy sets are persisted overrides, and failures are unresolved cleanup diagnostics.
    /// 插件标识选择转换，策略集合是持久化覆盖，失败集合是未解决清理诊断。
    ///
    /// # Returns
    /// 返回值
    /// Returns the final immutable catalog snapshot.
    /// 返回最终不可变目录快照。
    pub fn finish_lifecycle_transition(
        &self,
        plugin_id: &str,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
        cleanup_failures: Vec<String>,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
            })?
            .remove(plugin_id);
        if cleanup_failures.is_empty() {
            self.clear_cleanup_diagnostics_if_fully_drained(plugin_id)?;
        } else {
            let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
            })?;
            diagnostics.insert(plugin_id.to_owned(), cleanup_failures);
        }
        self.refresh_catalog(disabled_plugin_ids, enabled_plugin_ids)
    }

    /// Probes dependency resolution and Lua module loading without opening a business session.
    /// 在不打开业务会话的前提下探测依赖解析与 Lua 模块加载。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact plugin to probe.
    /// `plugin_id` 是需要探测的精确插件。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only after the probe lease is also closed successfully.
    /// 仅在探测租约也成功关闭后返回成功。
    pub async fn probe_plugin(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` was not discovered."
            ))
        })?;
        if !matches!(
            plugin.availability,
            VulcanPluginAvailability::Enabled | VulcanPluginAvailability::Disabled
        ) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` is unavailable: {}",
                plugin.diagnostics.join("; ")
            )));
        }
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        if !manifest.has_lua_runtime() {
            return Ok(());
        }
        // executionRoot prevents a post-catalog package mutation from changing probe code.
        // executionRoot 防止目录扫描后的插件包变更替换探测代码。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let probe_id = Uuid::new_v4().to_string();
        let (lease_id, sid, generation) = self
            .create_managed_lease(
                &probe_id,
                &execution_root,
                &execution_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::EnablingProbe,
            )
            .await?;
        // code probes the already loaded plugin through the fixed bridge ABI.
        // code 通过固定桥接 ABI 探测已加载插件。
        let code = String::from("local bridge = require('runtime.bridge')\nreturn bridge.probe()");
        let engine = self.runtime_engine()?;
        let stable_plugin_id = plugin_id.to_owned();
        let probe_result = call_engine_blocking(Arc::clone(&engine), {
            let lease_id = lease_id.clone();
            let sid = sid.clone();
            let plugin_id = stable_plugin_id.clone();
            move |engine| {
                let raw = engine
                    .eval_system_runtime_lease_json(
                        &json!({
                            "lease_id": lease_id,
                            "sid": sid,
                            "generation": generation,
                            "code": code,
                            "args": {},
                            "timeout_ms": VULCAN_PLUGIN_PROBE_TIMEOUT_MS,
                        })
                        .to_string(),
                    )
                    .map_err(VulcanPluginRuntimeError::new)?;
                let response = parse_engine_response(&raw, "probe Vulcan Plugin runtime lease")?;
                if response.get("result").and_then(Value::as_bool) != Some(true) {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Vulcan Plugin `{plugin_id}` probe did not return true."
                    )));
                }
                Ok(())
            }
        })
        .await;
        let retained_lease = RetainedVulcanPluginLease {
            plugin_id: stable_plugin_id.clone(),
            owner_session_id: None,
            lease_id: lease_id.clone(),
            sid: sid.clone(),
            generation,
            reserved_session_slot: false,
        };
        let close_result = call_engine_blocking(engine, move |engine| {
            let raw = engine
                .close_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id,
                        "sid": sid,
                        "generation": generation,
                    })
                    .to_string(),
                )
                .map_err(VulcanPluginRuntimeError::new)?;
            parse_engine_response(&raw, "close Vulcan Plugin probe lease").map(|_| ())
        })
        .await;
        match (probe_result, close_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(probe_error), Ok(())) => Err(probe_error),
            (probe_result, Err(close_error)) => {
                let diagnostic = format!(
                    "Vulcan Plugin `{stable_plugin_id}` probe lease cleanup failed and remains retained: {close_error}"
                );
                self.retain_failed_lease(retained_lease, diagnostic.clone())?;
                let message = match probe_result {
                    Ok(()) => diagnostic,
                    Err(probe_error) => format!("{probe_error}; {diagnostic}"),
                };
                Err(VulcanPluginRuntimeError::new(message))
            }
        }
    }

    /// Returns one immutable current catalog snapshot.
    /// 返回一份不可变的当前目录快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns a reference-counted snapshot that remains stable for its consumer.
    /// 返回供消费者稳定持有的引用计数快照。
    pub fn snapshot(&self) -> Arc<VulcanPluginCatalogSnapshot> {
        self.state
            .snapshot
            .read()
            .map(|snapshot| Arc::clone(&snapshot))
            .unwrap_or_else(|poisoned| Arc::clone(&poisoned.into_inner()))
    }

    /// Invokes one optional manifest-declared lifecycle callback through a temporary bridge lease.
    /// 通过临时桥接租约调用一个可选的清单声明生命周期回调。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the exact package and `event` selects its declared callback.
    /// `plugin_id` 选择精确插件包，`event` 选择其声明的回调。
    ///
    /// # Returns
    /// 返回值
    /// Returns success when no callback is declared or after the callback and lease cleanup succeed.
    /// 未声明回调时直接成功；否则在回调与租约清理均成功后返回成功。
    pub async fn invoke_lifecycle(
        &self,
        plugin_id: &str,
        event: VulcanPluginLifecycleEvent,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        let Some(lua_runtime) = manifest.lua_runtime.as_ref() else {
            return Ok(());
        };
        let (function, event_key) = match event {
            VulcanPluginLifecycleEvent::Install => {
                (lua_runtime.lifecycle.install.clone(), "install")
            }
            VulcanPluginLifecycleEvent::Enable => (lua_runtime.lifecycle.enable.clone(), "enable"),
            VulcanPluginLifecycleEvent::EnableRollback => {
                (lua_runtime.lifecycle.disable.clone(), "enable_rollback")
            }
            VulcanPluginLifecycleEvent::Disable => {
                (lua_runtime.lifecycle.disable.clone(), "disable")
            }
            VulcanPluginLifecycleEvent::Uninstall => {
                (lua_runtime.lifecycle.uninstall.clone(), "uninstall")
            }
        };
        let Some(function) = function else {
            return Ok(());
        };
        // environmentRoot provides lifecycle-owned writable dependency and process state.
        // environmentRoot 提供生命周期持有的可写依赖与进程状态。
        let environment_root = ensure_plugin_execution_environment(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        // executionRoot pins lifecycle Lua code to the trusted package generation.
        // executionRoot 把生命周期 Lua 代码固定到可信插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        self.reserve_session_slot(plugin_id, lua_runtime.max_active_sessions)?;
        let lifecycle_id = format!("lifecycle-{event_key}-{}", Uuid::new_v4());
        let lease_result = self
            .create_managed_lease(
                &lifecycle_id,
                &execution_root,
                &environment_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::from(event),
            )
            .await;
        let (lease_id, sid, generation) = match lease_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(plugin_id);
                return Err(error);
            }
        };
        let callback_result = self
            .evaluate_bridge_function(
                &lease_id,
                &sid,
                generation,
                &function,
                json!({
                    "event": event_key,
                    "plugin_id": plugin_id,
                    "environment_root": environment_root,
                }),
                VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS,
            )
            .await
            .map(|_| ());
        let close_result = self.close_raw_lease(&lease_id, &sid, generation).await;
        self.release_session_slot(plugin_id);
        match (callback_result, close_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(callback_error), Err(close_error)) => Err(VulcanPluginRuntimeError::new(format!(
                "{callback_error}; lifecycle lease cleanup failed: {close_error}"
            ))),
        }
    }

    /// Requires one tool definition to match the current enabled plugin generation.
    /// 要求一个工具定义匹配当前已启用插件代次。
    /// `tool` is the frozen request descriptor; returns unit or an explicit stale-generation error.
    /// `tool` 是冻结的请求描述；返回空值或明确的陈旧代次错误。
    pub(super) fn ensure_current_tool_generation(
        &self,
        tool: &VulcanPluginToolDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer available.",
                tool.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != tool.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` changed lifecycle generation; retry with a new provider request.",
                tool.plugin_id
            )));
        }
        Ok(())
    }

    /// Requires one hook declaration to match the current enabled plugin generation.
    /// 要求一个 hook 声明匹配当前已启用插件代次。
    /// `hook` identifies the registered callback; returns unit only while both catalog generations match.
    /// `hook` 标识注册回调；仅在两处目录代次匹配时返回空值。
    pub(super) fn ensure_current_hook_generation(
        &self,
        hook: &VulcanPluginHookDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&hook.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer available.",
                hook.plugin_id
            ))
        })?;
        let current_hook = snapshot
            .enabled_hooks
            .get(&hook.registration_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin hook `{}` is not enabled in the current catalog.",
                    hook.registration_id
                ))
            })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != hook.plugin_generation
            || current_hook.plugin_generation != hook.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` changed lifecycle generation; retry the hook on the current runtime.",
                hook.plugin_id
            )));
        }
        Ok(())
    }

    /// Applies generations, transient lifecycle overrides, active counts, and cleanup diagnostics.
    /// 应用代次、临时生命周期覆盖、活动计数与清理诊断。
    /// Mutates the scanned `snapshot` using this manager's state; returns unit or a lock diagnostic.
    /// 使用本管理器状态修改扫描所得的 `snapshot`；返回空值或锁诊断。
    fn project_runtime_state(
        &self,
        snapshot: &mut VulcanPluginCatalogSnapshot,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let previous = self.snapshot();
        let active_counts = self.state.active_session_counts.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
        })?;
        let overrides = self.state.lifecycle_overrides.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
        })?;
        let cleanup_diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let mut generations = self.state.plugin_generations.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin generation lock is poisoned.")
        })?;
        let runtime_engine_available = self
            .state
            .engine
            .read()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin runtime engine lock is poisoned.")
            })?
            .is_some();
        for plugin in snapshot.plugins.values_mut() {
            let requires_runtime = plugin
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.has_lua_runtime());
            plugin.runtime_execution_available = !requires_runtime || runtime_engine_available;
            if requires_runtime && !runtime_engine_available {
                plugin.diagnostics.push(String::from(
                    "The plugin package is valid, but its Lua execution runtime is unavailable; read-only skills and agents remain active.",
                ));
            }
            let generation = generations.entry(plugin.id.clone()).or_insert(1);
            let changed = previous.plugins.get(&plugin.id).is_some_and(|prior| {
                prior.version != plugin.version
                    || prior.content_fingerprint != plugin.content_fingerprint
                    || prior.desired_enabled != plugin.desired_enabled
                    || prior.availability != plugin.availability
            });
            if changed {
                *generation = generation.saturating_add(1);
            }
            plugin.generation = *generation;
            plugin.active_session_count = active_counts.get(&plugin.id).copied().unwrap_or(0);
            for tool in &mut plugin.tools {
                tool.plugin_generation = *generation;
            }
            for hook in &mut plugin.hooks {
                hook.plugin_generation = *generation;
            }
            for server in &mut plugin.mcp_servers {
                server.plugin_generation = *generation;
            }
            if let Some(runtime_state) = overrides.get(&plugin.id).copied() {
                plugin.runtime_state = runtime_state;
            } else if plugin.runtime_state == VulcanPluginRuntimeState::Disabled
                && cleanup_diagnostics.contains_key(&plugin.id)
            {
                plugin.runtime_state = VulcanPluginRuntimeState::DisabledWithCleanupWarning;
            }
            if plugin.origin == VulcanPluginPackageOrigin::UserInstalled {
                match plugin_is_authorized(&self.state.plugin_state_root, plugin) {
                    Ok(true) => {}
                    Ok(false) => {
                        plugin.runtime_state = VulcanPluginRuntimeState::Unavailable;
                        plugin.diagnostics.push(String::from(
                            "User-installed plugin requires authorization for its current version, content fingerprint, and complete capability set.",
                        ));
                    }
                    Err(error) => {
                        plugin.runtime_state = VulcanPluginRuntimeState::Unavailable;
                        plugin.diagnostics.push(error);
                    }
                }
            }
            if let Some(diagnostics) = cleanup_diagnostics.get(&plugin.id) {
                plugin.diagnostics.extend(diagnostics.iter().cloned());
            }
        }
        rebuild_enabled_surface(snapshot);
        Ok(())
    }

    /// Begins one transient lifecycle state and publishes a generation-bumped snapshot.
    /// 开始一个临时生命周期状态，并发布代次递增的快照。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the package, `runtime_state` is the target, and source states are exact.
    /// `plugin_id` 选择插件包，`runtime_state` 是目标状态，源状态集合必须精确匹配。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after atomically validating and publishing the transition.
    /// 原子校验并发布转换后返回成功。
    fn begin_lifecycle_transition(
        &self,
        plugin_id: &str,
        runtime_state: VulcanPluginRuntimeState,
        allowed_source_states: &[VulcanPluginRuntimeState],
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        let mut snapshot = (*self.snapshot()).clone();
        let plugin = snapshot.plugins.get_mut(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` was not discovered."
            ))
        })?;
        if !allowed_source_states.contains(&plugin.runtime_state) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` cannot enter state `{runtime_state:?}` from state `{:?}`.",
                plugin.runtime_state
            )));
        }
        if !matches!(
            plugin.availability,
            VulcanPluginAvailability::Enabled | VulcanPluginAvailability::Disabled
        ) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` cannot enter a lifecycle transition while unavailable."
            )));
        }
        let generation = {
            let mut generations = self.state.plugin_generations.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin generation lock is poisoned.")
            })?;
            let generation = generations.entry(plugin_id.to_owned()).or_insert(1);
            *generation = generation.saturating_add(1);
            *generation
        };
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
            })?
            .insert(plugin_id.to_owned(), runtime_state);
        plugin.runtime_state = runtime_state;
        plugin.generation = generation;
        for tool in &mut plugin.tools {
            tool.plugin_generation = generation;
        }
        for hook in &mut plugin.hooks {
            hook.plugin_generation = generation;
        }
        for server in &mut plugin.mcp_servers {
            server.plugin_generation = generation;
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        rebuild_enabled_surface(&mut snapshot);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }
}

/// Rebuilds provider-visible tools and prompt contributions from enabled plugin descriptors.
/// 根据已启用插件描述重新构建 Provider 可见工具与提示词贡献。
/// Replaces the derived surfaces of `snapshot` in place and returns no value.
/// 原位替换 `snapshot` 的派生暴露面，不返回值。
fn rebuild_enabled_surface(snapshot: &mut VulcanPluginCatalogSnapshot) {
    snapshot.enabled_tools.clear();
    snapshot.enabled_hooks.clear();
    snapshot.enabled_mcp_servers.clear();
    snapshot.prompt_contributions.clear();
    snapshot.enabled_skills.clear();
    snapshot.enabled_agents.clear();
    for plugin in snapshot.plugins.values() {
        if plugin.availability != VulcanPluginAvailability::Enabled
            || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
        {
            continue;
        }
        for skill in &plugin.skills {
            snapshot
                .enabled_skills
                .insert(skill.name.clone(), skill.clone());
        }
        for agent in &plugin.agents {
            snapshot
                .enabled_agents
                .insert(agent.id.clone(), agent.clone());
        }
        for server in &plugin.mcp_servers {
            snapshot.enabled_mcp_servers.insert(
                format!("{}:{}", server.plugin_id, server.id),
                server.clone(),
            );
        }
        if !plugin.runtime_execution_available {
            continue;
        }
        for tool in &plugin.tools {
            snapshot.enabled_tools.insert(tool.id.clone(), tool.clone());
        }
        for hook in &plugin.hooks {
            snapshot
                .enabled_hooks
                .insert(hook.registration_id.clone(), hook.clone());
        }
        snapshot
            .prompt_contributions
            .extend(plugin.prompt_contributions.iter().cloned());
    }
}

#[cfg(test)]
mod tests;
