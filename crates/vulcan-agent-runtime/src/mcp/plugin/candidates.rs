//! Materializes candidate MCP inputs while retaining their original unpublished authority.
//! 物化候选 MCP 输入，同时保留其原未发布权威。

use super::*;
use vulcan_plugin_runtime::{VulcanPluginMcpStdioLaunchSpec, VulcanPluginPreparedMcpCandidate};

/// A declared source controls launcher resolution and legacy configuration eligibility.
/// 声明来源控制启动器解析及旧配置读取资格。
pub(super) enum PluginMcpMaterializationSource<'a> {
    /// Ordinary declarations remain subject to the manager's current generation checks.
    /// 普通声明继续受管理器当前代次检查约束。
    Published(&'a VulcanPluginRuntimeManager),
    /// Candidate declarations use only the prepared request's frozen launchers and selections.
    /// 候选声明仅使用准备请求的固定启动器及选择。
    Candidate(&'a VulcanPluginPreparedMcpCandidate),
}

impl PluginMcpMaterializationSource<'_> {
    /// Resolves the server's declared stdio launcher from this sole source.
    /// 从此唯一来源解析服务器声明的标准输入输出启动器。
    /// The server is selected by the owning adapter; returns a cloned exact launch specification.
    /// 服务器由所属适配器选择；返回克隆的精确启动规格。
    pub(super) fn stdio_launcher(
        &self,
        server: &VulcanPluginMcpServerDescriptor,
    ) -> Result<VulcanPluginMcpStdioLaunchSpec, String> {
        match self {
            Self::Published(plugins) => plugins
                .resolve_mcp_stdio_launcher_descriptor(server)
                .map_err(|error| error.to_string()),
            Self::Candidate(candidate) => candidate
                .stdio_launcher(&server.id)
                .cloned()
                .map_err(|error| error.to_string()),
        }
    }

    /// Reads legacy configuration only for a declaration explicitly sourced from public discovery.
    /// 仅对明确来自公开发现的声明读取旧配置。
    /// The plugin identity is exact; a candidate missing frozen selection fails without fallback.
    /// 插件身份精确；候选缺少固定选择时直接失败，不回退。
    pub(super) fn legacy_configuration(&self, plugin_id: &str) -> Result<Option<Value>, String> {
        match self {
            Self::Published(plugins) => plugin_server_configuration(plugins, plugin_id),
            Self::Candidate(_) => Err(String::from(
                "Activation candidate is missing its frozen configuration selection.",
            )),
        }
    }
}

/// Packages global candidate definitions with their original request resource owner.
/// 将全局候选定义与原请求资源所有者一起封装。
/// This prepared object is not a published partition or an operating-system cleanup receipt.
/// 此准备对象不是已发布分区，也不是操作系统清理回执。
#[derive(Debug, Clone)]
pub struct PreparedPluginMcpConfigs {
    /// Actual package, credentials and request lock retained across every clone.
    /// 每个克隆都保留实际插件包、凭据与请求锁。
    candidate: VulcanPluginPreparedMcpCandidate,
    /// Exact global definitions with resolved values; conversation scopes require a separate owner grant.
    /// 具有已解析值的精确全局定义；会话作用域需要独立所有者授权。
    servers: Vec<McpServerConfig>,
}

impl PreparedPluginMcpConfigs {
    /// Transfers the sealed original authority and its materialized definitions together.
    /// 将密封原权威与其物化定义一起移交。
    /// No inputs; consuming this object prevents callers from pairing an arbitrary configuration with its owner.
    /// 无输入；消费此对象，防止调用方将任意配置与其所有者配对。
    pub(in crate::mcp) fn into_runtime_inputs(
        self,
    ) -> (VulcanPluginPreparedMcpCandidate, Vec<McpServerConfig>) {
        (self.candidate, self.servers)
    }

    /// Borrows the original candidate authority for transfer to actual runtime resource ownership.
    /// 借用原候选权威，供实际运行时资源所有权接管。
    /// No inputs; copying identifiers or server configurations cannot replace retaining this owner.
    /// 无输入；复制身份或服务器配置不能替代保留此所有者。
    pub fn candidate(&self) -> &VulcanPluginPreparedMcpCandidate {
        &self.candidate
    }

    /// Borrows global definitions without releasing their request resource owner.
    /// 借用全局定义，不释放其请求资源所有者。
    /// No inputs; callers must retain this object throughout all actual readiness work.
    /// 无输入；调用方必须在全部实际就绪工作期间保留此对象。
    pub fn servers(&self) -> &[McpServerConfig] {
        &self.servers
    }
}

/// Materializes the prepared candidate's global declarations from its exact selections and secret revision.
/// 按准备候选的精确选择与秘密修订物化其全局声明。
/// The owned candidate and secret reader are authoritative; returns configs retaining the original owner.
/// 自有候选及秘密读取者具有权威；返回保留原所有者的配置对象。
/// No public manager is accepted, so candidate materialization cannot read a latest public selection.
/// 不接受公开管理器，因此候选物化不能读取最新公开选择。
pub fn candidate_global_mcp_server_configs(
    candidate: VulcanPluginPreparedMcpCandidate,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<PreparedPluginMcpConfigs, String> {
    // Configuration and launcher selection both use the same immutable candidate authority.
    // 配置及启动器选择均使用相同不可变候选权威。
    let source = PluginMcpMaterializationSource::Candidate(&candidate);
    let servers = candidate
        .servers()
        .iter()
        .filter(|server| server.manifest.scope == VulcanPluginMcpScopeManifest::Global)
        .map(|server| plugin_mcp_server_config(&source, server, secrets, None))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedPluginMcpConfigs { candidate, servers })
}
