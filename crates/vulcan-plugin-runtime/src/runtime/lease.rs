//! LuaSkills lease creation, evaluation, closing, and exact response decoding.
//! LuaSkills租约创建、执行、关闭与精确响应解码。

use super::bridge::VulcanPluginBridgeLeaseScope;
use super::{
    ManagedVulcanPluginSession, VULCAN_PLUGIN_PROBE_TIMEOUT_MS, VulcanPluginRuntimeError,
    VulcanPluginRuntimeManager,
};
use crate::{VulcanPluginLuaRuntimeManifest, VulcanPluginManifest};
use luaskills::LuaEngine;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};

impl VulcanPluginRuntimeManager {
    /// Creates one LuaSkills managed System lease for an exact validated package.
    /// 为一个精确且通过校验的插件包创建 LuaSkills 受管系统租约。
    /// Uses the supplied session/package/workspace identity, validated manifest, and scope; returns the lease id, sid, and generation or a load/cleanup error.
    /// 使用传入的会话/包/工作区身份、已校验清单与作用域；返回租约标识、sid及代次，或加载/清理错误。
    pub(super) async fn create_managed_lease(
        &self,
        session_id: &str,
        package_root: &Path,
        workspace_root: &Path,
        manifest: &VulcanPluginManifest,
        lease_scope: VulcanPluginBridgeLeaseScope,
    ) -> Result<(String, String, u64), VulcanPluginRuntimeError> {
        let lua_runtime = required_lua_runtime(manifest)?;
        // canonical_package_root pins the real plugin package identity before its bundle is read.
        // canonical_package_root 在读取 bundle 前固定真实插件包身份。
        let canonical_package_root = package_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve Vulcan plugin package root: {error}"
            ))
        })?;
        // canonical_workspace_root binds process cwd authorization to the lease, not plugin input.
        // canonical_workspace_root 把进程 cwd 授权绑定到租约，而不是插件输入。
        let canonical_workspace_root = workspace_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve Vulcan plugin workspace root: {error}"
            ))
        })?;
        if !canonical_workspace_root.is_dir() {
            return Err(VulcanPluginRuntimeError::new(
                "Vulcan plugin workspace root is not a directory.",
            ));
        }
        // bundle_path is resolved from the validated manifest and may not escape the real package.
        // bundle_path 从已校验清单解析，且不得逃逸真实插件包。
        let bundle_path = canonical_package_root
            .join(&lua_runtime.bundle)
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve Vulcan plugin Lua bundle: {error}"
                ))
            })?;
        if !bundle_path.starts_with(&canonical_package_root) || !bundle_path.is_file() {
            return Err(VulcanPluginRuntimeError::new(
                "Vulcan plugin Lua bundle escapes its package root or is not a regular file.",
            ));
        }
        // bundle_source is immutable input loaded into the fixed bridge lease.
        // bundle_source 是加载到固定桥接租约中的不可变输入。
        let bundle_source = fs::read_to_string(&bundle_path).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to read Vulcan plugin Lua bundle: {error}"
            ))
        })?;
        // bridge_package_root is the only host-owned LuaSkills bridge package used by plugins.
        // bridge_package_root 是插件使用的唯一宿主自有 LuaSkills 桥接包。
        let bridge_package_root =
            self.state
                .bridge_package_root
                .canonicalize()
                .map_err(|error| {
                    VulcanPluginRuntimeError::new(format!(
                        "Failed to resolve Vulcan plugin bridge package root: {error}"
                    ))
                })?;
        let bridge_dependencies_file = bridge_package_root.join("dependencies.yaml");
        if !bridge_dependencies_file.is_file() {
            return Err(VulcanPluginRuntimeError::new(
                "Vulcan plugin bridge dependencies.yaml is unavailable.",
            ));
        }
        // bridgeModulePath must remain inside the canonical fixed bridge package root.
        // bridgeModulePath 必须保持在规范化的固定桥接包根目录内。
        let bridge_module_path = bridge_package_root
            .join("runtime")
            .join("bridge.lua")
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve Vulcan plugin bridge module: {error}"
                ))
            })?;
        if !bridge_module_path.starts_with(&bridge_package_root) || !bridge_module_path.is_file() {
            return Err(VulcanPluginRuntimeError::new(
                "Vulcan plugin bridge module escapes its package root or is not a regular file.",
            ));
        }
        // bridgeSource bypasses Lua package.searchpath while preserving the exact trusted package file.
        // bridgeSource 绕过 Lua package.searchpath，同时保留完全相同的可信包文件。
        let bridge_source = fs::read_to_string(&bridge_module_path).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to read Vulcan plugin bridge module: {error}"
            ))
        })?;
        // lua_bridge_package_root removes Windows verbatim prefixes before the bridge package root enters LuaSkills.
        // lua_bridge_package_root 在桥接包根进入 LuaSkills 前移除 Windows verbatim 前缀。
        let lua_bridge_package_root = vulcan_runtime_assets::native_host_path(&bridge_package_root);
        // lua_workspace_root removes Windows verbatim prefixes before the authorized workspace enters LuaSkills.
        // lua_workspace_root 在授权工作区进入 LuaSkills 前移除 Windows verbatim 前缀。
        let lua_workspace_root = vulcan_runtime_assets::native_host_path(&canonical_workspace_root);
        let requested_sid = format!("vulcan-plugin/{}/{session_id}", manifest.id);
        let create_request = json!({
            "sid": requested_sid.clone(),
            "ttl_sec": 0,
            "replace": false,
            "cwd": lua_bridge_package_root,
            "workspace_root": lua_workspace_root,
            "system_package": {
                "id": "vulcan-plugin-bridge",
                "root": lua_bridge_package_root,
                "dependencies_file": "dependencies.yaml",
            },
        });
        let engine = self.runtime_engine()?;
        let response = call_engine_blocking(Arc::clone(&engine), move |engine| {
            let raw = engine
                .create_system_runtime_lease_json(&create_request.to_string())
                .map_err(VulcanPluginRuntimeError::new)?;
            parse_engine_response(&raw, "create Vulcan plugin bridge lease")
        })
        .await?;
        let lease_id = required_response_string(&response, "lease_id")?;
        let sid = required_response_string(&response, "sid")?;
        if sid != requested_sid {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin bridge lease returned unexpected sid `{sid}` instead of `{requested_sid}`."
            )));
        }
        let generation = required_response_u64(&response, "generation")?;
        // load_request evaluates only the fixed bridge module; the real plugin is supplied as data.
        // load_request 只执行固定桥接模块；真实插件以数据形式提供。
        let mut load_request = json!({
            "lease_id": lease_id.clone(),
            "sid": sid.clone(),
            "generation": generation,
            "code": "local loader, compile_error = load(args.bridge_source, '@vulcan-plugin-bridge/runtime/bridge.lua', 't')\nif loader == nil then error('Failed to compile Vulcan plugin bridge: ' .. tostring(compile_error)) end\nlocal bridge = loader()\nif type(bridge) ~= 'table' then error('Vulcan plugin bridge must return a table') end\npackage.loaded['runtime.bridge'] = bridge\nreturn bridge.load(args.plugin)",
            "args": {
                "bridge_source": bridge_source,
                "plugin": {
                    "plugin_id": manifest.id,
                    "runtime_id": self.state.manager_id.clone(),
                    "lease_scope": lease_scope.as_str(),
                    "workspace_root": lua_workspace_root,
                    "bundle_source": bundle_source,
                    "lua_modules": lua_runtime.lua_modules,
                    "invoke_function": lua_runtime.invoke_function,
                    "shutdown_function": lua_runtime.shutdown_function,
                    "hook_shutdown_function": lua_runtime.hook_shutdown_function,
                    "hook_functions": lua_runtime
                        .hooks
                        .iter()
                        .map(|hook| hook.function.clone())
                        .collect::<Vec<_>>(),
                    "additional_functions": lua_runtime
                        .actions
                        .iter()
                        .map(|action| action.function.clone())
                        .chain([
                            lua_runtime.lifecycle.install.clone(),
                            lua_runtime.lifecycle.enable.clone(),
                            lua_runtime.lifecycle.disable.clone(),
                            lua_runtime.lifecycle.uninstall.clone(),
                        ].into_iter().flatten())
                        .collect::<BTreeSet<_>>(),
                },
            },
            "timeout_ms": VULCAN_PLUGIN_PROBE_TIMEOUT_MS,
        });
        let load_result = call_engine_blocking(engine, move |engine| {
            // Initialization consumes the original Action budget after blocking-queue admission.
            // 初始化在进入阻塞执行后消耗原始 Action 预算。
            if let Some(deadline) = super::action_status::callback_deadline() {
                let remaining = deadline
                    .saturating_duration_since(std::time::Instant::now())
                    .as_millis() as u64;
                if remaining == 0 {
                    return Err(VulcanPluginRuntimeError::new(
                        "Plugin initialization deadline exceeded.",
                    ));
                }
                load_request["timeout_ms"] = json!(remaining.min(VULCAN_PLUGIN_PROBE_TIMEOUT_MS));
            }
            let raw = engine
                .eval_system_runtime_lease_json(&load_request.to_string())
                .map_err(VulcanPluginRuntimeError::new)?;
            let response = parse_engine_response(&raw, "load Vulcan plugin bundle through bridge")?;
            if response.get("result").and_then(Value::as_bool) != Some(true) {
                return Err(VulcanPluginRuntimeError::new(
                    "Vulcan plugin bridge did not confirm bundle loading.",
                ));
            }
            Ok(())
        })
        .await;
        if let Err(error) = load_result {
            let cleanup_result = self.close_raw_lease(&lease_id, &sid, generation).await;
            return match cleanup_result {
                Ok(()) => Err(error),
                Err(cleanup_error) => {
                    let diagnostic =
                        format!("{error}; failed to close rejected bridge lease: {cleanup_error}");
                    self.retain_failed_lease(
                        super::RetainedVulcanPluginLease {
                            plugin_id: manifest.id.clone(),
                            owner_session_id: None,
                            lease_id,
                            sid,
                            generation,
                            reserved_session_slot: false,
                        },
                        diagnostic.clone(),
                    )?;
                    Err(VulcanPluginRuntimeError::new(diagnostic))
                }
            };
        }
        Ok((lease_id, sid, generation))
    }

    /// Evaluates one exact manifest-validated plugin function through the fixed bridge ABI.
    /// 通过固定桥接 ABI 执行一个经清单校验的精确插件函数。
    ///
    /// # Parameters
    /// 参数
    /// Lease identity, function name, arguments, and timeout are all supplied by the host.
    /// 租约身份、函数名、参数与超时均由宿主提供。
    ///
    /// # Returns
    /// 返回值
    /// Returns the decoded plugin result envelope.
    /// 返回解码后的插件结果信封。
    pub(super) async fn evaluate_bridge_function(
        &self,
        lease_id: &str,
        sid: &str,
        generation: u64,
        function_name: &str,
        plugin_args: Value,
        timeout_ms: u64,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let lease_id = lease_id.to_owned();
        let sid = sid.to_owned();
        let function_name = function_name.to_owned();
        // Include blocking-pool and engine-lock waits in the supplied remaining budget.
        // 阻塞池及引擎锁等待包含在传入的剩余预算内。
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(timeout_ms))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin execution deadline is out of range.")
            })?;
        let engine = self.runtime_engine()?;
        call_engine_blocking(engine, move |engine| {
            let timeout_ms = deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_millis() as u64;
            if timeout_ms == 0 {
                return Err(VulcanPluginRuntimeError::new(
                    "Plugin execution deadline expired while waiting for the engine.",
                ));
            }
            let eval_request = json!({
                "lease_id": lease_id,
                "sid": sid,
                "generation": generation,
                "code": "local bridge = require('runtime.bridge')\nreturn bridge.invoke(args)",
                "args": {
                    "function_name": function_name,
                    "plugin_args": plugin_args,
                },
                "timeout_ms": timeout_ms,
            });
            let raw = engine
                .eval_system_runtime_lease_json(&eval_request.to_string())
                .map_err(VulcanPluginRuntimeError::new)?;
            let response = parse_engine_response(&raw, "evaluate Vulcan plugin bridge function")?;
            parse_plugin_result(&response)
        })
        .await
    }

    /// Closes one raw LuaSkills System lease by exact id and generation.
    /// 按精确标识与代次关闭一个原始 LuaSkills 系统租约。
    /// `lease_id`, `sid`, and `generation` select one exact lease; returns unit or an explicit engine failure.
    /// `lease_id`、`sid`及`generation`选择精确租约；返回空值或明确的引擎失败。
    pub(super) async fn close_raw_lease(
        &self,
        lease_id: &str,
        sid: &str,
        generation: u64,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let lease_id = lease_id.to_owned();
        let sid = sid.to_owned();
        let engine = self.runtime_engine()?;
        call_engine_blocking(engine, move |engine| {
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
            parse_engine_response(&raw, "close Vulcan Plugin hook lease").map(|_| ())
        })
        .await
    }
}

