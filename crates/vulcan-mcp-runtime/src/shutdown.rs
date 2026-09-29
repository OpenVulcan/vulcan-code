//! Cancellation-safe ownership of one transport's asynchronous close operation.
//! 单个传输异步关闭操作的取消安全所有权。

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::service::{RoleClient, RunningService};
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::McpRuntimeError;
use crate::rmcp_support::EventingClientHandler;

mod backend;
use backend::BackendCloseReceipt;
pub(crate) use backend::observe_backend_close;

/// Observation budget shared by all built-in transport close operations.
/// 所有内建传输关闭操作共用的观测预算。
const TRANSPORT_SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

/// The exact close result, retained for concurrent and subsequent observers.
/// 为并发及后续观察者保留的精确关闭结果。
type ShutdownResult = Result<(), McpRuntimeError>;

/// A not-yet-started close operation and its only result publisher.
/// 尚未启动的关闭操作及其唯一结果发布者。
struct PendingShutdown {
    /// The future owns the service or listener until actual close completion.
    /// 此任务持有服务或监听器，直至实际关闭结束。
    future: Pin<Box<dyn Future<Output = ShutdownResult> + Send>>,
    /// Only the worker retains this sender after close is requested.
    /// 请求关闭后仅工作任务保留此发送端。
    result_tx: watch::Sender<Option<ShutdownResult>>,
}

/// Starts close once and lets callers observe it without owning or cancelling the work.
/// 仅启动一次关闭，让调用方观测工作而不拥有或取消该工作。
///
/// A successful result proves completion of the supplied close future only.
/// 成功结果仅证明传入的关闭任务已经完成。
/// Process-tree exit and detached backend work need their own lifetime evidence.
/// 进程树退出和后端派生任务仍须具有各自的生命周期证据。
pub(crate) struct TransportShutdown {
    /// Stable diagnostic prefix; it never contains an endpoint or a credential.
    /// 稳定诊断前缀，绝不包含端点或凭据。
    label: &'static str,
    /// Runtime captured at construction, including for destruction outside a runtime context.
    /// 构造时捕获的执行器，也用于不在执行器上下文中的析构。
    runtime: Handle,
    /// Synchronous handoff prevents cancellation between taking work and spawning it.
    /// 同步移交防止取走工作与启动工作之间发生取消。
    pending: Mutex<Option<PendingShutdown>>,
    /// Every observer reads the same immutable terminal result.
    /// 每个观察者读取同一个不可变终态结果。
    result_rx: watch::Receiver<Option<ShutdownResult>>,
}

impl TransportShutdown {
    /// Retains one close future without starting it.
    /// 保留一个关闭任务，但不立即启动。
    ///
    /// Inputs are a static diagnostic label and an owned cleanup future.
    /// 输入为静态诊断标签和自有清理任务。
    /// Returns an owner tied to the current Tokio runtime; construction requires that runtime.
    /// 返回绑定当前 Tokio 执行器的所有者；构造时必须处于该执行器中。
    pub(crate) fn new(
        label: &'static str,
        future: impl Future<Output = ShutdownResult> + Send + 'static,
    ) -> Self {
        // The owner retains only a receiver after the single worker starts.
        // 唯一工作任务启动后，所有者仅保留接收端。
        let (result_tx, result_rx) = watch::channel(None);
        Self {
            label,
            runtime: Handle::current(),
            pending: Mutex::new(Some(PendingShutdown {
                future: Box::pin(future),
                result_tx,
            })),
            result_rx,
        }
    }

