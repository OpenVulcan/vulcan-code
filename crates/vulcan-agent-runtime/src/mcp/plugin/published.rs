//! Ordinary published materialization retains its original resources with their exact runtime definitions.
//! 普通已发布物化将原资源与其精确运行时定义共同保留。

use super::*;
use vulcan_plugin_runtime::{
    VulcanPluginPreparedPublishedMcp, VulcanPluginRegisteredNativeMcpHostAuthority,
};

/// Sealed ordinary inputs bind original full resource ownership to its host-materialized definitions.
/// 密封普通输入将原完整资源所有权绑定至其宿主物化定义。
/// This type cannot be constructed from detached configurations, cloned or serialized.
/// 此类型不能从独立配置构造、克隆或序列化。
pub struct PreparedPublishedPluginMcpConfigs {
    /// Actual selected package, credentials and environment remain owned until native retirement.
    /// 实际已选插件包、凭据及环境保持自有，直至原生退役。
    prepared: VulcanPluginPreparedPublishedMcp,
    /// Original non-secret route is captured even if the global declaration set is empty.
    /// 即使全局声明集合为空，也捕获原非秘密路由。
    plugin_id: String,
    /// The exact route generation is metadata, never a substitute for resource or source identity.
    /// 精确路由代次是元数据，绝不替代资源或来源身份。
    generation: u64,
    /// Definitions derive only from the same frozen declaration, configuration and credential binding.
    /// 定义仅从同一固定声明、配置及凭据绑定派生。
    servers: Vec<McpServerConfig>,
}

impl PreparedPublishedPluginMcpConfigs {
    /// Borrow non-secret routing metadata; no inputs and no execution authority is returned.
    /// 借用非秘密路由元数据；无输入，不返回执行权威。
    pub fn identity(&self) -> (&str, u64) {
        (&self.plugin_id, self.generation)
    }

    /// Borrow materialized definitions for inspection without detaching their original resource owner.
    /// 借用物化定义以供检查，不使其脱离原资源所有者。
    /// No inputs; callers must keep this sealed object alive until its complete transfer to native work.
    /// 无输入；调用方必须保留此密封对象，直至完整移交至原生工作。
    pub fn servers(&self) -> &[McpServerConfig] {
        &self.servers
    }

    /// Validate authority's exact original issuer before a mixed bootstrap can dispatch unrelated work.
    /// 在混合引导可能派发无关工作之前，校验 authority 精确原签发者。
    /// Return no source or execution authority; failure contains no protected input value.
    /// 不返回来源或执行权威；失败不包含受保护输入值。
    pub(in crate::mcp) fn validate_original_host(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<(), String> {
        self.prepared
            .identity(authority)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Consume the original prepared resources and exact definitions together for native registration.
    /// 为原生登记共同消费原已准备资源及精确定义。
    /// No inputs; only the internal runtime can receive this pair, preventing arbitrary re-pairing.
    /// 无输入；仅内部运行时可接收此组合，防止任意重新配对。
    pub(in crate::mcp) fn into_runtime_inputs(
        self,
    ) -> (VulcanPluginPreparedPublishedMcp, Vec<McpServerConfig>) {
        (self.prepared, self.servers)
    }
}

/// Materialize prepared global declarations using their original authority and exact credential backend.
/// 使用原 authority 及精确凭据后端物化 prepared 全局声明。
/// Return sealed resources and definitions; errors neither launch work nor consult a current manager.
/// 返回密封资源及定义；错误既不启动工作，也不查询当前管理器。
pub fn published_global_mcp_server_configs(
    prepared: VulcanPluginPreparedPublishedMcp,
    authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<PreparedPublishedPluginMcpConfigs, String> {
    // Obtain original route metadata directly from the owner rather than an optional first server.
    // 直接从所有者取得原路由元数据，而非可缺失的首个服务器。
    let (plugin_id, generation) = prepared
        .identity(authority)
        .map_err(|error| error.to_string())?;
    // Retain this validated route while the source remains borrowed during materialization.
    // 来源在物化期间保持借用时，保留此已校验路由。
    let plugin_id = plugin_id.to_owned();
    // One explicit source fixes launcher and configuration behavior for all actual declarations.
    // 单个显式来源固定全部实际声明的启动器及配置行为。
    let source = PluginMcpMaterializationSource::PreparedPublished {
        prepared: &prepared,
        authority,
    };
    // Scope and all fields come from the same actual immutable descriptor owned by prepared.
    // 作用域及全部字段来自 prepared 拥有的同一实际不可变描述符。
    let servers = prepared
        .servers(authority)
        .map_err(|error| error.to_string())?
        .iter()
        .filter(|server| server.manifest.scope == VulcanPluginMcpScopeManifest::Global)
        .map(|server| plugin_mcp_server_config(&source, server, secrets, None))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedPublishedPluginMcpConfigs {
        prepared,
        plugin_id,
        generation,
        servers,
    })
}
