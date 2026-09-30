//! Candidate MCP preparation retains the exact unpublished request without weakening ordinary routing.
//! 候选 MCP 准备保留精确未发布请求，不削弱普通路由校验。

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use uuid::Uuid;

use super::retain_candidate_readiness;
use crate::{
    VulcanPluginActivationCandidate, VulcanPluginDescriptor, VulcanPluginMcpServerDescriptor,
    VulcanPluginMcpStdioLaunchSpec, VulcanPluginMcpTransportManifest, VulcanPluginRuntimeError,
    VulcanPluginRuntimeManager,
};

/// Immutable candidate MCP inputs that must remain owned until actual readiness resources retire.
/// 不可变候选 MCP 输入，必须持有至实际就绪资源退役。
/// Cloning preserves the original request lock; this object does not launch or publish a transport.
/// 克隆保留原请求锁；此对象不会启动或发布传输。
#[derive(Clone)]
pub struct VulcanPluginPreparedMcpCandidate {
    /// One shared owner keeps package, credentials, configuration and request authority together.
    /// 单个共享所有者将插件包、凭据、配置与请求权威保留在一起。
    inner: Arc<PreparedCandidateMcpResources>,
}

/// Actual retained resources and host-resolved launchers for a single inspected request.
/// 单个已检查请求实际保留的资源及宿主解析启动器。
struct PreparedCandidateMcpResources {
    /// Exact disabled descriptor retains real immutable package and credential reader leases.
    /// 精确停用描述符保留真实不可变插件包与凭据读租约。
    plugin: VulcanPluginDescriptor,
    /// Durable request identity belongs to this owner and is never used as a substitute for its lock.
    /// 持久请求身份属于此所有者，绝不替代其锁。
    request_id: Uuid,
    /// Launchers are materialized solely from this candidate's own validated declarations.
    /// 启动器仅从此候选自身的已校验声明物化。
    stdio_launchers: BTreeMap<String, VulcanPluginMcpStdioLaunchSpec>,
    /// Original exclusive request ownership survives every clone of this prepared candidate.
    /// 原排他请求所有权在此准备候选的每个克隆期间存续。
    _readiness: Arc<std::fs::File>,
    /// Stdio candidates retain the actual execution-copy lease before any launcher is prepared.
    /// 标准输入输出候选在任何启动器准备前保留实际执行副本租约。
    _environment: Option<std::fs::File>,
}

impl fmt::Debug for VulcanPluginPreparedMcpCandidate {
    /// Formats only identities and counts, excluding configuration, credentials and launch arguments.
    /// 仅格式化身份和数量，排除配置、凭据及启动参数。
    /// The formatter receives a redacted summary; returns its formatting result.
    /// 格式器接收脱敏摘要；返回格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VulcanPluginPreparedMcpCandidate")
            .field("plugin_id", &self.inner.plugin.id)
            .field("request_id", &self.inner.request_id)
            .field("server_count", &self.inner.plugin.mcp_servers.len())
            .finish_non_exhaustive()
    }
}

impl VulcanPluginPreparedMcpCandidate {
    /// Returns the exact plugin identity without consulting public discovery.
    /// 返回精确插件身份，不查询公开发现。
    /// No inputs; the returned string borrows this resource owner.
    /// 无输入；返回字符串借用此资源所有者。
    pub fn plugin_id(&self) -> &str {
        &self.inner.plugin.id
    }

    /// Returns the original durable request identity for diagnostics and explicit coordination.
    /// 返回原持久请求身份，供诊断与显式协调使用。
    /// No inputs; copying this identity alone does not retain readiness resources.
    /// 无输入；仅复制此身份不会保留就绪资源。
    pub fn request_id(&self) -> Uuid {
        self.inner.request_id
    }