    /// Retains an rmcp service and closes it once when requested or dropped.
    /// 保留 rmcp 服务，在请求或析构时仅关闭一次。
    ///
    /// Inputs identify the transport, connected service, and exact backend receipt; returns their close owner.
    /// 输入标识传输、已连接服务及精确后端回执；返回它们的关闭所有者。
    pub(crate) fn for_service(
        label: &'static str,
        mut service: RunningService<RoleClient, Arc<EventingClientHandler>>,
        backend_close: BackendCloseReceipt,
    ) -> Self {
        Self::new(label, async move {
            // Joining the service alone does not report errors returned by its transport close.
            // 仅等待服务任务不会报告传输关闭返回的错误。
            let service_result = service
                .close()
                .await
                .map(|_| ())
                .map_err(|error| McpRuntimeError::transport(format!("{label}.join: {error}")));
            let backend_result = backend_close.wait().await;
            match (service_result, backend_result) {
                (Ok(()), result) | (result, Ok(())) => result,
                (Err(service_error), Err(backend_error)) => Err(McpRuntimeError::transport(
                    format!("{service_error}; {backend_error}"),
                )),
            }
        })
    }

    /// Requests the original close operation once without waiting.
    /// 仅请求一次原关闭操作，不等待完成。
    ///
    /// No arguments or return value; a detached worker owns all supplied cleanup resources.
    /// 无参数与返回值；独立工作任务拥有传入的全部清理资源。
    fn request(&self) {
        // No user code runs under this lock, and there is no await during ownership transfer.
        // 此锁内不执行用户代码，所有权移交期间也不存在等待点。
        let pending = self
            .pending
            .lock()
            .expect("MCP transport shutdown handoff poisoned")
            .take();
        if let Some(pending) = pending {
            self.runtime.spawn(async move {
                // Dropping this sender without publishing makes interruption observable.
                // 若未发布就丢弃发送端，观察者会明确得知执行中断。
                let result = pending.future.await;
                pending.result_tx.send_replace(Some(result));
            });
        }
    }

    /// Waits for the shared close result within the standard observation budget.
    /// 在标准观测预算内等待共享关闭结果。
    ///
    /// Returns the original result, timeout, or worker interruption; never cancels cleanup.
    /// 返回原始结果、超时或工作任务中断；绝不取消清理。
    pub(crate) async fn wait(&self) -> ShutdownResult {
        self.wait_for(TRANSPORT_SHUTDOWN_WAIT).await
    }

    /// Requests the original close and waits for its final result without an observation deadline.
    /// 请求原关闭并等待最终结果，不附加观测截止时间。
    ///
    /// No inputs; returns the shared terminal result or an explicit worker interruption.
    /// 无输入；返回共享终态结果或明确的工作任务中断。
    /// Runtime owners use this path while their callers keep independent finite observation budgets.
    /// 运行时所有者使用此路径，调用方保持独立的有限观测预算。
    pub(crate) async fn wait_until_complete(&self) -> ShutdownResult {
        self.request();
        // A private cursor prevents observers from consuming each other's updates.
        // 独立游标防止观察者消耗彼此的更新。
        let mut result_rx = self.result_rx.clone();
        loop {
            if let Some(result) = result_rx.borrow_and_update().clone() {
                return result;
            }
            result_rx.changed().await.map_err(|_| {
                McpRuntimeError::transport(format!(
                    "{} interrupted before close completion",
                    self.label
                ))
            })?;
        }
    }

    /// Observes the same operation for a caller-selected duration used by bounded tests.
    /// 在有界测试指定的时长内观测同一操作。
    ///
    /// The duration limits this observer only; returns the shared result or observation failure.
    /// 时长仅限制当前观察者；返回共享结果或观测失败。
    async fn wait_for(&self, budget: Duration) -> ShutdownResult {
        tokio::time::timeout(budget, self.wait_until_complete())
            .await
            .map_err(|_| {
                McpRuntimeError::transport(format!(
                    "{} observation timed out after {} ms; cleanup remains pending",
                    self.label,
                    budget.as_millis()
                ))
            })?
    }
}

impl Drop for TransportShutdown {
    /// Starts pending cleanup while preserving the worker's resource ownership.
    /// 启动待处理清理，同时保留工作任务的资源所有权。
    ///
    /// Destruction is nonblocking and returns no value; it does not prove cleanup completion.
    /// 析构不阻塞且无返回值；析构本身不证明清理完成。
    fn drop(&mut self) {
        self.request();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod backend_tests;
