//! Explicit reusable-tool initialization under existing controller and module ownership.
//! 既有控制器及模块归属下的明确可复用工具初始化。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Initialize one additional VM for plugin_id and its published tool_id at exactly plugin_generation.
    /// 为 plugin_id 及其已发布 tool_id 在精确 plugin_generation 下初始化一个额外 VM。
    /// The trusted caller supplies owner_session_id and workspace_root, authorizing normal module initialization capabilities.
    /// 可信调用方提供 owner_session_id 及 workspace_root，并授权普通模块初始化能力。
    /// Return the core instance_id object after confirmed initialization; admitted failures retain queryable formal call evidence.
    /// 确认初始化后返回核心 instance_id 对象；已入场失败保留可查询正式调用证据。
    /// This explicit operation may produce initialization effects, never invokes business, and never replenishes a target automatically.
    /// 此明确操作可能产生初始化副作用，绝不调用业务，也不自动补齐目标数量。
    pub async fn prewarm_reusable_tool_instance(
        &self,
        plugin_id: &str,
        tool_id: &str,
        plugin_generation: u64,
        owner_session_id: String,
        workspace_root: PathBuf,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        if owner_session_id.trim().is_empty() {
            return Err(VulcanPluginRuntimeError::new(
                "Explicit prewarming requires a controller owner.",
            ));
        }
        // One published snapshot owns both the selected tool declaration and its exact package generation.
        // 单个已发布快照同时拥有所选工具声明及其精确包代次。
        let snapshot = self.snapshot();
        // Tool IDs resolve only through the enabled catalog; callers cannot supply replacement declarations.
        // 工具标识仅通过已启用目录解析；调用方不能提供替代声明。
        let tool = snapshot.enabled_tools.get(tool_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Reusable prewarm tool is not enabled.")
        })?;
        // Generation counters are per plugin, so equal numbers cannot authorize a tool transferred to another plugin.
        // 代次计数按插件独立维护，因此相同数值不能授权已转移至另一插件的工具。
        if tool.plugin_id != plugin_id || tool.plugin_generation != plugin_generation {
            return Err(VulcanPluginRuntimeError::new(
                "Reusable prewarm plugin identity or generation changed before admission.",
            ));
        }
        if tool.manifest.execution.reusable.is_none() {
            return Err(VulcanPluginRuntimeError::new(
                "Explicit prewarming requires a declared reusable tool.",
            ));
        }
        // The shared resolver supplies the declared default and grace; prewarming has no business timeout argument.
        // 共享解析器提供声明默认值及宽限；预热没有业务超时参数。
        let timeout_ms = crate::runtime::sessions::resolve_host_timeout(tool, &Map::new())?;
        // One absolute deadline covers host preparation, core queueing and actual initialization.
        // 单个绝对截止时间覆盖宿主准备、核心排队及真实初始化。
        let deadline = super::super::pinned::deadline(timeout_ms)?;
        // Retain the exact attached parent, never create a separate scheduler for prewarming.
        // 保留精确附加父级，绝不为预热创建独立调度器。
        let owner = self.required_formal_owner()?;
        // Resolve only the package named by the same trusted tool snapshot.
        // 仅解析同一可信工具快照指定的包。
        let plugin = snapshot
            .plugins
            .get(&tool.plugin_id)
            .cloned()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Reusable prewarm plugin is missing."))?;
        // The original manifest owns the independent finalizer even when business never executes.
        // 即使业务从未执行，原清单仍拥有独立关闭器。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Reusable prewarm manifest is missing.")
        })?;
        // Reuse the authoritative Lua declaration without probing exports or substituting a function.
        // 复用权威 Lua 声明，不探测导出或替换函数。
        let runtime = crate::runtime::lease::required_lua_runtime(manifest)?;
        // Native prewarming has no business envelope; the existing retained input budget still counts all routing and cleanup data.
        // 原生预热没有业务信封；既有保留输入预算仍计入全部路由及清理数据。
        let input = Preparation {
            workspace: CallWorkspace::Provided(workspace_root),
            arguments: Value::Null,
            finalizer: Some(ModuleFinalizer {
                export: runtime.shutdown_function.clone(),
                arguments: self.formal_envelope(
                    plugin_payload("shutdown", Map::new())?,
                    owner.settings.shutdown_timeout_ms,
                )?,
                timeout_ms: owner.settings.shutdown_timeout_ms,
            }),
            admission: CallAdmission::Prewarm {
                tool_id: tool.id.clone(),
                owner_session_id: owner_session_id.clone(),
            },
            scope: VulcanPluginBridgeLeaseScope::Enabled,
            security_partition: format!("tool:{owner_session_id}"),
            deadline,
        };
        self.invoke_owned_formal_call(owner, plugin, input).await
    }
}
