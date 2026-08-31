//! Manifest-declared business actions with caller policy, schema validation, and lease cleanup.
//! 带调用方策略、Schema校验与租约清理的清单声明业务Action。

use super::bridge::VulcanPluginBridgeLeaseScope;
use super::environment::plugin_execution_package_root;
use super::lease::required_lua_runtime;
use super::{
    RetainedVulcanPluginLease, VulcanPluginPersistentRuntimeKind, VulcanPluginRuntimeError,
    VulcanPluginRuntimeManager, canonical_plugin_file, read_plugin_json_state,
};
use crate::VulcanPluginRuntimeState;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use uuid::Uuid;

/// One host or WebUI invocation of a manifest-declared plugin business action.
/// 一次由宿主或 WebUI 发起的清单声明插件业务 action 调用。
#[derive(Debug, Clone)]
pub struct VulcanPluginActionRequest {
    /// Exact owning plugin identifier.
    /// 精确的所属插件标识。
    pub plugin_id: String,
    /// Exact plugin-local action identifier.
    /// 精确的插件内 action 标识。
    pub action_id: String,
    /// Raw action arguments validated against the manifest schema.
    /// 根据清单 Schema 校验的原始 action 参数。
    pub arguments: Value,
    /// Caller class enforced against the action declaration.
    /// 根据 action 声明执行校验的调用方类别。
    pub caller: crate::VulcanPluginActionCaller,
    /// Canonical workspace root exposed to a workspace-bound temporary bridge lease.
    /// 暴露给工作区绑定临时桥接租约的规范化工作区根目录。
    pub workspace_root: PathBuf,
    /// Whether the host authorizes reuse of the enabled plugin's persistent global runtime.
    /// 宿主是否授权复用已启用插件的持久全局运行时。
    pub reuse_enabled_runtime: bool,
}

impl VulcanPluginRuntimeManager {
    /// Invokes one enabled manifest-declared plugin action through its authorized persistent or temporary lease.
    /// 通过获授权的持久或临时租约调用已启用且由清单声明的插件Action。
    ///
    /// # Parameters
    /// 参数
    /// `request` binds plugin identity, action identity, caller, arguments, and workspace root.
    /// `request` 绑定插件身份、action 身份、调用方、参数与工作区根。
    ///
    /// # Returns
    /// 返回值
    /// Returns the schema-validated action result after persistent dispatch or temporary-lease cleanup.
    /// 在持久调度或临时租约清理后返回通过 Schema 校验的 action 结果。
    pub async fn invoke_action(
        &self,
        request: VulcanPluginActionRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&request.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is not installed.",
                request.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is not enabled.",
                request.plugin_id
            )));
        }
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has no valid manifest.",
                request.plugin_id
            ))
        })?;
        let lua_runtime = required_lua_runtime(&manifest)?;
        let action = lua_runtime
            .actions
            .iter()
            .find(|action| action.id == request.action_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{}` does not declare action `{}`.",
                    request.plugin_id, request.action_id
                ))
            })?;
        if !action.callers.contains(&request.caller) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin action `{}` does not authorize caller `{:?}`.",
                action.id, request.caller
            )));
        }
        if request.reuse_enabled_runtime
            && request.caller != crate::VulcanPluginActionCaller::PluginUi
        {
            return Err(VulcanPluginRuntimeError::new(
                "Persistent enabled-plugin action runtimes are reserved for global plugin UI calls.",
            ));
        }
        // executionRoot pins action code and schemas to one authorized package generation.
        // executionRoot 把 Action 代码与 Schema 固定到一个已授权插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        validate_plugin_action_value(
            &execution_root,
            action.input_schema_file.as_deref(),
            &request.arguments,
            "action input",
        )?;
        let workspace_root = request.workspace_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to resolve workspace root: {error}"))
        })?;
        if request.reuse_enabled_runtime {
            let plugin_generation = plugin.generation;
            let runtime = self
                .ensure_enabled_plugin_runtime(
                    &plugin.id,
                    plugin_generation,
                    VulcanPluginPersistentRuntimeKind::UiAction,
                )
                .await?;
            let _operation = runtime.operation_gate.lock().await;
            if runtime.closing.load(Ordering::Acquire)
                || runtime.plugin_generation != plugin_generation
            {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin `{}` persistent runtime is closing or stale.",
                    plugin.id
                )));
            }
            let result = self
                .evaluate_bridge_function(
                    &runtime.lease_id,
                    &runtime.sid,
                    runtime.generation,
                    &action.function,
                    json!({ "arguments": request.arguments }),
                    action.timeout_ms,
                )
                .await?;
            validate_plugin_action_value(
                &execution_root,
                action.output_schema_file.as_deref(),
                &result,
                "action output",
            )?;
            return Ok(result);
        }
        self.reserve_session_slot(&plugin.id, lua_runtime.max_active_sessions)?;
        let action_lease_id = format!("action-{}", Uuid::new_v4());
        let create_result = self
            .create_managed_lease(
                &action_lease_id,
                &execution_root,
                &workspace_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::Enabled,
            )
            .await;
        let (lease_id, sid, generation) = match create_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(&plugin.id);
                return Err(error);
            }
        };
        let invocation_result = self
            .evaluate_bridge_function(
                &lease_id,
                &sid,
                generation,
                &action.function,
                json!({ "arguments": request.arguments }),
                action.timeout_ms,
            )
            .await;
        let close_result = self.close_raw_lease(&lease_id, &sid, generation).await;
        match close_result {
            Ok(()) => self.release_session_slot(&plugin.id),
            Err(close_error) => {
                let diagnostic = format!(
                    "Vulcan plugin action lease `{lease_id}` cleanup failed and remains retained: {close_error}"
                );
                self.retain_failed_lease(
                    RetainedVulcanPluginLease {
                        plugin_id: plugin.id.clone(),
                        owner_session_id: None,
                        lease_id,
                        sid,
                        generation,
                        reserved_session_slot: true,
                    },
                    diagnostic.clone(),
                )?;
                return Err(VulcanPluginRuntimeError::new(match invocation_result {
                    Ok(_) => diagnostic,
                    Err(invoke_error) => format!("{invoke_error}; {diagnostic}"),
                }));
            }
        }
        let result = invocation_result?;
        validate_plugin_action_value(
            &execution_root,
            action.output_schema_file.as_deref(),
            &result,
            "action output",
        )?;
        Ok(result)
    }
}

/// Validates one plugin action value when its manifest declares a schema.
/// 当插件清单声明 Schema 时校验一个 action 值。
///
/// # Parameters
/// 参数
/// `package_root` owns the optional schema path, `value` is untrusted, and `label` names the phase.
/// `package_root` 持有可选 Schema 路径，`value` 不可信，`label` 命名校验阶段。
///
/// # Returns
/// 返回值
/// Returns unit when no schema is declared or the value satisfies it; a missing declared file remains an error.
/// 未声明Schema或值满足声明时返回空值；已声明文件缺失仍是错误。
fn validate_plugin_action_value(
    package_root: &Path,
    schema_file: Option<&str>,
    value: &Value,
    label: &str,
) -> Result<(), VulcanPluginRuntimeError> {
    let Some(schema_file) = schema_file else {
        return Ok(());
    };
    let schema_path = canonical_plugin_file(package_root, schema_file, label)
        .map_err(VulcanPluginRuntimeError::new)?;
    let schema = read_plugin_json_state(&schema_path, label)?;
    crate::catalog::validate_json_schema_instance(&schema, value, label)
        .map_err(VulcanPluginRuntimeError::new)
}
