//! Proven ordinary startup registers the actual original before dispatch and retains no files in observers.
//! 已证明普通启动在派发前登记实际原实例，观察者不保留文件。

use super::*;
use crate::agent_runtime::extension_generation::ExtensionGeneration;
use crate::mcp::PreparedPublishedPluginMcpConfigs;
use std::sync::Weak;
use vulcan_plugin_runtime::{
    VulcanPluginPublishedMcpProfile, VulcanPluginRegisteredNativeMcpHostAuthority,
    VulcanPluginRuntimeManager,
};

mod publication;
#[cfg(test)]
mod tests;
pub use publication::{PreparedPublishedPluginMcpPublication, PublishedPluginMcpPublicationGuard};

/// Actual lease ownership transfers once; historical confirmation retains only weak original identities.
/// 实际租约所有权仅转交一次；历史确认仅保留弱原身份。
enum PublishedMcpLeaseState {
    /// Unpublished control owns the exact lease until a full extension accepts it.
    /// 未发布控制拥有精确租约，直至完整扩展接纳它。
    Preparing(Arc<McpRuntimePartitionLease>),
    /// Same actual committed extension and lease remain identifiable without prolonging execution.
    /// 无需延长执行即可持续识别同一实际已提交扩展及租约。
    Published {
        /// Weak actual lease identity never prevents the last published execution owner from dropping.
        /// 弱实际租约身份绝不阻止最后一个已发布执行所有者销毁。
        lease: Weak<McpRuntimePartitionLease>,
        /// Weak full-pointer witness is recorded adjacent to its one actual replacement.
        /// 弱完整指针见证在其单次实际替换的相邻处记录。
        extension: Weak<ExtensionGeneration>,
    },
}

/// Single-shot control owns the original closed lease; abandonment requests its real cleanup.
/// 单次控制拥有原关闭租约；放弃请求其真实清理。
/// This control is not a full publication witness or permission to restore a predecessor.
/// 此控制不是完整发布见证，也不是恢复前驱的许可。
pub struct PublishedPluginMcpRuntime {
    /// Original actual lease is allocated once before the first task starts.
    /// 原实际租约在首个任务启动前仅分配一次。
    lease: Mutex<PublishedMcpLeaseState>,
    /// Original admission lineage retains cleanup controls even if this return is lost.
    /// 即使此返回丢失，原入场谱系也保留清理控制。
    access: McpRuntimeObservationAccess,
    /// Pure source and original independent control survive without owning native resources.
    /// 纯来源及原独立控制无需拥有原生资源即可存续。
    observation: PublishedPluginMcpObservation,
}

/// Original readiness and cleanup observations retain neither package pins nor environment files.
/// 原就绪及清理观察既不保留插件包固定，也不保留环境文件。
#[derive(Clone)]
pub struct PublishedPluginMcpObservation {
    /// Same actual control contains live transport and sealed cleanup observations.
    /// 同一实际控制包含实时传输及密封清理观察。
    runtime: McpRuntimeHandle,
    /// This opaque source must separately match the actual launched input Arc before capture.
    /// 捕获之前，此不透明来源须另行匹配实际已启动输入 Arc。
    profile: VulcanPluginPublishedMcpProfile,
    /// Non-secret routing identity is copied from the original resource owner.
    /// 非秘密路由身份从原资源所有者复制。
    plugin_id: String,
    /// Original route metadata never substitutes for native ownership or source identity.
    /// 原路由元数据绝不替代原生所有权或来源身份。
    generation: u64,
}

