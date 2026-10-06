//! Category-isolated persistent hook and UI runtimes, hook dispatch, and native hook assets.
//! 按类别隔离的持久Hook与UI运行时、Hook分发与原生Hook资源。

use super::environment::plugin_execution_package_root;
use super::process::current_plugin_target_triple;
use super::{
    VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS, VulcanPluginPersistentRuntimeKind,
    VulcanPluginRuntimeError, VulcanPluginRuntimeManager, canonical_plugin_file,
};
use crate::{VulcanPluginHookDescriptor, VulcanPluginPermission, VulcanPluginRuntimeState};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use vulcan_hooks::{HookInvocation, HookResult};

impl VulcanPluginRuntimeManager {
    /// Acquires the stable lifecycle gate for one plugin/category without awaiting under the registry lock.
    /// 获取单个插件及类别的稳定生命周期门，不在注册表锁内异步等待。
    /// The returned shared gate remains identical while any initializer or drain holds it.
    /// 只要初始化或排空仍持有返回的共享门，其身份就保持不变。
    pub(super) fn persistent_runtime_gate(
        &self,
        plugin_id: &str,
        runtime_kind: VulcanPluginPersistentRuntimeKind,
    ) -> Result<Arc<AsyncMutex<()>>, VulcanPluginRuntimeError> {
        let mut gates = self.state.persistent_runtime_gates.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Plugin lifecycle gate registry is poisoned.")
        })?;
        gates.retain(|_, gate| gate.strong_count() > 0);
        let key = (plugin_id.to_owned(), runtime_kind);
        if let Some(gate) = gates.get(&key).and_then(std::sync::Weak::upgrade) {
            return Ok(gate);
        }
        let gate = Arc::new(AsyncMutex::new(()));
        gates.insert(key, Arc::downgrade(&gate));
        Ok(gate)
    }

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
        // Resolve current eligibility before preparing the sole formal Hook instance.
        // 准备唯一正式 Hook 实例前解析当前资格。
        let snapshot = self.snapshot();
        let Some(plugin) = snapshot.plugins.get(plugin_id) else {
            return Ok(false);
        };
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || !plugin.runtime_execution_available
            || plugin.hooks.is_empty()
        {
            return Ok(false);
        }
        self.ensure_formal_hook(
            plugin_id,
            plugin.generation,
            super::formal::operation_deadline(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS)?,
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
        // Freeze the deadline before supervisor scheduling so executor delay cannot renew the budget.
        // 在监督任务调度前冻结截止时间，防止执行器延迟续期预算。
        let expires = super::formal::operation_deadline(
            hook.manifest
                .timeout_ms
                .unwrap_or(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS),
        )?;
        // Detached observation preserves the original manager and declaration until its finite result.
        // 分离观测保留原管理器及声明，直至有限结果返回。
        let manager = self.clone();
        // Freeze the exact catalog identity supplied to this public invocation.
        // 冻结本次公开调用提供的精确目录身份。
        let hook = hook.clone();
        self.supervisor_handle()?
            .spawn(async move { manager.invoke_hook_owned(&hook, invocation, expires).await })
            .await
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!("Plugin hook supervisor failed: {error}"))
            })?
    }

    /// Admits the original Hook to its formal fixed session and observes actual execution.
    /// 将原 Hook 接纳至其正式固定会话并观测实际执行。
    /// Frozen hook identity and invocation produce one typed result, even after waiter cancellation.
    /// 冻结的 Hook 身份及调用生成一个强类型结果，即使等待方已取消。
    async fn invoke_hook_owned(
        &self,
        hook: &VulcanPluginHookDescriptor,
        invocation: HookInvocation,
        expires: std::time::Instant,
    ) -> Result<HookResult, VulcanPluginRuntimeError> {
        // Preserve the declared version-one hint; actual execution uses the deadline frozen before scheduling.
        // 保留已声明版本一提示；实际执行使用调度前冻结的截止时间。
        let host_timeout_ms = hook
            .manifest
            .timeout_ms
            .unwrap_or(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS);
        self.ensure_current_hook_generation(hook)?;
        // Only host metadata supplies the optional canonical workspace.
        // 仅宿主元数据提供可选规范工作区。
        let workspace = trusted_hook_workspace_root(&invocation.context)?;
        let mut context = invocation.context;
        if let Some(workspace_root) = &workspace {
            context.metadata.insert(
                String::from("workspace_root"),
                Value::String(workspace_root.clone()),
            );
        }
        // Preserve the actual Hook version-one envelope and its registration/context/payload fields.
        // 保留实际 Hook 版本一信封及其注册、上下文、载荷字段。
        let invocation_json = serde_json::to_string(&json!({
            "registration": invocation.registration,
            "context": context,
            "payload": invocation.payload,
        }))
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to encode Vulcan Plugin hook invocation: {error}"
            ))
        })?;
        let result = self
            .invoke_formal_hook(
                hook,
                json!({"invocation_json": invocation_json, "host_timeout_ms": host_timeout_ms}),
                workspace.map(PathBuf::from),
                expires,
            )
            .await?;
        serde_json::from_value(result).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to decode Vulcan Plugin hook result: {error}"
            ))
        })
    }

    /// Prepare the exact persistent category for tests within the declared default Hook budget.
    /// 在已声明默认 Hook 预算内为测试准备精确持久类别。
    #[cfg(test)]
    pub(super) async fn ensure_enabled_plugin_runtime(
        &self,
        plugin_id: &str,
        generation: u64,
        kind: VulcanPluginPersistentRuntimeKind,
    ) -> Result<Arc<super::formal::PersistentRuntime>, VulcanPluginRuntimeError> {
        self.ensure_formal_instance(
            plugin_id,
            generation,
            kind,
            super::formal::operation_deadline(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS)?,
        )
        .await
    }

    /// Checks the current catalog at admission and publication; no historical descriptor grants execution.
    /// 在入场与发布时检查当前目录；历史描述不能授予执行资格。
    /// Returns success only for the requested enabled generation.
    /// 仅当请求代次仍启用时返回成功。
    pub(super) fn ensure_current_persistent_generation(
        &self,
        plugin_id: &str,
        generation: u64,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Published identity alone cannot admit business before the original target handoff completes.
        // 仅发布身份不能在原目标交接完成前接纳业务。
        crate::runtime::update_drain::validate_ordinary_lifecycle_admission(
            &self.state,
            plugin_id,
            crate::VulcanPluginBridgeLeaseScope::Enabled,
        )?;
        if self
            .snapshot()
            .plugins
            .get(plugin_id)
            .is_some_and(|plugin| {
                plugin.generation == generation
                    && plugin.runtime_state == VulcanPluginRuntimeState::Enabled
            })
        {
            Ok(())
        } else {
            Err(VulcanPluginRuntimeError::new(format!(
                "Plugin `{plugin_id}` lifecycle generation is stale or disabled."
            )))
        }
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