/// Resolves the Lua protocol required by an execution path that cannot operate without it.
/// 解析无法脱离 Lua 协议运行的执行路径所要求的 Lua 协议。
///
/// # Parameters
/// 参数
/// `manifest` is the already validated plugin manifest captured by the current generation.
/// `manifest` 是当前代次捕获且已经校验的插件清单。
///
/// # Returns
/// 返回值
/// Returns the exact Lua declaration or an explicit invariant error for a non-Lua plugin.
/// 返回精确 Lua 声明；若插件不含 Lua 协议则返回显式不变量错误。
pub(super) fn required_lua_runtime(
    manifest: &VulcanPluginManifest,
) -> Result<&VulcanPluginLuaRuntimeManifest, VulcanPluginRuntimeError> {
    manifest.lua_runtime.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new(format!(
            "Vulcan plugin `{}` does not declare a Lua runtime.",
            manifest.id
        ))
    })
}

/// Runs one synchronous LuaSkills operation outside the async executor thread.
/// 在异步执行器线程之外运行一个同步 LuaSkills 操作。
/// Locks the supplied shared `engine` on a blocking worker and returns `operation`'s typed result or a task/engine error.
/// 在阻塞工作线程锁定传入的共享`engine`，返回`operation`的强类型结果或任务/引擎错误。
///
/// The read guard is sufficient because `operation` only receives `&LuaEngine`: every plugin lease
/// create, evaluate, status and close entry is an upstream `&self` method. Distinct plugin leases
/// therefore overlap, while a skill-management write still drains them before it mutates the engine.
/// 读守卫已经足够，因为 `operation` 只接收 `&LuaEngine`：插件租约的创建、执行、状态与关闭入口
/// 在上游都是 `&self` 方法。因此不同插件租约可以重叠，而技能管理写操作仍会在变更引擎前排空它们。
pub(super) async fn call_engine_blocking<T>(
    engine: Arc<RwLock<LuaEngine>>,
    operation: impl FnOnce(&LuaEngine) -> Result<T, VulcanPluginRuntimeError> + Send + 'static,
) -> Result<T, VulcanPluginRuntimeError>
where
    T: Send + 'static,
{
    let record = super::action_status::ACTION_RECORD
        .try_with(Arc::clone)
        .ok();
    let queued = std::time::Instant::now();
    tokio::task::spawn_blocking(move || {
        let _scope = super::action_status::BlockingActionScope::enter(record.clone());
        if let Some(record) = &record {
            record.observe("blocking_queue", queued.elapsed());
        }
        let lock_started = std::time::Instant::now();
        let engine = engine.read().map_err(|_| {
            VulcanPluginRuntimeError::new("LuaSkills engine read lock is poisoned.")
        })?;
        if let Some(record) = &record {
            record.observe("engine_lock", lock_started.elapsed());
        }
        let started = std::time::Instant::now();
        let result = operation(&engine);
        if let Some(record) = &record {
            record.observe("engine_operation", started.elapsed());
        }
        result
    })
    .await
    .map_err(|error| {
        VulcanPluginRuntimeError::new(format!("Vulcan Plugin runtime task failed: {error}"))
    })?
}

