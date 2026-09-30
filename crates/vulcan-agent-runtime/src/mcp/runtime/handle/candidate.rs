//! Isolated readiness control consumes the original sealed activation request.
//! 隔离就绪控制消费原密封激活请求。

use super::*;
use crate::mcp::PreparedPluginMcpConfigs;
use vulcan_mcp_runtime::{McpResourceRelease, McpTransportResources};

mod target;
pub use target::{TargetPluginMcpHandoffGuard, TargetPluginMcpRuntime};

/// Controls one unpublished readiness run without exposing business-tool admission.
/// 控制单次未发布就绪运行，不暴露业务工具入场。
/// Dropping control requests shutdown; observations remain usable until actual owners release.
/// 销毁控制对象请求关闭；观察对象可持续查询至实际所有者释放。
pub struct CandidatePluginMcpRuntime {
    /// The original control state and release receipt, never a replacement request.
    /// 原控制状态与释放回执，绝非替代请求。
    observation: CandidatePluginMcpObservation,
}

/// Observes the original request without retaining its package or environment authority.
/// 观察原请求，不保留其插件包或环境权威。
/// Release proves local ownership ended, not readiness, protocol cleanup, or side-effect rollback.
/// 释放证明本地所有权结束，不证明就绪、协议清理或副作用回滚。
#[derive(Clone)]
pub struct CandidatePluginMcpObservation {
    /// Original lifecycle diagnostics; abandoned lifecycle tasks revoke retained transport entries.
    /// 原生命周期诊断；被放弃的生命周期任务撤销保留的传输条目。
    runtime: McpRuntimeHandle,
    /// Aggregate evidence includes all lifecycle tasks and all their actual transport resources.
    /// 聚合证据包含全部生命周期任务及其全部实际传输资源。
    release: McpResourceRelease,
    /// Copied stable request identity contains no resource-owning reference.
    /// 复制的稳定请求身份不包含拥有资源的引用。
    request_id: uuid::Uuid,
    /// Copied plugin identity identifies the unpublished selection for diagnostics.
    /// 复制的插件身份用于标识未发布选择的诊断。
    plugin_id: String,
}

impl CandidatePluginMcpRuntime {
    /// Validate this original attempt and await its actual resource release using the host's existing readiness budget.
    /// 使用宿主既有就绪预算校验此原尝试，并等待其实际资源释放。
    /// Returns success only after readiness, protocol shutdown and release; cancellation requests original shutdown.
    /// 仅就绪、协议关闭及释放均完成后返回成功；取消请求原关闭。
    pub async fn validate_and_release(self) -> Result<(), String> {
        // The observation carries no resource lease, so waiting cannot retain the authority it is draining.
        // 观测不携带资源租约，因此等待不能保留正在排空的权威。
        let observation = self.observer();
        // Readiness, protocol cleanup and actual release are separate evidence and all must succeed.
        // 就绪、协议清理及实际释放是独立证据，必须全部成功。
        let ready = observation
            .wait_until_ready(MCP_PARTITION_READINESS_TIMEOUT)
            .await;
        self.shutdown();
        // Successful terminal state includes the original transport shutdown barrier.
        // 成功终态包含原传输关闭屏障。
        let stopped = observation
            .runtime
            .wait_until_stopped(MCP_PARTITION_READINESS_TIMEOUT)
            .await;
        // Resource release remains required even if readiness or protocol shutdown failed.
        // 即使就绪或协议关闭失败，仍要求实际资源释放。
        let released = observation
            .wait_until_released(MCP_PARTITION_READINESS_TIMEOUT)
            .await;
        // Preserve each failed boundary instead of interpreting local release as protocol success.
        // 保留每个失败边界，而不将本地释放解释为协议成功。
        let failures = [
            ("readiness", ready),
            ("shutdown", stopped),
            ("resource release", released),
        ]
        .into_iter()
        .filter_map(|(stage, result)| result.err().map(|error| format!("{stage}: {error}")))
        .collect::<Vec<_>>();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Candidate MCP validation remains incomplete: {}",
                failures.join("; ")
            ))
        }
    }

    /// Starts one isolated attempt using only a sealed prepared candidate and its frozen definitions.
    /// 仅用密封准备候选及其固定定义启动一次隔离尝试。
    /// Prepared is consumed synchronously; returns control before any cancellable readiness wait.
    /// 同步消费 prepared；在任何可取消就绪等待前返回控制对象。
    /// Validation errors occur before spawning tasks; this method never publishes or commits the candidate.
    /// 校验错误发生在启动任务前；此方法绝不发布或提交候选。
    pub fn start(prepared: PreparedPluginMcpConfigs) -> Result<Self, String> {
        // Move authority and definitions together so callers cannot substitute another selection.
        // 一起移动权威及定义，防止调用方替换为其他选择。
        let (candidate, servers) = prepared.into_runtime_inputs();
        // Copy the exact stable request identity without acquiring another resource lease.
        // 复制精确稳定请求身份，不获取其他资源租约。
        let request_id = candidate.request_id();
        // Copy the plugin identity for diagnostics without retaining its descriptor.
        // 复制插件身份供诊断，不保留其描述符。
        let plugin_id = candidate.plugin_id().to_owned();
        // Only lifecycle tasks and their actual resources receive aggregate ownership.
        // 仅生命周期任务及其实际资源接收聚合所有权。
        let (resources, release) = McpTransportResources::new(candidate);
        // The original runtime owns execution while observers retain only its control state.
        // 原运行时拥有执行，观察者仅保留其控制状态。
        let runtime =
            create_mcp_runtime(servers, resources, McpRuntimePurpose::CandidateReadiness)?;
        Ok(Self {
            observation: CandidatePluginMcpObservation {
                runtime,
                release,
                request_id,
                plugin_id,
            },
        })
    }

    /// Clones the original observation without starting work or acquiring another request lease.
    /// 克隆原观察对象，不启动工作或获取其他请求租约。
    /// No inputs; returns diagnostics and the same retained release evidence.
    /// 无输入；返回诊断及相同保留释放证据。
    pub fn observer(&self) -> CandidatePluginMcpObservation {
        self.observation.clone()
    }

    /// Requests latched shutdown of the original attempt without waiting or replaying initialization.
    /// 请求原尝试的锁存关闭，不等待或重放初始化。
    /// No inputs or return value; use the observer to await actual resource release separately.
    /// 无输入与返回值；使用观察对象单独等待实际资源释放。
    pub fn shutdown(&self) {
        self.observation.runtime.shutdown();
    }
}

