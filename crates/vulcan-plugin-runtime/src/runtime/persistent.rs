//! Category-isolated persistent hook and UI runtimes, hook dispatch, and native hook assets.
//! 按类别隔离的持久Hook与UI运行时、Hook分发与原生Hook资源。

use super::bridge::VulcanPluginBridgeLeaseScope;
use super::environment::plugin_execution_package_root;
use super::lease::{call_engine_blocking, parse_engine_response, parse_plugin_result};
use super::process::current_plugin_target_triple;
use super::{
    ManagedVulcanPluginHookRuntime, VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS,
    VulcanPluginPersistentRuntimeKind, VulcanPluginRuntimeError, VulcanPluginRuntimeManager,
    canonical_plugin_file, lock_hook_runtimes,
};
use crate::{VulcanPluginHookDescriptor, VulcanPluginPermission, VulcanPluginRuntimeState};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex as AsyncMutex;
use vulcan_hooks::{HookInvocation, HookResult};

/// Maximum Lua execution time used while shutting down one persistent hook runtime.
/// 关闭单个持久 hook 运行时时使用的 Lua 最大执行时长。
const VULCAN_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS: u64 = 5_000;

impl VulcanPluginRuntimeManager {
    /// Resolves a native binary directory requested by one successfully completed plugin hook.
    /// 解析一个成功完成插件 Hook 请求的原生二进制目录。
    ///
    /// # Parameters
    /// 参数
    /// `registration_id` binds the request to an enabled hook and `binary_id` selects its resource.
    /// `registration_id` 把请求绑定到已启用 Hook，`binary_id` 选择其资源。
    ///
    /// # Returns
    /// 返回值
    /// Returns the canonical directory containing the exact current-target binary.
    /// 返回包含当前精确目标二进制文件的规范目录。
    pub fn hook_binary_directory(
        &self,
        registration_id: &str,
        binary_id: &str,
    ) -> Result<PathBuf, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let hook = snapshot.enabled_hooks.get(registration_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin hook `{registration_id}` is not enabled."
            ))
        })?;
        let plugin = snapshot.plugins.get(&hook.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is not installed.",
                hook.plugin_id
            ))
        })?;
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has no valid manifest.",
                hook.plugin_id
            ))
        })?;
        if !manifest
            .permissions
            .contains(&VulcanPluginPermission::ProcessBinary)
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` did not declare process_binary capability.",
                hook.plugin_id
            )));
        }
        let binary = manifest
            .execution
            .native_binaries
            .iter()
            .find(|entry| entry.id == binary_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{}` does not declare binary `{binary_id}`.",
                    hook.plugin_id
                ))
            })?;
        let target = current_plugin_target_triple().map_err(VulcanPluginRuntimeError::new)?;
        let relative = binary.targets.get(target).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` binary `{binary_id}` has no asset for target `{target}`.",
                hook.plugin_id
            ))
        })?;
        // executionRoot resolves the host-owned directory injected for this logical binary id.
        // executionRoot 解析当前逻辑二进制标识对应的宿主持有目录。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let executable = canonical_plugin_file(&execution_root, relative, "binary")
            .map_err(VulcanPluginRuntimeError::new)?;
        executable.parent().map(Path::to_path_buf).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin binary `{}` has no containing directory.",
                executable.display()
            ))
        })
    }

    /// Prewarms one latency-sensitive hook runtime for every enabled plugin that contributes hooks.
    /// 为每个提供 Hook 的已启用插件预热一个延迟敏感 Hook 运行时。
    ///
    /// Returns the deterministically ordered plugin identifiers whose runtimes are ready.
    /// 返回运行时已就绪的确定性排序插件标识列表。
    /// Reads this manager's current catalog without extra arguments; returns the exact warmed plugin identifiers or the first preparation error.
    /// 读取本管理器当前目录，无额外参数；返回精确预热插件标识或首个准备错误。
    pub async fn prewarm_enabled_plugin_runtimes(
        &self,
    ) -> Result<Vec<String>, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin_ids = snapshot
            .plugins
            .values()
            .filter(|plugin| {
                plugin.runtime_state == VulcanPluginRuntimeState::Enabled
                    && plugin.runtime_execution_available
                    && !plugin.hooks.is_empty()
            })
            .map(|plugin| plugin.id.clone())
            .collect::<BTreeSet<_>>();
        let mut ready_plugin_ids = Vec::with_capacity(plugin_ids.len());
        for plugin_id in &plugin_ids {
            if self.prewarm_plugin_runtime(plugin_id).await? {
                ready_plugin_ids.push(plugin_id.clone());
            }
        }
        Ok(ready_plugin_ids)
    }

    /// Prewarms the persistent hook runtime for one enabled plugin when it contributes hooks.
    /// 当指定的已启用插件提供 Hook 时，预热其持久 Hook 运行时。
    ///
    /// Returns true when an eligible enabled plugin exists and its runtime is ready, otherwise false.
    /// 当存在符合条件的已启用插件且其运行时已就绪时返回 true，否则返回 false。
    /// plugin_id selects the current plugin; returns whether its eligible Hook runtime was prepared.
    /// plugin_id选择当前插件；返回其符合条件的Hook运行时是否已准备。
    pub async fn prewarm_plugin_runtime(
        &self,
        plugin_id: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let Some(plugin) = snapshot.plugins.get(plugin_id) else {
            return Ok(false);
        };
        let eligible = plugin.runtime_state == VulcanPluginRuntimeState::Enabled
            && plugin.runtime_execution_available
            && !plugin.hooks.is_empty();
        if !eligible {
            return Ok(false);
        }
        let generation = plugin.generation;
        self.ensure_enabled_plugin_runtime(
            plugin_id,
            generation,
            VulcanPluginPersistentRuntimeKind::Hook,
        )
        .await?;
        Ok(true)
    }

    /// Invokes one validated hook registration with the supplied immutable invocation context.
    /// 使用给定的不可变调用上下文执行一个已验证的 Hook 注册项。
    ///
    /// The hook runtime is reused for the enabled plugin generation and is closed on disable.
    /// Hook 运行时会在插件启用代次内复用，并在停用时关闭。
    /// hook selects the frozen callback and invocation supplies host context; returns a typed HookResult or a lifecycle/engine error.
    /// hook选择冻结回调，invocation提供宿主上下文；返回强类型HookResult或生命周期/引擎错误。
    pub async fn invoke_hook(
        &self,
        hook: &VulcanPluginHookDescriptor,
        invocation: HookInvocation,
    ) -> Result<HookResult, VulcanPluginRuntimeError> {
        self.ensure_current_hook_generation(hook)?;
        let runtime = self.ensure_hook_runtime(hook).await?;
        let _operation = runtime.operation_gate.lock().await;
        if runtime.closing.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` hook runtime is closing.",
                hook.plugin_id
            )));
        }
        self.ensure_current_hook_generation(hook)?;
        if runtime.plugin_generation != hook.plugin_generation {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` hook runtime belongs to a stale lifecycle generation.",
                hook.plugin_id
            )));
        }
        // trusted_workspace_root is the normalized host path authorized for Lua bridge operations.
        // trusted_workspace_root 是授权给 Lua bridge 操作的规范化宿主路径。
        let trusted_workspace_root = trusted_hook_workspace_root(&invocation.context)?;
        // lua_context prevents the original verbatim workspace metadata from entering the Lua invocation payload.
        // lua_context 防止原始 verbatim 工作区元数据进入 Lua 调用载荷。
        let mut lua_context = invocation.context;
        // workspace_root borrows the already-normalized Lua boundary value when this hook has workspace scope.
        // workspace_root 在当前 Hook 具有工作区作用域时借用已完成规范化的 Lua 边界值。
        if let Some(workspace_root) = trusted_workspace_root.as_ref() {
            lua_context.metadata.insert(
                String::from("workspace_root"),
                Value::String(workspace_root.clone()),
            );
        }
        let invocation_json = serde_json::to_string(&json!({
            "registration": invocation.registration,
            "context": lua_context,
            "payload": invocation.payload,
        }))
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to encode Vulcan Plugin hook invocation: {error}"
            ))
        })?;
        let function = hook.manifest.function.clone();
        // code dispatches the validated hook function through the fixed bridge ABI.
        // code 通过固定桥接 ABI 调度已校验的 hook 函数。
        let code =
            String::from("local bridge = require('runtime.bridge')\nreturn bridge.invoke(args)");
        let lease_id = runtime.lease_id.clone();
        let generation = runtime.generation;
        let host_timeout_ms = hook
            .manifest
            .timeout_ms
            .unwrap_or(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS);
        let engine = self.runtime_engine()?;
        let sid = runtime.sid.clone();
        let result = call_engine_blocking(engine, move |engine| {
            let raw = engine
                .eval_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id,
                        "sid": sid,
                        "generation": generation,
                        "code": code,
                        "args": {
                            "function_name": function,
                            "workspace_root": trusted_workspace_root,
                            "plugin_args": {
                                "invocation_json": invocation_json,
                                "host_timeout_ms": host_timeout_ms,
                            },
                        },
                        "timeout_ms": host_timeout_ms,
                    })
                    .to_string(),
                )
                .map_err(VulcanPluginRuntimeError::new)?;
            let response = parse_engine_response(&raw, "evaluate Vulcan Plugin hook lease")?;
            parse_plugin_result(&response)
        })
        .await?;
        serde_json::from_value(result).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to decode Vulcan Plugin hook result: {error}"
            ))
        })
    }

    /// Resolves or creates the persistent Lua lease for one current plugin generation.
    /// 解析或创建一个当前插件代次使用的持久 Lua 租约。
    /// hook provides the exact plugin generation; returns its shared Hook runtime or a stale-generation error.
    /// hook提供精确插件代次；返回其共享Hook运行时或陈旧代次错误。
    async fn ensure_hook_runtime(
        &self,
        hook: &VulcanPluginHookDescriptor,
    ) -> Result<Arc<ManagedVulcanPluginHookRuntime>, VulcanPluginRuntimeError> {
        self.ensure_current_hook_generation(hook)?;
        self.ensure_enabled_plugin_runtime(
            &hook.plugin_id,
            hook.plugin_generation,
            VulcanPluginPersistentRuntimeKind::Hook,
        )
        .await
    }

    /// Resolves or creates one category-isolated persistent Lua lease for an enabled plugin.
    /// 为一个已启用插件解析或创建按类别隔离的持久 Lua 租约。
    /// plugin_id, plugin_generation, and runtime_kind select the exact isolated runtime; returns its shared handle or a preparation error.
    /// plugin_id、plugin_generation与runtime_kind选择精确隔离运行时；返回共享句柄或准备错误。
    pub(super) async fn ensure_enabled_plugin_runtime(
        &self,
        plugin_id: &str,
        plugin_generation: u64,
        runtime_kind: VulcanPluginPersistentRuntimeKind,
    ) -> Result<Arc<ManagedVulcanPluginHookRuntime>, VulcanPluginRuntimeError> {
        let _initialization = match runtime_kind {
            VulcanPluginPersistentRuntimeKind::Hook => {
                self.state.hook_runtime_initialization_gate.lock().await
            }
            VulcanPluginPersistentRuntimeKind::UiAction => {
                self.state
                    .ui_action_runtime_initialization_gate
                    .lock()
                    .await
            }
        };
        let runtimes = match runtime_kind {
            VulcanPluginPersistentRuntimeKind::Hook => &self.state.hook_runtimes,
            VulcanPluginPersistentRuntimeKind::UiAction => &self.state.ui_action_runtimes,
        };
        let existing_runtime = {
            let runtimes = lock_hook_runtimes(runtimes)?;
            runtimes.get(plugin_id).cloned()
        };
        if let Some(runtime) = existing_runtime {
            if runtime.plugin_generation == plugin_generation
                && !runtime.closing.load(Ordering::Acquire)
            {
                return Ok(runtime);
            }
            if let Some(diagnostic) = self
                .close_persistent_runtime(Arc::clone(&runtime), runtime_kind)
                .await?
            {
                self.record_cleanup_failure(plugin_id, diagnostic)?;
            }
            lock_hook_runtimes(runtimes)?.remove(plugin_id);
        }
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer available.",
                plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` persistent {} runtime belongs to a stale or disabled lifecycle generation.",
                runtime_kind.label()
            )));
        }
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` has no valid manifest.",
                plugin_id
            ))
        })?;
        // executionRoot pins the category-isolated persistent runtime to one trusted package generation.
        // executionRoot 把按类别隔离的持久运行时固定到一个可信插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let (lease_id, sid, generation) = self
            .create_managed_lease(
                runtime_kind.lease_id(),
                &execution_root,
                &execution_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::Enabled,
            )
            .await?;
        let runtime = Arc::new(ManagedVulcanPluginHookRuntime {
            plugin_id: plugin_id.to_string(),
            plugin_generation,
            lease_id,
            sid,
            generation,
            manifest,
            operation_gate: AsyncMutex::new(()),
            closing: AtomicBool::new(false),
        });
        lock_hook_runtimes(runtimes)?.insert(plugin_id.to_string(), Arc::clone(&runtime));
        Ok(runtime)
    }

    /// Stops one category-isolated persistent runtime and closes its System lease after optional Hook shutdown.
    /// 停止一个按类别隔离的持久运行时，在可选Hook关闭之后关闭其系统租约。
    /// runtime and runtime_kind identify the shared lease; returns an optional graceful-shutdown diagnostic after close, or an explicit failure.
    /// runtime与runtime_kind标识共享租约；关闭后返回可选优雅关闭诊断，或明确失败。
    pub(super) async fn close_persistent_runtime(
        &self,
        runtime: Arc<ManagedVulcanPluginHookRuntime>,
        runtime_kind: VulcanPluginPersistentRuntimeKind,
    ) -> Result<Option<String>, VulcanPluginRuntimeError> {
        runtime.closing.store(true, Ordering::Release);
        let _operation = runtime.operation_gate.lock().await;
        let shutdown_result = if runtime_kind == VulcanPluginPersistentRuntimeKind::Hook
            && let Some(function) = runtime
                .manifest
                .lua_runtime
                .as_ref()
                .and_then(|lua_runtime| lua_runtime.hook_shutdown_function.clone())
        {
            let plugin_id = runtime.plugin_id.clone();
            // code dispatches graceful hook shutdown through the fixed bridge ABI.
            // code 通过固定桥接 ABI 调度 hook 的优雅关闭。
            let code = String::from(
                "local bridge = require('runtime.bridge')\nreturn bridge.invoke(args)",
            );
            let lease_id = runtime.lease_id.clone();
            let sid = runtime.sid.clone();
            let generation = runtime.generation;
            let engine = self.runtime_engine()?;
            call_engine_blocking(engine, move |engine| {
                let raw = engine
                    .eval_system_runtime_lease_json(
                        &json!({
                            "lease_id": lease_id,
                            "sid": sid,
                            "generation": generation,
                            "code": code,
                            "args": {
                                "function_name": function,
                                "plugin_args": {
                                    "payload_json": json!({
                                        "event": "hook_runtime_shutdown",
                                        "plugin_id": plugin_id,
                                    })
                                    .to_string(),
                                    "host_timeout_ms": VULCAN_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS,
                                },
                            },
                            "timeout_ms": VULCAN_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS,
                        })
                        .to_string(),
                    )
                    .map_err(VulcanPluginRuntimeError::new)?;
                let response = parse_engine_response(&raw, "shutdown Vulcan Plugin hook lease")?;
                parse_plugin_result(&response).map(|_| ())
            })
            .await
        } else {
            Ok(())
        };
        self.close_raw_lease(&runtime.lease_id, &runtime.sid, runtime.generation)
            .await?;
        Ok(shutdown_result.err().map(|error| {
            format!(
                "Plugin `{}` persistent {} lease closed after graceful shutdown failed: {error}",
                runtime.plugin_id,
                runtime_kind.label()
            )
        }))
    }
}

/// Resolves one host-owned per-invocation workspace root from generic hook context metadata.
/// 从通用 Hook 上下文元数据解析单个宿主持有的每次调用工作区根。
///
/// # Parameters
/// 参数
/// `context` is the immutable host hook context delivered to the plugin runtime.
/// `context` 是传递给插件运行时的不可变宿主 Hook 上下文。
///
/// # Returns
/// 返回值
/// Returns no override when the host did not supply one, or one canonical existing directory.
/// 宿主未提供时不返回覆盖；否则返回一个规范化且已存在的目录。
fn trusted_hook_workspace_root(
    context: &vulcan_hooks::HookContext,
) -> Result<Option<String>, VulcanPluginRuntimeError> {
    let Some(value) = context.metadata.get("workspace_root") else {
        return Ok(None);
    };
    let raw_workspace_root = value.as_str().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Hook context workspace_root metadata must be a string.")
    })?;
    let workspace_root = PathBuf::from(raw_workspace_root)
        .canonicalize()
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve Hook context workspace root: {error}"
            ))
        })?;
    if !workspace_root.is_dir() {
        return Err(VulcanPluginRuntimeError::new(
            "Hook context workspace root is not a directory.",
        ));
    }
    Ok(Some(vulcan_runtime_assets::native_host_path_text(
        &workspace_root.to_string_lossy(),
    )))
}

#[cfg(test)]
mod tests;