/// Parses one LuaSkills JSON response and requires a successful top-level state.
/// 解析一条 LuaSkills JSON 响应并要求成功的顶层状态。
/// Decodes `raw` under the diagnostic `operation` label; returns the successful envelope or its explicit error.
/// 在诊断标签`operation`下解码`raw`；返回成功信封或其明确错误。
pub(super) fn parse_engine_response(
    raw: &str,
    operation: &str,
) -> Result<Value, VulcanPluginRuntimeError> {
    let response: Value = serde_json::from_str(raw).map_err(|error| {
        VulcanPluginRuntimeError::new(format!("Failed to decode {operation} response: {error}"))
    })?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = response
            .get("error")
            .or_else(|| response.get("message"))
            .map(render_error_value)
            .unwrap_or_else(|| response.to_string());
        return Err(VulcanPluginRuntimeError::new(format!(
            "{operation} failed: {message}"
        )));
    }
    Ok(response)
}

/// Decodes one exact plugin JSON frame nested in a System lease response.
/// 解码嵌套在系统租约响应中的精确插件 JSON 帧。
/// Reads `response` from lease evaluation; returns its decoded plugin result or the protocol/plugin error.
/// 读取租约执行所得的`response`；返回解码后的插件结果或协议/插件错误。
pub(super) fn parse_plugin_result(response: &Value) -> Result<Value, VulcanPluginRuntimeError> {
    let frame = response
        .get("result")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new("Vulcan Plugin runtime eval returned no JSON frame.")
        })?;
    let plugin: Value = serde_json::from_str(frame).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to decode Vulcan Plugin JSON frame: {error}"
        ))
    })?;
    if plugin.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = plugin
            .get("error")
            .map(render_error_value)
            .unwrap_or_else(|| String::from("Unknown Vulcan Plugin runtime error."));
        return Err(VulcanPluginRuntimeError::new(format!(
            "Vulcan Plugin request failed: {message}"
        )));
    }
    Ok(plugin.get("result").cloned().unwrap_or(Value::Null))
}

