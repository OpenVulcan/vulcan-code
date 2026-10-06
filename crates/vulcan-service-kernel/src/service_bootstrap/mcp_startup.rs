//! Bootstrap consumes explicit legacy definitions or sealed full published sources before actual dispatch.
//! 引导在实际派发之前消费明确旧定义或密封完整已发布来源。

use super::AgentRuntimeBridge;
use std::{collections::BTreeSet, sync::Arc};
use vulcan_agent_runtime::mcp::{
    McpRuntimeAccess, McpServerConfig, McpServerSource, PluginMcpSecretStore,
    PreparedPublishedPluginMcpConfigs, PublishedPluginMcpRuntime,
    bootstrap_mcp_runtime_access_with_published, plugin_mcp_server_configs_for_descriptors,
    published_global_mcp_server_configs,
};
use vulcan_plugin_runtime::{
    VulcanPluginCatalogSnapshot, VulcanPluginRegisteredNativeMcpHostAuthority,
    VulcanPluginRuntimeManager, VulcanPluginRuntimeState,
};

/// Prepared original inputs distinguish full fixed selections from the documented legacy live-file contract.
/// 原已准备输入区分完整固定选择与文档规定的旧实时文件契约。
pub(crate) struct PluginMcpBootstrapPlan {
    /// Only explicitly legacy declarations may enter the source-neutral factory.
    /// 仅明确旧声明可进入中立来源工厂。
    legacy: Vec<McpServerConfig>,
    /// Each sealed bundle owns its original physical source until native dispatch consumes it once.
    /// 每个密封包拥有原物理来源，直至原生派发仅消费一次。
    published: Vec<PreparedPublishedPluginMcpConfigs>,
}

impl PluginMcpBootstrapPlan {
    /// Prepare all enabled MCP owners from catalog using manager's first authority and exact secrets backend.
    /// 使用 manager 首次 authority 及精确 secrets 后端，从 catalog 准备全部已启用 MCP 所有者。
    /// Return explicit producer inputs or an error before any native task starts; preparation errors never change producer.
    /// 在任何原生任务启动之前返回明确生产者输入或错误；准备错误绝不改变生产者。
    pub(crate) fn prepare(
        manager: &VulcanPluginRuntimeManager,
        catalog: &Arc<VulcanPluginCatalogSnapshot>,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        secrets: &dyn PluginMcpSecretStore,
    ) -> Result<Self, String> {
        if !authority.matches_manager(manager) || !Arc::ptr_eq(&manager.snapshot(), catalog) {
            return Err(String::from(
                "Plugin MCP bootstrap requires its original catalog and host.",
            ));
        }
        // Derive owners from the actual enabled declaration map, including conversation-only owners.
        // 从实际已启用声明表派生所有者，包括仅会话所有者。
        let owners = catalog
            .enabled_mcp_servers
            .values()
            .map(|server| server.plugin_id.clone())
            .collect::<BTreeSet<_>>();
        // This set records declared full selections rather than successful attempts or guessed source values.
        // 此集合记录已声明完整选择，而非成功尝试或猜测来源值。
        let mut full_owners = BTreeSet::new();
        // Original physical bundles stay together across all fallible preparation.
        // 原物理包跨全部可失败准备保持共同持有。
        let mut published = Vec::new();
        for plugin_id in owners {
            // Every enabled declaration has one exact catalog owner; missing ownership is an error.
            // 每个已启用声明具有单个精确目录所有者；归属缺失为错误。
            let plugin = catalog.plugins.get(&plugin_id).ok_or_else(|| {
                String::from("Plugin MCP bootstrap declaration has no catalog owner.")
            })?;
            if !plugin.desired_enabled || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            {
                return Err(String::from(
                    "Plugin MCP bootstrap requires enabled ordinary owners.",
                ));
            }
            // A managed selection carries its complete configuration; flat or bundled packages use the live file.
            // 受管选择携带完整配置；平铺或内建插件包使用实时文件。
            if plugin.managed_package().is_some() {
                // The core validates complete pins, original catalog identity and current selection before materialization.
                // 核心在物化前校验完整固定、原目录身份及当前选择。
                let prepared = manager
                    .prepare_published_mcp_resources(catalog, &plugin_id, authority)
                    .map_err(|error| error.to_string())?;
                published.push(published_global_mcp_server_configs(
                    prepared, authority, secrets,
                )?);
                full_owners.insert(plugin_id);
            }
        }
        // Legacy values never become full-source evidence and full selections are never materialized twice.
        // 旧值绝不成为完整来源证据，完整选择绝不被物化两次。
        let legacy_declarations = catalog
            .enabled_mcp_servers
            .values()
            .filter(|server| !full_owners.contains(&server.plugin_id))
            .cloned()
            .collect::<Vec<_>>();
        // This existing producer preserves the explicit bundled, flat and older managed contracts.
        // 此既有生产者保留明确内置、平铺及旧受管契约。
        let legacy =
            plugin_mcp_server_configs_for_descriptors(manager, &legacy_declarations, secrets)?;
        Ok(Self { legacy, published })
    }

