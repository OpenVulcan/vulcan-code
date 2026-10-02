//! Original cleanup results are positive evidence, independent from startup diagnostics.
//! 原清理结果是正向证据，独立于启动诊断。

use super::*;
use std::sync::Weak;

/// Sealed observation binds a real cleanup outcome to the original transport and release receipt.
/// 密封观察将真实清理结果绑定原传输及释放回执。
/// Constructors are internal to actual cleanup and abnormal lifecycle disposal; no UUID or counter substitutes ownership.
/// 构造器仅供实际清理及异常生命周期销毁使用；不以 UUID 或计数替代所有权。
#[derive(Clone)]
pub(super) struct McpActualCleanupObservation {
    /// Weak identity identifies the actual transport without preventing its destruction.
    /// 弱身份标识实际传输，不阻止其销毁。
    original_transport: Option<Weak<dyn McpTransport>>,
    /// The original non-owning native release observation remains attached to this same cleanup result.
    /// 原非拥有原生释放观察持续绑定此相同清理结果。
    original_release: Option<vulcan_mcp_runtime::McpResourceRelease>,
    /// Every observer reads the same original operation's final result; None means its sole producer is pending.
    /// 每个观察者读取相同原操作的最终结果；None 表示其唯一产出者仍待决。
    completion: watch::Receiver<Option<McpActualCleanupResult>>,
}

/// The real original operation returns sanitized logs and its exact protocol/resource outcome.
/// 真实原操作返回脱敏日志及精确协议和资源结果。
pub(super) type McpActualCleanupResult = Result<Vec<String>, (Vec<String>, String)>;

/// Sole producer publishes its actual final result or a negative result when original ownership is lost.
/// 唯一产出者发布实际最终结果，或在原所有权丢失时发布负向结果。
pub(super) struct McpCleanupCompletionWriter {
    /// The channel belongs to the same actual operation installed before transport removal.
    /// 通道属于在传输移除之前登记的同一实际操作。
    completion: watch::Sender<Option<McpActualCleanupResult>>,
}

/// Sole admission result is shared by drain waiting and retired-owner pruning.
/// 唯一准入结果由排空等待及退役所有者清理共用。
pub(super) enum McpCleanupAdmission {
    /// Original shutdown or cleanup still lacks positive completion evidence.
    /// 原关闭或清理仍缺少正向完成证据。
    Pending,
    /// All actual original owners retired successfully after latched shutdown.
    /// 锁存关闭后，全部实际原所有者成功退役。
    Complete,
    /// The same original cleanup failed; native disposal remains independently observable.
    /// 同一原清理失败；原生销毁仍可独立观察。
    Failed(String),
}

impl McpActualCleanupObservation {
    /// Registers the original operation before effects and returns its sole final-result writer.
    /// 在效果发生前登记原操作，并返回其唯一最终结果写入者。
    /// Transport and release identify exact ownership; returns sealed observation and original producer authority.
    /// transport 及 release 标识精确所有权；返回密封观察及原产出者权威。
    pub(super) fn begin(
        transport: Option<Weak<dyn McpTransport>>,
        release: Option<vulcan_mcp_runtime::McpResourceRelease>,
    ) -> (Self, McpCleanupCompletionWriter) {
        // None is the pending result of this actual producer, never a separate runtime phase.
        // None 是此实际产出者的待决结果，绝不是单独运行时阶段。
        let (completion, receiver) = watch::channel(None);
        (
            Self {
                original_transport: transport,
                original_release: release,
                completion: receiver,
            },
            McpCleanupCompletionWriter { completion },
        )
    }

    /// Records abnormal lifecycle disposal as negative authority without manufacturing success.
    /// 将异常生命周期销毁记录为负向权威，不制造成功。
    /// Original owners identify disposal; error is the fixed native lifecycle failure.
    /// 原所有者标识销毁；error 为固定原生生命周期失败。
    pub(super) fn failed(
        transport: Option<Weak<dyn McpTransport>>,
        release: Option<vulcan_mcp_runtime::McpResourceRelease>,
        error: String,
    ) -> Self {
        let (observation, writer) = Self::begin(transport, release);
        writer.complete(Err((Vec::new(), error)));
        observation
    }

    /// Waits for the same original result without running protocol shutdown or owning native resources again.
    /// 等待同一原结果，不再次运行协议关闭或拥有原生资源。
    /// No inputs; observer cancellation leaves the sole actual producer and other observers unchanged.
    /// 无输入；观察者取消不影响唯一实际产出者或其他观察者。
    pub(super) async fn wait(&self) -> McpActualCleanupResult {
        let mut receiver = self.completion.clone();
        loop {
            let result = receiver.borrow_and_update().clone();
            if let Some(result) = result {
                return result;
            }
            if receiver.changed().await.is_err() {
                return Err((
                    Vec::new(),
                    String::from("MCP cleanup owner ended without publishing its final result."),
                ));
            }
        }
    }
}