impl Drop for CandidatePluginMcpRuntime {
    /// Ensures abandoned readiness control requests cleanup on the original lifecycle.
    /// 确保被放弃的就绪控制向原生命周期请求清理。
    /// No inputs or return value; destruction never asserts that asynchronous cleanup finished.
    /// 无输入与返回值；析构绝不宣称异步清理已完成。
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl CandidatePluginMcpObservation {
    /// Returns the exact original activation request identity without resolving current state.
    /// 返回精确原激活请求身份，不解析当前选择。
    /// No inputs; returns the stable identifier copied before launch.
    /// 无输入；返回启动前复制的稳定标识。
    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }

    /// Returns the exact plugin identity attached to this original request.
    /// 返回附着于此原请求的精确插件身份。
    /// No inputs; returns a borrowed diagnostic identifier, not plugin authority.
    /// 无输入；返回借用的诊断标识，不是插件权威。
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Reads lifecycle diagnostics without exposing transport handles or callable tools.
    /// 读取生命周期诊断，不暴露传输句柄或可调用工具。
    /// No inputs; returns current states, preserving failure independently of release evidence.
    /// 无输入；返回当前状态，独立于释放证据保留失败。
    pub fn server_states(&self) -> Vec<McpRuntimeServerState> {
        self.runtime.server_states()
    }

    /// Observes readiness of the original single attempt without publishing or cancelling it.
    /// 观察原单次尝试的就绪状态，不发布或取消该尝试。
    /// Maximum wait is host-owned; returns an error on failure, shutdown, or expiration.
    /// maximum_wait 由宿主持有；失败、关闭或到期时返回错误。
    /// Cancelling this wait leaves original control and ownership intact.
    /// 取消此等待保留原控制及所有权。
    pub async fn wait_until_ready(&self, maximum_wait: Duration) -> Result<(), String> {
        // The latched signal also rejects a stale Connected snapshot after shutdown was requested.
        // 锁存信号同时拒绝请求关闭之后过期的已连接快照。
        let mut shutdown = self.runtime.inner.shutdown_tx.subscribe();
        if *shutdown.borrow() {
            return Err(String::from("Candidate MCP readiness was stopped."));
        }
        // Readiness and the latched stop signal race only within this observation.
        // 就绪与锁存停止信号仅在此次观察中竞争。
        let result = tokio::select! {
            _ = shutdown.changed() => Err(String::from("Candidate MCP readiness was stopped.")),
            result = self.runtime.wait_until_ready(maximum_wait) => result,
        };
        if *shutdown.borrow() {
            return Err(String::from("Candidate MCP readiness was stopped."));
        }
        result
    }

    /// Waits for all original lifecycle and actual transport references to release their authority.
    /// 等待全部原生命周期及实际传输引用释放其权威。
    /// Maximum wait bounds only this observation; returns actual release evidence or an explicit error.
    /// maximum_wait 仅限制此次观察；返回实际释放证据或明确错误。
    /// This does not request shutdown, commit a transaction, or prove remote side effects were undone.
    /// 此操作不请求关闭、不提交事务，也不证明远程副作用已撤销。
    pub async fn wait_until_released(&self, maximum_wait: Duration) -> Result<(), String> {
        tokio::time::timeout(maximum_wait, self.release.wait())
            .await
            .map_err(|_| {
                format!(
                    "Candidate MCP request `{}` resource release timed out.",
                    self.request_id
                )
            })?
            .map_err(|error| error.to_string())
    }
}