    /// Start user and explicit legacy definitions, then register every sealed original before concurrent readiness.
    /// 启动用户及明确旧定义，随后在并发就绪之前登记每个密封原实例。
    /// manager and authority are the original pair; user contains only user declarations.
    /// manager 及 authority 为原配对；user 仅包含用户声明。
    /// Return an armed original owner; cancellation or error requests cleanup of all actual same-lineage work.
    /// 返回已启用原所有者；取消或错误请求清理全部实际同谱系工作。
    pub(crate) async fn start(
        self,
        manager: &VulcanPluginRuntimeManager,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        mut user: Vec<McpServerConfig>,
    ) -> Result<PendingMcpBootstrap, String> {
        if user
            .iter()
            .any(|server| server.source != McpServerSource::UserConfigured)
        {
            return Err(String::from(
                "MCP bootstrap user definitions contain a plugin owner.",
            ));
        }
        user.extend(self.legacy);
        // One factory starts all explicit producers before any readiness wait, preserving concurrent startup deadlines.
        // 单个工厂在任何就绪等待之前启动全部明确生产者，保留并发启动截止时间。
        let (access, originals) =
            bootstrap_mcp_runtime_access_with_published(user, manager, authority, self.published)
                .await?;
        // Arm late-stage cleanup immediately on the factory's return, before any additional await or publication.
        // 在工厂返回后立即启用后期清理，先于任何额外 await 或发布。
        let guard = McpBootstrapFailureGuard {
            access: Some(access),
        };
        Ok(PendingMcpBootstrap { guard, originals })
    }
}

/// Original access cleanup remains armed through full publication and every later fallible startup stage.
/// 原访问清理跨完整发布及随后每个可失败启动阶段保持启用。
struct McpBootstrapFailureGuard {
    /// Presence owns cleanup responsibility; final readiness removes it exactly once.
    /// 存在表示拥有清理责任；最终就绪仅移除一次。
    access: Option<McpRuntimeAccess>,
}

impl McpBootstrapFailureGuard {
    /// Borrow the original armed access; no inputs or replacement source is consulted.
    /// 借用原已启用访问；无输入，不查询替代来源。
    fn access(&self) -> &McpRuntimeAccess {
        self.access
            .as_ref()
            .expect("MCP bootstrap cleanup owner must remain armed")
    }
}

impl Drop for McpBootstrapFailureGuard {
    /// Close all real originals on failed or cancelled startup, including already committed same-lineage partitions.
    /// 在启动失败或取消时关闭全部真实原实例，包括已提交同谱系分区。
    /// No input or result; actual cleanup remains independently observable and no initialization is replayed.
    /// 无输入及结果；实际清理仍可独立观察，不重播初始化。
    fn drop(&mut self) {
        if let Some(access) = &self.access {
            access.shutdown();
        }
    }
}

/// Ready original controls remain closed until the unpublished bridge accepts their actual full successors.
/// 就绪原控制保持关闭，直至未发布桥接纳其实际完整后继。
pub(crate) struct PendingMcpBootstrap {
    /// Original cleanup owner outlives every pending control and actual publication attempt.
    /// 原清理所有者跨每个待处理控制及实际发布尝试存续。
    guard: McpBootstrapFailureGuard,
    /// Original single-shot controls own their preparing leases, never reconstructed native work.
    /// 原单次控制拥有准备租约，绝不拥有重建原生工作。
    originals: Vec<PublishedPluginMcpRuntime>,
}

impl PendingMcpBootstrap {
    /// Clone this original access for the initial unpublished bridge, retaining its exact shared registries.
    /// 为首次未发布桥克隆此原访问，保留其精确共享注册表。
    /// No inputs; return the same lineage without starting another partition.
    /// 无输入；返回同谱系，不启动另一个分区。
    pub(crate) fn access(&self) -> McpRuntimeAccess {
        self.guard.access().clone()
    }

    /// Consume ready originals into bridge's actual full pointer using their original catalog.
    /// 使用原 catalog，将就绪原实例消费至 bridge 实际完整指针。
    /// bridge must still be exclusively owned by bootstrap; return the same armed cleanup owner after all commits.
    /// bridge 须仍由引导排他拥有；全部提交后返回同一已启用清理所有者。
    pub(crate) fn publish(
        self,
        bridge: &mut AgentRuntimeBridge,
        catalog: &Arc<VulcanPluginCatalogSnapshot>,
    ) -> Result<PublishedMcpBootstrap, String> {
        for native in &self.originals {
            // The bridge's outer ownership precedes hook preparation, core source and native exclusion.
            // 桥外层所有权先于 Hook 准备、核心来源及原生互斥。
            bridge.publish_native_plugin_extension_generation(native, Arc::clone(catalog))?;
        }
        // Controls are now non-owning and drop without closing the service-owned actual execution leases.
        // 控制现在为非拥有，销毁不会关闭服务拥有的实际执行租约。
        drop(self.originals);
        Ok(PublishedMcpBootstrap { guard: self.guard })
    }

    /// Clone independent original observations for real startup-boundary verification.
    /// 为真实启动边界验证克隆独立原观察。
    /// No inputs; return no physical inputs, transport or execution lease.
    /// 无输入；不返回物理输入、传输或执行租约。
    #[cfg(test)]
    pub(crate) fn observations(
        &self,
    ) -> Vec<vulcan_agent_runtime::mcp::PublishedPluginMcpObservation> {
        self.originals
            .iter()
            .map(PublishedPluginMcpRuntime::observer)
            .collect()
    }
}

/// Full native commits still require the final service-readiness boundary before startup cleanup is disarmed.
/// 完整原生提交仍要求最终服务就绪边界，之后才能解除启动清理。
pub(crate) struct PublishedMcpBootstrap {
    /// The same original cleanup lineage covers late bootstrap failures after the bridge has been exposed.
    /// 同一原清理谱系覆盖桥暴露之后的启动后期失败。
    guard: McpBootstrapFailureGuard,
}

impl PublishedMcpBootstrap {
    /// Transfer cleanup responsibility to final ready service ownership; no input or result is required.
    /// 将清理责任转交最终就绪服务所有权；无需输入及结果。
    /// This consumes the one published bootstrap owner without retaining historical runtime controls.
    /// 此操作消费单个已发布启动所有者，不保留历史运行时控制。
    pub(crate) fn commit(mut self) {
        self.guard.access.take();
    }
}
