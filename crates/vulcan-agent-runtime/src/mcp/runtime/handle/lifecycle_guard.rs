//! Final task disposal revokes transport admission even when its executor disappears.
//! 即使执行器消失，最终任务销毁仍撤销传输入场。

use super::*;

/// Owns lifecycle admission cleanup independently of whether an async task ever gets polled.
/// 独立于异步任务是否曾被轮询，拥有生命周期入场清理。
struct LifecycleAdmissionGuard {
    /// Original registry and diagnostic state, shared with non-owning observers.
    /// 原注册表及诊断状态，与非拥有观察者共享。
    inner: Arc<McpRuntimeInner>,
    /// Exact server identity has one lifecycle writer per runtime.
    /// 精确服务器身份在每个运行时中仅有一个生命周期写入者。
    server_id: String,
    /// Final task disposal retains aggregate authority until admission and diagnostics are revoked.
    /// 最终任务销毁保留聚合权威，直至入场及诊断被撤销。
    _resources: vulcan_mcp_runtime::McpTransportResources,
}

impl Drop for LifecycleAdmissionGuard {
    /// Releases the registry's actual transport owner after task exit, cancellation, or executor loss.
    /// 在任务退出、取消或执行器丢失后释放注册表的实际传输所有者。
    /// No inputs or return; abnormal exit records failure and never fabricates successful shutdown.
    /// 无输入与返回值；异常退出记录失败，绝不伪造成功关闭。
    fn drop(&mut self) {
        // Dispose outside the map lock; native/resource owners remain responsible for actual cleanup.
        // 在映射锁外销毁；原生及资源所有者继续负责实际清理。
        let transport = self
            .inner
            .transports
            .write()
            .expect("mcp runtime transport map poisoned")
            .remove(&self.server_id);
        // The actual allocation remains the original negative observation identity even after disposal.
        // 销毁后，实际分配仍为原负向观察身份。
        let original_transport = transport.as_ref().map(Arc::downgrade);
        drop(transport);
        // Abnormal task disposal must also revoke the exact successful launch-input capture record.
        // 异常任务销毁还必须撤销精确成功启动输入捕获记录。
        self.inner
            .launch_inputs
            .write()
            .expect("MCP actual launch input map poisoned")
            .remove(&self.server_id);
        // A previously published terminal result must survive ordinary task disposal unchanged.
        // 已发布终止结果必须在普通任务销毁期间保持不变。
        let terminal = self
            .inner
            .snapshot
            .read()
            .expect("mcp runtime snapshot poisoned")
            .servers
            .get(&self.server_id)
            .is_some_and(|server| {
                matches!(
                    server.status,
                    McpRuntimeServerStatus::Stopped | McpRuntimeServerStatus::Failed
                )
            });
        if !terminal {
            // Executor loss cannot inherit an earlier operation's success merely because physical disposal finishes.
            // 执行器丢失不能仅因物理销毁完成，就继承前一操作的成功。
            let original_release = self
                .inner
                .resource_releases
                .read()
                .expect("MCP launch release map poisoned")
                .get(&self.server_id)
                .cloned();
            self.inner
                .cleanup_observations
                .write()
                .expect("MCP actual cleanup observation map poisoned")
                .insert(
                    self.server_id.clone(),
                    McpActualCleanupObservation::failed(
                        original_transport,
                        original_release,
                        String::from("MCP lifecycle task ended without confirming shutdown."),
                    ),
                );
            update_server_status(
                &self.inner,
                &self.server_id,
                McpRuntimeServerStatus::Failed,
                Some(String::from(
                    "MCP lifecycle task ended without confirming shutdown.",
                )),
                Vec::new(),
            );
            update_server_prompt_count(&self.inner, &self.server_id, 0);
            update_server_resource_count(&self.inner, &self.server_id, 0);
        }
    }
}

/// Creates disposal protection before spawn so cancellation of an unpolled task is covered.
/// 在启动前创建销毁保护，覆盖未轮询任务的取消。
/// Inner and server select the sole admission; resources keeps its original aggregate scope alive.
/// inner 与 server 选择唯一入场；resources 保留其原聚合范围。
/// Returns an opaque owner whose destructor revokes the exact registration without async execution.
/// 返回不透明所有者，其析构无需异步执行即可撤销精确注册。
pub(super) fn retain_lifecycle_admission(
    inner: Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    resources: vulcan_mcp_runtime::McpTransportResources,
) -> impl Send + 'static {
    LifecycleAdmissionGuard {
        inner,
        server_id: server.id.clone(),
        _resources: resources,
    }
}