impl McpCleanupCompletionWriter {
    /// Publishes the sole actual protocol/resource result before releasing producer authority.
    /// 释放产出者权威前，发布唯一实际协议及资源结果。
    /// Result comes from the original cleanup operation; no value is returned.
    /// result 来自原清理操作；不返回值。
    pub(super) fn complete(self, result: McpActualCleanupResult) {
        self.completion.send_replace(Some(result));
    }
}

impl Drop for McpCleanupCompletionWriter {
    /// Losing the original pending producer is negative evidence even when physical disposal later completes.
    /// 即使物理销毁之后完成，丢失原待决产出者仍为负向证据。
    /// No inputs or return; a previously published actual final result remains immutable.
    /// 无输入或返回；先前已发布实际最终结果保持不可变。
    fn drop(&mut self) {
        // Release the read borrow before publishing the negative final result.
        // 发布负向最终结果前，释放读取借用。
        let pending = self.completion.borrow().is_none();
        if pending {
            self.completion.send_replace(Some(Err((
                Vec::new(),
                String::from("MCP cleanup owner ended without publishing its final result."),
            ))));
        }
    }
}

impl McpRuntimeHandle {
    /// Evaluates the sole original-owner retirement contract while holding its live admission locks.
    /// 在持有实时准入锁时，计算唯一原所有者退役契约。
    /// No inputs; returns pending, positive actual completion, or the original cleanup error.
    /// 无输入；返回等待、正向实际完成或原清理错误。
    pub(super) fn cleanup_admission(&self) -> McpCleanupAdmission {
        // Admission closes before observation; no shutdown request alone proves physical cleanup.
        // 观察前关闭准入；仅关闭请求不能证明物理清理。
        let business = self
            .inner
            .business_gate
            .read()
            .expect("mcp business gate poisoned");
        if *business != McpBusinessGate::Closed || !self.shutdown_requested() {
            return McpCleanupAdmission::Pending;
        }
        // Keep the same order used by protected launch capture while checking real remaining ownership.
        // 检查真实剩余所有权时，保持受保护启动捕获使用的相同锁顺序。
        let transports = self
            .inner
            .transports
            .read()
            .expect("mcp runtime transport map poisoned");
        let inputs = self
            .inner
            .launch_inputs
            .read()
            .expect("MCP actual launch input map poisoned");
        let releases = self
            .inner
            .resource_releases
            .read()
            .expect("MCP launch release map poisoned");
        let observations = self
            .inner
            .cleanup_observations
            .read()
            .expect("MCP actual cleanup observation map poisoned");
        let snapshot = self
            .inner
            .snapshot
            .read()
            .expect("mcp runtime snapshot poisoned");
        // A protocol error remains a failure even when its resource later disappears independently.
        // 即使其资源之后独立消失，协议错误仍保持失败。
        for server in snapshot.servers.values() {
            if server.status == McpRuntimeServerStatus::Stopped
                && let Some(error) = &server.last_error
            {
                return McpCleanupAdmission::Failed(error.clone());
            }
        }
        for observation in observations.values() {
            if let Some(Err((_, error))) = &*observation.completion.borrow() {
                return McpCleanupAdmission::Failed(error.clone());
            }
        }
        if !transports.is_empty() || !inputs.is_empty() || !releases.is_empty() {
            return McpCleanupAdmission::Pending;
        }
        for (server_id, server) in &snapshot.servers {
            if let Some(observation) = observations.get(server_id) {
                if observation.completion.borrow().is_none() {
                    return McpCleanupAdmission::Pending;
                }
                // A retained caller may still refer to the old transport allocation; the native receipt decides physical release.
                // 保留调用者仍可引用旧传输分配；原生回执决定物理释放。
                if observation
                    .original_release
                    .as_ref()
                    .is_some_and(|release| !release.is_released())
                    || (observation.original_release.is_none()
                        && observation
                            .original_transport
                            .as_ref()
                            .is_some_and(|transport| transport.upgrade().is_some()))
                    || !matches!(
                        server.status,
                        McpRuntimeServerStatus::Stopped | McpRuntimeServerStatus::Failed
                    )
                {
                    return McpCleanupAdmission::Pending;
                }
                continue;
            }
            // Only a declared disabled entry in its unchanged initial state has a legitimate never-launched result.
            // 只有声明停用且保持原初始状态的条目，才具有合法的从未启动结果。
            let never_launched = server.status == McpRuntimeServerStatus::Disabled
                && self
                    .inner
                    .registry
                    .server_by_id(server_id)
                    .is_some_and(|declaration| !declaration.enabled);
            if !never_launched {
                return McpCleanupAdmission::Pending;
            }
        }
        if !self.inner.partition_release.is_released() {
            return McpCleanupAdmission::Pending;
        }
        McpCleanupAdmission::Complete
    }
}

#[cfg(test)]
mod tests;
