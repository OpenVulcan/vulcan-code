//! Manifest-bound restricted Lua modules for the formal pool owner.
//! 面向正式池所有者、绑定清单的受限 Lua 模块。

use super::*;
mod hygiene;
use crate::runtime::lease::required_lua_runtime;
use crate::runtime::local_files::canonical_plugin_file;
use hygiene::bundle_source_with_hygiene;
use luaskills::runtime::embedded::{ModuleExport, ModuleFinalizer};
use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;

/// Host-approved execution domain for one exact manifest bundle, without caller-provided Lua source.
/// 单个精确清单 bundle 的宿主批准执行域，不接收调用方提供的 Lua 源码。
pub struct VulcanPluginBundlePoolRequest {
    /// Exact installed plugin ID selected by the host catalog.
    /// 由宿主目录选择的精确已安装插件 ID。
    pub plugin_id: String,
    /// Catalog generation observed before requesting this execution domain.
    /// 请求此执行域前观测到的目录代次。
    pub plugin_generation: u64,
    /// Explicit authorized workspace; plugin arguments cannot replace it.
    /// 显式已授权工作区；插件参数不能替换它。
    pub workspace_root: PathBuf,
    /// Host-owned tenant/workspace/session partition for instance isolation.
    /// 用于实例隔离的宿主持有租户、工作区或会话分区。
    pub security_partition: String,
    /// Exact lifecycle state required for native service entry.
    /// 原生服务入场所要求的精确生命周期状态。
    pub scope: VulcanPluginBridgeLeaseScope,
    /// Explicit host-resolved policy; callers must preserve manifest reuse and ordering requirements.
    /// 显式宿主解析策略；调用方必须保留清单复用及顺序要求。
    pub policy: PluginPoolConfig,
    /// Optional host-selected manifest shutdown entry, with frozen arguments and an independent finite budget.
    /// 可选宿主选择的清单关闭入口，携带冻结参数及独立有限预算。
    /// Tool and Hook routes must supply their own version-one envelopes; ordinary exports cannot be selected.
    /// 工具及 Hook 路由必须提供各自的版本一信封；不能选择普通业务导出。
    pub finalizer: Option<ModuleFinalizer>,
}

impl VulcanPluginEmbeddedHost {
    /// Register request's manifest bundle with independent capacity and return its native pool identity.
    /// 以独立容量注册 request 的清单 bundle，并返回原生池身份。
    /// Exact catalog and capability validation remains shared with capacity-bound registration.
    /// 精确目录及能力校验继续与绑定容量的注册共用。
    pub fn register_bundle_pool(
        &self,
        request: VulcanPluginBundlePoolRequest,
    ) -> EmbeddedResult<String> {
        self.register_bundle_pool_with_capacity(None, request, None, None)
    }

    /// Register request's real bundle in exact capacity_id and return its native member pool identity.
    /// 在精确 capacity_id 中注册 request 的真实 bundle，并返回原生成员池身份。
    /// This shares capacity only; package, Lua state and native authority remain bound to the original member.
    /// 此方法仅共享容量；包、Lua 状态及原生权威保持绑定原成员。
    pub fn register_bundle_pool_in_capacity(
        &self,
        capacity_id: &str,
        request: VulcanPluginBundlePoolRequest,
    ) -> EmbeddedResult<String> {
        self.register_bundle_pool_with_capacity(Some(capacity_id), request, None, None)
    }

    /// Register the exact reusable tool in capacity_id; request must match its declared policy and shutdown.
    /// 在 capacity_id 中注册精确可复用工具；request 必须匹配其声明策略及关闭器。
    /// Returns the core pool identity after capturing private hygiene callbacks on every original VM.
    /// 在每个原始 VM 中捕获私有清理回调后返回核心池身份。
    pub fn register_reusable_bundle_pool_in_capacity(
        &self,
        capacity_id: &str,
        request: VulcanPluginBundlePoolRequest,
        tool_id: &str,
    ) -> EmbeddedResult<String> {
        self.register_bundle_pool_with_capacity(Some(capacity_id), request, Some(tool_id), None)
    }