/// Renders one structured or scalar runtime error without losing upstream detail.
/// 渲染一个结构化或标量运行时错误，同时不丢失上游细节。
/// Converts the supplied `error` JSON value into its original string or serialized JSON text.
/// 将传入的`error`JSON值转换为原字符串或序列化JSON文本。
fn render_error_value(error: &Value) -> String {
    error
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| error.to_string())
}

/// Requires one non-empty string field from a successful engine response.
/// 从成功引擎响应中要求一个非空字符串字段。
/// Reads exact `field` from `response`; returns its nonempty string or a missing-field diagnostic.
/// 从`response`读取精确`field`；返回非空字符串或字段缺失诊断。
fn required_response_string(
    response: &Value,
    field: &str,
) -> Result<String, VulcanPluginRuntimeError> {
    response
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("System lease response omitted {field}."))
        })
}

/// Requires one unsigned integer field from a successful engine response.
/// 从成功引擎响应中要求一个无符号整数字段。
/// Reads exact `field` from `response`; returns its u64 value or a missing-field diagnostic.
/// 从`response`读取精确`field`；返回u64值或字段缺失诊断。
fn required_response_u64(response: &Value, field: &str) -> Result<u64, VulcanPluginRuntimeError> {
    response.get(field).and_then(Value::as_u64).ok_or_else(|| {
        VulcanPluginRuntimeError::new(format!("System lease response omitted {field}."))
    })
}

