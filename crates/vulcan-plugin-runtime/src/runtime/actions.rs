//! Manifest-declared business actions with caller policy, schema validation, and lease cleanup.
//! 带调用方策略、Schema校验与租约清理的清单声明业务Action。

use super::action_status::{ACTION_RECORD, ActionAdmission, VulcanPluginActionIdentity};
use super::environment::plugin_execution_package_root;
use super::execution::ActionExecution;
use super::lease::required_lua_runtime;
use super::{
    VulcanPluginPersistentRuntimeKind, VulcanPluginRuntimeError, VulcanPluginRuntimeManager,
    canonical_plugin_file, read_plugin_json_state,
};
use crate::VulcanPluginRuntimeState;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
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
        self.invoke_action_checked(
            request,
            None,
            VulcanPluginActionIdentity {
                client_id: self.state.manager_id.clone(),
                invocation_id: Uuid::new_v4().to_string(),
            },
        )
        .await
    }

    /// Invokes a UI action only for the exact incarnation that supplied its module.
    /// 仅为提供界面模块的精确运行实例执行界面 Action。
    /// Identity comes from the authenticated module projection; stale pages must reload.
    /// 身份来自已认证模块投影；陈旧页面必须重新加载。
    pub async fn invoke_ui_action(
        &self,
        request: VulcanPluginActionRequest,
        runtime_identity: &str,
        identity: VulcanPluginActionIdentity,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        self.invoke_action_checked(request, Some(runtime_identity), identity)
            .await
    }

    /// Freezes generation and deadline before spawning the cancellation-safe owner.
    /// 启动可安全应对取消的所有者前冻结代次与截止时间。
    /// Host calls have no UI identity; mounted UI calls must provide their exact projection identity.
    /// 宿主调用没有界面身份；已挂载界面调用必须提供其精确投影身份。
    async fn invoke_action_checked(
        &self,
        request: VulcanPluginActionRequest,
        runtime_identity: Option<&str>,
        identity: VulcanPluginActionIdentity,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let entered = tokio::time::Instant::now();
        let snapshot = self.snapshot();
        let plugin = snapshot
            .plugins
            .get(&request.plugin_id)
            .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin action owner is unavailable."))?;
        let plugin_generation = plugin.generation;
        if runtime_identity.is_some_and(|identity| {
            identity != format!("{}:{}", snapshot.service_epoch, plugin_generation)
        }) {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin UI runtime changed; reload the plugin page before invoking actions.",
            ));
        }
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Plugin action manifest is unavailable.")
        })?;
        let lua = required_lua_runtime(manifest)?;
        let action = lua
            .actions
            .iter()
            .find(|action| action.id == request.action_id)
            .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin action is not declared."))?;
        let permit = self.admit_action(&plugin.id, lua.max_active_sessions)?;
        let deadline = entered
            .checked_add(std::time::Duration::from_millis(action.timeout_ms))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin action timeout exceeds the host clock range.")
            })?;
        let cancellable = action.effect == crate::VulcanPluginActionEffect::Read;
        let record = self.register_action(ActionAdmission {
            identity,
            plugin_id: plugin.id.clone(),
            action_id: action.id.clone(),
            runtime_identity: format!("{}:{}", snapshot.service_epoch, plugin_generation),
            entered: entered.into_std(),
            deadline: deadline.into_std(),
            cancellable,
            permit,
        })?;
        let (waiter, cancelled) = tokio::sync::oneshot::channel();
        let mut waiter = Some(waiter);
        // Submitted mutations survive waiter disconnect; only the original deadline limits admission.
        // 已提交变更在等待方断开后继续；仅原始截止时间限制入场。
        let mutation_waiter = if cancellable { None } else { waiter.take() };
        // The supervisor retains registered execution and physical cleanup after an HTTP waiter is dropped.
        // HTTP 等待方被丢弃后，监督任务仍保留已登记执行及物理清理。
        let manager = self.clone();
        // Own completion before spawning so an unpolled task cannot leak active admission.
        // 派发之前拥有完成记录，避免未轮询任务泄漏活动入场。
        let completion = super::action_status::ActionCompletionGuard(record.clone());
        let task = self.supervisor_handle()?.spawn(async move {
            let _completion = completion;
            let _mutation_waiter = mutation_waiter;
            ACTION_RECORD
                .scope(record.clone(), async {
                    let result = manager
                        .invoke_action_owned(
                            request,
                            plugin_generation,
                            ActionExecution {
                                deadline,
                                cancelled,
                                record: record.clone(),
                            },
                        )
                        .await;
                    record.finish(result.is_ok());
                    result
                })
                .await
        });
        let result = tokio::time::timeout_at(deadline, task).await;
        drop(waiter);
        result.map_err(|_| VulcanPluginRuntimeError::new("Plugin action wait timed out; started work retains its execution permit until cleanup. Do not automatically replay mutations."))?
            .map_err(|error| VulcanPluginRuntimeError::new(format!("Plugin action supervisor failed: {error}")))?
    }

    /// Owns the complete action lifecycle through schema validation and lease cleanup.
    /// 持有完整 Action 生命周期，直至 Schema 校验与租约清理完成。
    /// The authorized request returns its validated result or a retained cleanup diagnostic.
    /// 获授权请求返回校验结果，或返回已保留的清理诊断。
    async fn invoke_action_owned(
        &self,
        mut request: VulcanPluginActionRequest,
        expected_generation: u64,
        mut execution: ActionExecution,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        execution.remaining_ms()?;
        self.ensure_current_persistent_generation(&request.plugin_id, expected_generation)?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&request.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is not installed.",
                request.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != expected_generation
        {
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
        // Only declared null time markers are replaced; explicit historical timestamps remain exact.
        // 仅替换已声明的空时间标记；明确历史时间戳保持精确。
        let frozen_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| VulcanPluginRuntimeError::new(format!("Invalid host clock: {error}")))?
            .as_millis();
        if let Some(arguments) = request.arguments.as_object_mut() {
            for argument in &action.snapshot_time_arguments {
                if arguments.get(argument).is_some_and(Value::is_null) {
                    arguments.insert(argument.clone(), json!(frozen_now));
                }
            }
        }
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
            execution.record.phase("initialization_wait");
            // Cancel only this waiter; the instance registry owns any real preparation already admitted.
            // 仅取消此等待者；实例注册表持有已接纳的真实准备。
            let runtime = execution
                .wait(self.ensure_formal_instance(
                    &plugin.id,
                    plugin.generation,
                    VulcanPluginPersistentRuntimeKind::UiAction,
                    execution.deadline.into_std(),
                ))
                .await??;
            let result = self
                .invoke_formal_ui_action(
                    &runtime,
                    &action.function,
                    json!({ "arguments": request.arguments }),
                    &mut execution,
                )
                .await?;
            if let Some(schema_file) = action.output_schema_file.as_deref() {
                // Every call still checks the actual package path and reads the current regular JSON file.
                // 每次调用仍检查实际包路径，并读取当前普通 JSON 文件。
                let schema_path =
                    canonical_plugin_file(&execution_root, schema_file, "action output")
                        .map_err(VulcanPluginRuntimeError::new)?;
                // Parsed contents are the authority; generation identity never substitutes for current file evidence.
                // 解析内容为权威；代次身份绝不替代当前文件证据。
                let schema = read_plugin_json_state(&schema_path, "action output")?;
                // This instance's sole slot saves compilation only and preserves the exact declaration for this result.
                // 此实例的唯一槽仅节省编译，并为此结果保留精确声明。
                let validator = runtime.ui_output_schema_validator(schema_path, schema)?;
                validator.validate(&result).map_err(|error| {
                    VulcanPluginRuntimeError::new(format!("Invalid action output: {error}"))
                })?;
            }
            return Ok(result);
        }
        // The formal call owns its private initialization, sole business operation and physical retirement.
        // 正式调用持有其私有初始化、唯一业务操作及物理退役。
        let result = self
            .invoke_formal_action(
                plugin.clone(),
                workspace_root,
                action.function.clone(),
                json!({ "arguments": request.arguments }),
                execution,
            )
            .await?;
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