impl PublishedPluginMcpRuntime {
    /// Start sealed prepared inputs under manager's original source and authority before any readiness wait.
    /// 任何就绪等待之前，在 manager 原来源及 authority 下启动密封 prepared 输入。
    /// current supplies the existing admission lineage; return a closed same-instance control or a pre-dispatch error.
    /// current 提供既有入场谱系；返回关闭的同实例控制或派发前错误。
    pub fn start(
        current: &McpRuntimeAccess,
        manager: &VulcanPluginRuntimeManager,
        prepared: PreparedPublishedPluginMcpConfigs,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<Self, String> {
        // Sealed consumption moves exact definitions together with their full physical resource authority.
        // 密封消费将精确定义与其完整物理资源权威共同移动。
        let (prepared, servers) = prepared.into_runtime_inputs();
        // Obtain identity directly even if this full published source has no global declarations.
        // 即使此完整已发布来源没有全局声明，也直接取得身份。
        let (plugin_id, generation) = prepared
            .identity(authority)
            .map_err(|error| error.to_string())?;
        // Copy routing identity before resource ownership transfers to native tasks.
        // 资源所有权转交原生任务之前复制路由身份。
        let plugin_id = plugin_id.to_owned();
        if servers.iter().any(|server| {
            server.source
                != (crate::mcp::McpServerSource::Plugin {
                    plugin_id: plugin_id.clone(),
                    generation,
                })
        }) {
            return Err(String::from(
                "Published MCP definitions differ from their original resource owner.",
            ));
        }
        // Native tasks alone retain actual package, credential and environment ownership after dispatch.
        // 派发后仅原生任务保留实际插件包、凭据及环境所有权。
        let (resources, _) = vulcan_mcp_runtime::McpTransportResources::new(prepared.clone());
        // Real manager and cross-store File fences stay held until control registration and dispatch finish.
        // 实际管理器及跨存储 File 屏障保持持有，直至控制登记及派发完成。
        let start = prepared
            .begin_dispatch(manager, authority)
            .map_err(|error| error.to_string())?;
        // One profile identity belongs to this same original source and every actual launched input Arc.
        // 单个画像身份属于此同一原来源及每个实际已启动输入 Arc。
        let profile = start.host_profile();
        // The existing tables register the actual control before their first native task can run.
        // 首个原生任务可能运行之前，既有表登记实际控制。
        let lease = current.start_published_plugin_partition(
            &plugin_id,
            servers,
            resources,
            profile.clone(),
        )?;
        drop(start);
        Ok(Self {
            observation: PublishedPluginMcpObservation {
                runtime: lease.runtime.clone(),
                profile,
                plugin_id,
                generation,
            },
            lease: Mutex::new(PublishedMcpLeaseState::Preparing(lease)),
            access: current.observation_access(),
        })
    }

    /// Clone the original non-owning observation; no inputs and no additional execution resources.
    /// 克隆原非拥有观察；无输入，不增加执行资源。
    pub fn observer(&self) -> PublishedPluginMcpObservation {
        self.observation.clone()
    }

    /// Request cleanup of the original instance without asserting actual retirement has completed.
    /// 请求原实例清理，不断言实际退役已完成。
    /// No inputs or return value; independent observations retain the original failure and release evidence.
    /// 无输入及返回值；独立观察保留原失败及释放证据。
    pub fn shutdown(&self) {
        self.observation.runtime.shutdown();
    }

    /// Compare the original actual full commit against current without retaining its resources.
    /// 不保留其资源，对照 current 比较原实际完整提交。
    /// Returns true only for the recorded pointer and exact lease; this read grants no publication or restoration authority.
    /// 仅已记录指针及精确租约返回真；此读取不授予发布或恢复权威。
    pub fn matches_original_publication(&self, current: &Arc<ExtensionGeneration>) -> bool {
        // Original ownership state is the sole confirmation source, not a second publication latch.
        // 原所有权状态是唯一确认来源，不使用第二个发布锁存位。
        let state = self.lease.lock().expect("published MCP ownership poisoned");
        match &*state {
            PublishedMcpLeaseState::Preparing(_) => false,
            PublishedMcpLeaseState::Published { lease, extension } => {
                Weak::ptr_eq(extension, &Arc::downgrade(current))
                    && current.mcp_runtime().contains_exact_plugin_lease(
                        &self.observation.plugin_id,
                        self.observation.generation,
                        lease,
                    )
            }
        }
    }
}

impl Drop for PublishedPluginMcpRuntime {
    /// Abandoned preparation closes the same actual original; destruction never starts replacement work.
    /// 被放弃准备关闭同一实际原实例；析构绝不启动替代工作。
    fn drop(&mut self) {
        if matches!(
            *self
                .lease
                .get_mut()
                .expect("published MCP ownership poisoned"),
            PublishedMcpLeaseState::Preparing(_)
        ) {
            self.shutdown();
        }
    }
}

impl PublishedPluginMcpObservation {
    /// Borrow original non-secret routing metadata; no inputs and no execution authority is returned.
    /// 借用原非秘密路由元数据；无输入，不返回执行权威。
    pub fn identity(&self) -> (&str, u64) {
        (&self.plugin_id, self.generation)
    }

    /// Clone pure original source values; no inputs and no actual-launch or restoration receipt is created.
    /// 克隆纯原来源值；无输入，不创建实际启动或恢复回执。
    pub fn profile(&self) -> VulcanPluginPublishedMcpProfile {
        self.profile.clone()
    }

    /// Verify server_id's same actual connected launch consumed expected without exposing protected inputs.
    /// 不暴露受保护输入，校验 server_id 同一实际已连接启动消费了 expected。
    /// Return success only for original transport, input Arc, source and live native release; errors contain no values.
    /// 仅原传输、输入 Arc、来源及存续原生释放均成立时成功；错误不含值。
    pub fn verify_source(
        &self,
        server_id: &str,
        expected: &VulcanPluginPublishedMcpProfile,
    ) -> Result<(), String> {
        self.runtime
            .with_connected_published_launch_inputs(server_id, expected, |_, _| ())
            .map_err(|error| format!("Published MCP launch source is unavailable: {error:?}."))
    }

    /// Read original live diagnostics; no inputs and no native transport or protected input is exposed.
    /// 读取原实时诊断；无输入，不暴露原生传输或受保护输入。
    pub fn server_states(&self) -> Vec<McpRuntimeServerState> {
        self.runtime.server_states()
    }

    /// Observe same-instance readiness within maximum_wait without publishing, cancelling or replaying work.
    /// 在 maximum_wait 内观测同实例就绪，不发布、取消或重播工作。
    /// Return the real discovery result or original error; cancelling this future retains the original control.
    /// 返回真实发现结果或原错误；取消此 future 保留原控制。
    pub async fn wait_until_ready(&self, maximum_wait: Duration) -> Result<(), String> {
        self.runtime.wait_until_ready(maximum_wait).await
    }

    /// Observe initial discovery using the runtime's sole ordinary bootstrap deadline.
    /// 使用运行时唯一普通启动截止时间观测首次发现。
    /// No inputs; return the same readiness result without opening business or retaining additional resources.
    /// 无输入；返回同一就绪结果，不开放业务或保留额外资源。
    pub async fn wait_until_bootstrap_ready(&self) -> Result<(), String> {
        self.wait_until_ready(MCP_PARTITION_READINESS_TIMEOUT).await
    }

    /// Observe original cleanup and aggregate physical release within maximum_wait using the sole cleanup predicate.
    /// 使用唯一清理判定，在 maximum_wait 内观测原清理及聚合物理释放。
    /// Return success only for actual original retirement; cleanup errors remain errors even if resources released.
    /// 仅实际原退役返回成功；即使资源已释放，清理错误仍保持错误。
    pub async fn wait_until_released(&self, maximum_wait: Duration) -> Result<(), String> {
        self.runtime
            .wait_until_drained(tokio::time::Instant::now() + maximum_wait)
            .await
    }
}