/// Closes one LuaSkills System lease synchronously during manager drop.
/// 在管理器释放期间同步关闭一个 LuaSkills 系统租约。
/// Uses `engine` and the identity in `session`; returns the close response or a runtime diagnostic.
/// 使用`engine`及`session`中的身份；返回关闭响应或运行时诊断。
pub(super) fn close_system_lease_sync(
    engine: &LuaEngine,
    session: &ManagedVulcanPluginSession,
) -> Result<Value, VulcanPluginRuntimeError> {
    let raw = engine
        .close_system_runtime_lease_json(
            &json!({
                "lease_id": session.lease_id,
                "sid": session.sid,
                "generation": session.generation,
            })
            .to_string(),
        )
        .map_err(VulcanPluginRuntimeError::new)?;
    parse_engine_response(&raw, "close Vulcan Plugin runtime lease")
}

/// Closes one raw LuaSkills System lease synchronously during manager drop.
/// 在管理器释放期间同步关闭一个原始 LuaSkills 系统租约。
/// Uses `engine` with exact lease/sid/generation identity; returns the close response or a runtime diagnostic.
/// 使用`engine`及精确租约/sid/代次身份；返回关闭响应或运行时诊断。
pub(super) fn close_raw_system_lease_sync(
    engine: &LuaEngine,
    lease_id: &str,
    sid: &str,
    generation: u64,
) -> Result<Value, VulcanPluginRuntimeError> {
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
    parse_engine_response(&raw, "close Vulcan Plugin hook lease")
}
