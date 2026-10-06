//! Materializes candidate MCP inputs while retaining their original unpublished authority.
//! 物化候选 MCP 输入，同时保留其原未发布权威。

use super::*;
use vulcan_plugin_runtime::{
    VulcanPluginMcpStdioLaunchSpec, VulcanPluginPreparedMcpCandidate,
    VulcanPluginPreparedMcpPredecessor, VulcanPluginPreparedMcpTarget,
    VulcanPluginPreparedPublishedMcp, VulcanPluginRegisteredNativeMcpHostAuthority,
};

/// A declared source controls launcher resolution and published configuration eligibility.
/// 声明来源控制启动器解析及已发布配置读取资格。
pub(super) enum PluginMcpMaterializationSource<'a> {
    /// Ordinary declarations remain subject to the manager's current generation checks.
    /// 普通声明继续受管理器当前代次检查约束。
    Published(&'a VulcanPluginRuntimeManager),
    /// Candidate declarations use only the prepared request's frozen launchers and selections.
    /// 候选声明仅使用准备请求的固定启动器及选择。
    Candidate(&'a VulcanPluginPreparedMcpCandidate),
    /// Actual selected target retains exact immutable selections and already checked launchers.
    /// 实际已选目标保留精确不可变选择及已检查启动器。
    Target(&'a VulcanPluginPreparedMcpTarget),
    /// Restored old declarations retain their own immutable preparation and never read the target.
    /// 恢复旧声明保留自身不可变准备，绝不读取目标。
    Predecessor(&'a VulcanPluginPreparedMcpPredecessor),
    /// Prepared ordinary resources resolve only their original launchers under the same host issuer.
    /// 已准备普通资源仅在同一宿主签发者下解析其原启动器。
    PreparedPublished {
        /// Original physical resources own the complete selected package and credential revision.
        /// 原物理资源持有完整已选插件包及凭据修订。
        prepared: &'a VulcanPluginPreparedPublishedMcp,
        /// This borrowed issuer must be the one that originally prepared those resources.
        /// 此借用签发者必须是最初准备这些资源的同一个。
        authority: &'a VulcanPluginRegisteredNativeMcpHostAuthority,
    },
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
            Self::Target(target) => target
                .stdio_launcher(&server.id)
                .cloned()
                .map_err(|error| error.to_string()),
            Self::Predecessor(predecessor) => predecessor
                .stdio_launcher(&server.id)
                .cloned()
                .map_err(|error| error.to_string()),
            Self::PreparedPublished {
                prepared,
                authority,
            } => prepared
                .stdio_launcher(authority, &server.id)
                .cloned()
                .map_err(|error| error.to_string()),
        }
    }

    /// Reads the manager's published configuration only for a declaration explicitly sourced from public discovery.
    /// 仅对明确来自公开发现的声明读取管理器已发布配置。
    /// The plugin identity is exact; a candidate missing frozen selection fails without fallback.
    /// 插件身份精确；候选缺少固定选择时直接失败，不回退。
    pub(super) fn published_configuration(&self, plugin_id: &str) -> Result<Option<Value>, String> {
        match self {
            Self::Published(plugins) => plugin_server_configuration(plugins, plugin_id),
            Self::Candidate(_) | Self::Target(_) | Self::Predecessor(_) => Err(String::from(
                "Prepared activation authority is missing its frozen configuration selection.",
            )),
            Self::PreparedPublished { .. } => Err(String::from(
                "Prepared published MCP is missing its frozen configuration selection.",
            )),
        }
    }
}

/// Sealed old definitions retain their actual original restoration resources until native retirement.
/// 密封旧定义保留其实际原恢复资源，直至原生退役。
#[derive(Debug)]
pub struct PreparedPredecessorPluginMcpConfigs {
    /// Physical old selection and environment stay attached to every materialized input.
    /// 物理旧选择及环境始终附着于每个物化输入。
    predecessor: VulcanPluginPreparedMcpPredecessor,
    /// Exact global definitions originate only from the retained old manifest.
    /// 精确全局定义仅源自保留旧清单。
    servers: Vec<McpServerConfig>,
}

impl PreparedPredecessorPluginMcpConfigs {
    /// Consume the sealed pair; no inputs, returning its same owner and definitions together.
    /// 消费密封组合；无输入，一起返回其相同所有者及定义。
    pub(in crate::mcp) fn into_runtime_inputs(
        self,
    ) -> (VulcanPluginPreparedMcpPredecessor, Vec<McpServerConfig>) {
        (self.predecessor, self.servers)
    }
}

/// Materialize predecessor's exact old global declarations using only its frozen credential binding.
/// 仅使用 predecessor 固定凭据绑定，物化其精确旧全局声明。
/// secrets reads that bound revision; return sealed inputs without granting startup or business access.
/// secrets 读取该绑定修订；返回密封输入，不授予启动或业务访问。
pub fn predecessor_global_mcp_server_configs(
    predecessor: VulcanPluginPreparedMcpPredecessor,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<PreparedPredecessorPluginMcpConfigs, String> {
    // Both launchers and values come from the same retained old selection.
    // 启动器及值均来自同一保留旧选择。
    let source = PluginMcpMaterializationSource::Predecessor(&predecessor);
    // Scope is authoritative in that same old declaration.
    // 作用域在该同一旧声明内具有权威。
    let servers = predecessor
        .servers()
        .iter()
        .filter(|server| server.manifest.scope == VulcanPluginMcpScopeManifest::Global)
        .map(|server| plugin_mcp_server_config(&source, server, secrets, None))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedPredecessorPluginMcpConfigs {
        predecessor,
        servers,
    })
}

/// Packages isolated candidate validation definitions with their original request resource owner.
/// 将隔离候选校验定义与原请求资源所有者一起封装。
/// This prepared object is not a published partition or an operating-system cleanup receipt.
/// 此准备对象不是已发布分区，也不是操作系统清理回执。
#[derive(Debug, Clone)]
pub struct PreparedPluginMcpConfigs {
    /// Actual package, credentials and request lock retained across every clone.
    /// 每个克隆都保留实际插件包、凭据与请求锁。
    candidate: VulcanPluginPreparedMcpCandidate,
    /// Exact definitions selected by the validation adapter; these inputs grant no conversation business admission.
    /// 校验适配器选择的精确定义；这些输入不授予会话业务入场。
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

    /// Borrows isolated validation definitions without releasing their request resource owner.
    /// 借用隔离校验定义，不释放其请求资源所有者。
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

/// Materialize the candidate's declared local installation checks without opening remote or public tool routes.
/// 物化候选声明的本地安装检查，不打开远程或公开工具路由。
/// Candidate and secrets retain the original authority; returns sealed configurations for local stdio servers.
/// candidate 及 secrets 保留原权威；返回本地标准输入输出服务器的密封配置。
pub fn candidate_local_mcp_server_configs(
    candidate: VulcanPluginPreparedMcpCandidate,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<PreparedPluginMcpConfigs, String> {
    // Local installation uses the same exact candidate materializer as global readiness.
    // 本地安装使用与全局就绪相同的精确候选物化器。
    let source = PluginMcpMaterializationSource::Candidate(&candidate);
    // Transport declaration alone selects local checks, preserving the existing installation contract.
    // 仅传输声明选择本地检查，保留既有安装契约。
    let servers = candidate
        .servers()
        .iter()
        .filter(|server| {
            matches!(
                server.manifest.transport,
                VulcanPluginMcpTransportManifest::Stdio { .. }
            )
        })
        .map(|server| plugin_mcp_server_config(&source, server, secrets, None))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedPluginMcpConfigs { candidate, servers })
}

/// Sealed actual target definitions retain the complete resource authority until transport retirement.
/// 密封实际目标定义保留完整资源权威，直至传输退役。
#[derive(Debug)]
pub struct PreparedTargetPluginMcpConfigs {
    /// Original selected resource owner cannot be substituted with candidate generation zero.
    /// 原已选资源所有者不能替换为候选零代次。
    target: VulcanPluginPreparedMcpTarget,
    /// Exact global definitions never detach from their owning selection.
    /// 精确全局定义绝不脱离所属选择。
    servers: Vec<McpServerConfig>,
}

impl PreparedTargetPluginMcpConfigs {
    /// Consume this sealed pair; no input and no arbitrary owner/config combination.
    /// 消费此密封组合；无输入，不允许任意所有者／配置组合。
    pub(in crate::mcp) fn into_runtime_inputs(
        self,
    ) -> (VulcanPluginPreparedMcpTarget, Vec<McpServerConfig>) {
        (self.target, self.servers)
    }
    /// Borrow the exact resource authority; no input and no ownership transfer.
    /// 借用精确资源权威；无输入，不转交所有权。
    pub fn target(&self) -> &VulcanPluginPreparedMcpTarget {
        &self.target
    }
}

/// Materialize actual target global declarations from the same retained selection and credential binding.
/// 从同一保留选择及凭据绑定物化实际目标全局声明。
/// target fixes every launcher and value source; secrets reads only its declared revision.
/// target 固定全部启动器及值来源；secrets 仅读取其声明修订。
/// Return sealed inputs for actual target preparation, without granting business admission.
/// 返回实际目标准备的密封输入，不授予业务入场。
pub fn target_global_mcp_server_configs(
    target: VulcanPluginPreparedMcpTarget,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<PreparedTargetPluginMcpConfigs, String> {
    // Read exact descriptor selections rather than a latest public manager catalog.
    // 读取精确描述符选择，而非管理器最新公开目录。
    let source = PluginMcpMaterializationSource::Target(&target);
    // Scope is declared by the same full target manifest.
    // 作用域由同一完整目标清单声明。
    let servers = target
        .servers()
        .iter()
        .filter(|server| server.manifest.scope == VulcanPluginMcpScopeManifest::Global)
        .map(|server| plugin_mcp_server_config(&source, server, secrets, None))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedTargetPluginMcpConfigs { target, servers })
}