    /// Register tool_id in capacity_id with the exact snapshot already used to choose its reusable domain.
    /// 使用已用于选择可复用域的精确 snapshot，在 capacity_id 中注册 tool_id。
    /// request supplies unchanged package and policy authority; returns the pool without replacing the snapshot.
    /// request 提供不变的插件包及策略权威；返回池且不替换快照。
    pub(in crate::runtime) fn register_reusable_bundle_pool_with_snapshot_in_capacity(
        &self,
        capacity_id: &str,
        request: VulcanPluginBundlePoolRequest,
        tool_id: &str,
        snapshot: CapabilitySnapshot,
    ) -> EmbeddedResult<String> {
        self.register_bundle_pool_with_capacity(
            Some(capacity_id),
            request,
            Some(tool_id),
            Some(snapshot),
        )
    }

    /// Load the manifest-selected bundle and fixed restricted loader for `request`.
    /// 为 `request` 加载清单选择的 bundle 及固定受限加载器。
    /// Returns a registered core pool; module initialization and declared exports run only on admission.
    /// 返回已注册核心池；模块初始化及声明导出仅在入场后运行。
    /// The plugin aggregate must already exist; explicit package dependencies are required.
    /// 插件聚合必须已存在；要求显式包依赖声明。
    /// capacity_id selects exact membership when present and independent placement when absent.
    /// capacity_id 存在时选择精确成员关系，缺省时选择独立归属。
    fn register_bundle_pool_with_capacity(
        &self,
        capacity_id: Option<&str>,
        request: VulcanPluginBundlePoolRequest,
        reusable_tool: Option<&str>,
        capability_snapshot: Option<CapabilitySnapshot>,
    ) -> EmbeddedResult<String> {
        // Keep the owning manager alive during exact descriptor and file resolution.
        // 在精确描述符及文件解析期间保持所属管理器存活。
        let state = self
            .authority
            .manager
            .upgrade()
            .ok_or_else(|| error(EmbeddedErrorCode::Closed, "Plugin manager is closed."))?;
        // The cloned descriptor retains the actual immutable package while its source is read.
        // 克隆描述符在读取源码期间保留真实不可变包。
        let plugin = state
            .snapshot
            .read()
            .map_err(|_| internal("Plugin catalog lock is poisoned."))?
            .plugins
            .get(&request.plugin_id)
            .cloned()
            .ok_or_else(|| {
                error(
                    EmbeddedErrorCode::StaleGeneration,
                    "Plugin is no longer installed.",
                )
            })?;
        if plugin.generation != request.plugin_generation {
            return Err(error(
                EmbeddedErrorCode::StaleGeneration,
                "Plugin changed before bundle preparation.",
            ));
        }
        if plugin.runtime_state != request.scope.required_runtime_state() {
            return Err(error(
                EmbeddedErrorCode::PermissionDenied,
                "Plugin lifecycle does not authorize bundle preparation.",
            ));
        }
        // Content-only plugins cannot acquire execution by calling this entry point.
        // 纯内容插件不能通过调用此入口获得执行能力。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin has no valid manifest.",
            )
        })?;
        // This is the same validated Lua contract used by existing tool and hook dispatch.
        // 这是现有工具及 Hook 分发使用的同一已校验 Lua 契约。
        let runtime = required_lua_runtime(manifest)
            .map_err(|failure| error(EmbeddedErrorCode::InvalidArgument, failure.message))?;
        // Resolve hygiene exclusively from the validated manifest, never from request-supplied Lua.
        // 仅从已校验清单解析清理契约，绝不接受请求提供的 Lua。
        let reusable = reusable_tool
            .map(|tool_id| {
                let tool = runtime
                    .tools
                    .iter()
                    .find(|tool| tool.id == tool_id)
                    .ok_or_else(|| {
                        error(
                            EmbeddedErrorCode::InvalidArgument,
                            "Reusable tool is not declared.",
                        )
                    })?;
                let reuse = tool.execution.reusable.as_ref().ok_or_else(|| {
                    error(
                        EmbeddedErrorCode::InvalidArgument,
                        "Tool does not declare reuse hygiene.",
                    )
                })?;
                if request.scope != VulcanPluginBridgeLeaseScope::Enabled
                    || request.policy.reuse != luaskills::runtime::embedded::InstanceReuse::Reusable
                    || request.policy.max_resident_vms != reuse.max_resident_vms
                    || request.policy.max_running_calls != reuse.max_running_calls
                    || request.policy.min_resident_vms != 0
                    || request.policy.serial
                        != (reuse.ordering == crate::VulcanPluginToolOrdering::KeySerial)
                    || request.policy.idle_ttl_ms != Some(reuse.idle_ttl_ms)
                    || request.policy.max_uses != Some(reuse.max_uses)
                    || request
                        .finalizer
                        .as_ref()
                        .map(|finalizer| finalizer.export.as_str())
                        != Some(runtime.shutdown_function.as_str())
                {
                    return Err(error(
                        EmbeddedErrorCode::InvalidArgument,
                        "Reusable tool policy differs from its exact manifest contract.",
                    ));
                }
                Ok(reuse)
            })
            .transpose()?;
        // Shutdown selection is explicit; sharing a bundle does not imply sharing lifecycle semantics.
        // 关闭选择为显式声明；共享 bundle 不表示共享生命周期语义。
        if let Some(finalizer) = &request.finalizer
            && finalizer.export != runtime.shutdown_function
            && Some(finalizer.export.as_str()) != runtime.hook_shutdown_function.as_deref()
        {
            return Err(error(
                EmbeddedErrorCode::InvalidArgument,
                "Plugin finalizer must select a declared tool or Hook shutdown function.",
            ));
        }
        // Missing dependencies are explicit, never synthesized into an authorized package.
        // 缺失依赖为显式错误，绝不在已授权包内合成依赖。
        let dependencies_file = manifest
            .execution
            .dependencies
            .manifest
            .clone()
            .ok_or_else(|| {
                error(
                    EmbeddedErrorCode::InvalidArgument,
                    "Formal plugin bundles require an explicit dependency manifest.",
                )
            })?;
        // Resolve exactly the package generation already used by process and configuration services.
        // 解析进程及配置服务已使用的精确包代次。
        let package =
            super::super::super::environment::plugin_execution_package_root(&state, &plugin)
                .map_err(|message| error(EmbeddedErrorCode::InvalidArgument, message))?;
        // Bundle containment is checked before reading any executable source bytes.
        // 在读取任何可执行源码字节前检查 bundle 包含关系。
        let bundle_file = canonical_plugin_file(&package, &runtime.bundle, "plugin Lua bundle")
            .map_err(|message| error(EmbeddedErrorCode::InvalidArgument, message))?;
        // Freeze real package bytes before publishing the core execution domain.
        // 在发布核心执行域前冻结真实包字节。
        let bundle_source = fs::read_to_string(bundle_file).map_err(|failure| {
            error(
                EmbeddedErrorCode::InvalidArgument,
                format!("Failed to read plugin bundle: {failure}"),
            )
        })?;
        // The fixed trusted loader is shared with the compatibility bridge's restricted environment.
        // 固定可信加载器与兼容桥共享受限环境。
        let bridge_file = canonical_plugin_file(
            &state.bridge_package_root,
            "runtime/bridge.lua",
            "plugin module loader",
        )
        .map_err(|message| error(EmbeddedErrorCode::InvalidArgument, message))?;
        // Loader source comes only from the host package, never the plugin manifest.
        // 加载器源码仅来自宿主包，绝不来自插件清单。
        let bridge_source = fs::read_to_string(bridge_file).map_err(|failure| {
            error(
                EmbeddedErrorCode::InvalidArgument,
                format!("Failed to read plugin module loader: {failure}"),
            )
        })?;
        // All public functions come from validated manifest declarations, with aliases deduplicated.
        // 所有公开函数来自已校验清单声明，并对别名去重。
        let functions: BTreeSet<String> = [
            runtime.invoke_function.clone(),
            runtime.shutdown_function.clone(),
        ]
        .into_iter()
        .chain(runtime.hook_shutdown_function.clone())
        .chain(runtime.hooks.iter().map(|hook| hook.function.clone()))
        .chain(runtime.actions.iter().map(|action| action.function.clone()))
        .chain(
            [
                runtime.lifecycle.install.clone(),
                runtime.lifecycle.enable.clone(),
                runtime.lifecycle.disable.clone(),
                runtime.lifecycle.uninstall.clone(),
            ]
            .into_iter()
            .flatten(),
        )
        .collect();
        // Source and manifest values cross into the loader as a single encoded data value.
        // 源码及清单值作为一个已编码数据值进入加载器。
        let mut capture_names = functions.clone();
        for reuse in runtime
            .tools
            .iter()
            .filter_map(|tool| tool.execution.reusable.as_ref())
        {
            capture_names.extend([
                reuse.before_call.clone(),
                reuse.after_call.clone(),
                reuse.reuse_check.clone(),
            ]);
        }
        // Capture hygiene privately; the core exports remain restricted to manifest business declarations.
        // 私有捕获清理函数；核心导出仍限制为清单业务声明。
        let load_arguments = json!({"plugin_id":plugin.id,"bundle_source":bundle_source,
            "lua_modules":runtime.lua_modules,"function_names":capture_names,
            "finalizer_name":request.finalizer.as_ref().map(|finalizer| &finalizer.export)})
        .to_string();
        // Literal quoting prevents bundle contents, newlines and delimiters from becoming wrapper code.
        // 字面量引用防止 bundle 内容、换行及分隔符变成包装代码。
        let source = bundle_source_with_hygiene(
            &bridge_source,
            &load_arguments,
            &runtime.invoke_function,
            reusable,
        );
        // Normalize the trusted workspace once before its spelling enters the core invocation identity.
        // 在可信工作区拼写进入核心调用身份前进行一次规范化。
        let workspace = request.workspace_root.canonicalize().map_err(|failure| {
            error(
                EmbeddedErrorCode::InvalidArgument,
                format!("Failed to resolve plugin workspace: {failure}"),
            )
        })?;
        // Existing version-one functions consume object arguments and return their JSON frame string.
        // 现有版本一函数消费对象参数，并返回其 JSON 帧字符串。
        let exports = functions
            .into_iter()
            .map(|name| ModuleExport {
                name,
                input_schema: json!({"type":"object"}),
                output_schema: json!({"type":"string"}),
            })
            .collect();
        self.register_pool_with_capacity(
            capacity_id,
            ModuleDefinition {
                finalizer: request.finalizer,
                plugin_id: plugin.id.clone(),
                generation: plugin.generation.to_string(),
                package_root: package.to_string_lossy().into_owned(),
                dependencies_file,
                workspace_root: Some(workspace.to_string_lossy().into_owned()),
                cwd: None,
                mounts: json!({}),
                security_partition: request.security_partition,
                source,
                exports,
            },
            request.policy,
            request.scope,
            capability_snapshot,
            reusable.map(|reuse| reuse.initialization_capabilities.iter().cloned().collect()),
        )
    }
}

/// Encode UTF-8 `value` as a Lua quoted string; return only a literal, never executable concatenation.
/// 将 UTF-8 `value` 编码为 Lua 引号字符串；仅返回字面量，绝不返回可执行拼接。
/// Three-digit decimal escapes preserve control bytes without consuming following source digits.
/// 三位十进制转义保留控制字节，且不吞入后续源码数字。
fn lua_string_literal(value: &str) -> String {
    // UTF-8 characters remain unchanged unless Lua string syntax requires an escape.
    // 除非 Lua 字符串语法要求转义，否则 UTF-8 字符保持不变。
    let mut literal = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            character if character.is_ascii_control() => {
                write!(literal, "\\{:03}", character as u32)
                    .expect("writing into String cannot fail");
            }
            character => literal.push(character),
        }
    }
    literal.push('"');
    literal
}