    /// Borrows exact server declarations with their frozen configuration and credential bindings.
    /// 借用带固定配置和凭据绑定的精确服务器声明。
    /// No inputs; descriptor clones retain their credentials but do not replace package or request ownership.
    /// 无输入；描述符克隆保留其凭据，但不能替代插件包或请求所有权。
    pub fn servers(&self) -> &[VulcanPluginMcpServerDescriptor] {
        &self.inner.plugin.mcp_servers
    }

    /// Borrows the exact prepared stdio launcher by manifest server identity.
    /// 按清单服务器身份借用精确准备的标准输入输出启动器。
    /// Unknown and remote server IDs are explicit errors; no public-catalog fallback is attempted.
    /// 未知及远端服务器 ID 明确报错；不尝试公开目录回退。
    pub fn stdio_launcher(
        &self,
        server_id: &str,
    ) -> Result<&VulcanPluginMcpStdioLaunchSpec, VulcanPluginRuntimeError> {
        self.inner.stdio_launchers.get(server_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Activation candidate {} has no prepared stdio server `{}:{server_id}`.",
                self.inner.request_id, self.inner.plugin.id
            ))
        })
    }
}

impl VulcanPluginRuntimeManager {
    /// Prepares declared MCP launchers from one inspected candidate while retaining its original request.
    /// 保留原请求，同时从单个已检查候选准备已声明 MCP 启动器。
    /// The candidate supplies all selected authorities; returns immutable owned inputs without runtime admission.
    /// 候选提供全部选定权威；返回不可变自有输入，不授予运行时入场。
    pub fn prepare_mcp_activation_candidate(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
    ) -> Result<VulcanPluginPreparedMcpCandidate, VulcanPluginRuntimeError> {
        // Retain before filesystem preparation so cancellation of an outer observer cannot unlock live work.
        // 文件系统准备前先保留，使外层观察者取消不能解除实际工作的锁。
        let readiness = retain_candidate_readiness(&self.state, candidate)?;
        // The exact descriptor carries approved permissions, selected configuration and credential leases.
        // 精确描述符携带已批准权限、选定配置及凭据租约。
        let plugin = candidate.package().clone();
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Activation candidate has no valid manifest.")
        })?;
        // Only stdio declarations need a filesystem launcher; remote declarations remain unchanged.
        // 仅标准输入输出声明需要文件系统启动器；远端声明保持不变。
        let mut stdio_launchers = BTreeMap::new();
        // Remote-only candidates need no local execution copy or filesystem lease.
        // 纯远端候选无需本地执行副本或文件系统租约。
        let environment = if plugin.mcp_servers.iter().any(|server| {
            matches!(
                server.manifest.transport,
                VulcanPluginMcpTransportManifest::Stdio { .. }
            )
        }) {
            Some(
                super::super::environment::retain_plugin_execution_environment(
                    &self.state,
                    &plugin.id,
                )
                .map_err(VulcanPluginRuntimeError::new)?,
            )
        } else {
            None
        };
        for server in &plugin.mcp_servers {
            if let VulcanPluginMcpTransportManifest::Stdio { launcher, .. } =
                &server.manifest.transport
            {
                let resolved = super::super::process::resolve_plugin_mcp_stdio_launcher(
                    &self.state,
                    &plugin,
                    manifest,
                    launcher,
                )
                .map_err(VulcanPluginRuntimeError::new)?;
                stdio_launchers.insert(server.id.clone(), resolved);
            }
        }
        Ok(VulcanPluginPreparedMcpCandidate {
            inner: Arc::new(PreparedCandidateMcpResources {
                plugin,
                request_id: candidate.transaction_record().request_id(),
                stdio_launchers,
                _readiness: readiness,
                _environment: environment,
            }),
        })
    }
}

#[cfg(test)]
mod tests;

mod target;
pub use target::{
    VulcanPluginMcpTargetHandoffGuard, VulcanPluginMcpTargetStartGuard,
    VulcanPluginPreparedMcpTarget, VulcanPluginRegisteredNativeMcpHostAuthority,
};
