//! Exercises backend close failures through the real rmcp client service.
//! 通过真实 rmcp 客户端服务验证后端关闭失败。

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rmcp::service::{RoleClient, RxJsonRpcMessage, ServiceExt, TxJsonRpcMessage};
use rmcp::transport::Transport;
use serde_json::json;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::{TransportShutdown, observe_backend_close};
use crate::rmcp_support::EventingClientHandler;

/// An in-memory backend that performs real initialization and controls only its close result.
/// 执行真实初始化、仅控制关闭结果的内存后端。
struct CloseResultBackend {
    /// Receives protocol replies consumed by the official client loop.
    /// 接收官方客户端循环消费的协议回复。
    replies: mpsc::Receiver<RxJsonRpcMessage<RoleClient>>,
    /// Sends initialization replies without replacing the official service.
    /// 发送初始化回复，不替换官方服务。
    reply_tx: mpsc::Sender<RxJsonRpcMessage<RoleClient>>,
    /// Selects the backend terminal close outcome.
    /// 选择后端最终关闭结果。
    fail_close: bool,
    /// Counts actual backend close calls independently of all observers.
    /// 独立于全部观察者，统计实际后端关闭次数。
    close_calls: Arc<AtomicUsize>,
    /// Optional test gate leaves the actual close pending until explicitly released.
    /// 可选测试门控使实际关闭保持等待，直到显式放行。
    close_gate: Option<oneshot::Receiver<()>>,
}

impl CloseResultBackend {
    /// Constructs a protocol backend with the specified close outcome and shared call counter.
    /// 根据指定关闭结果和共享调用计数器构建协议后端。
    /// Returns an unconnected transport; neither argument initiates a service.
    /// 返回尚未连接的传输；两个参数均不会启动服务。
    fn new(fail_close: bool, close_calls: Arc<AtomicUsize>) -> Self {
        // One bounded queue supplies the single initialization response.
        // 单个有界队列提供唯一的初始化响应。
        let (reply_tx, replies) = mpsc::channel(1);
        Self {
            replies,
            reply_tx,
            fail_close,
            close_calls,
            close_gate: None,
        }
    }
}

impl Transport<RoleClient> for CloseResultBackend {
    /// Keeps backend errors in their original concrete type.
    /// 保持后端错误的原具体类型。
    type Error = io::Error;

    /// Accepts client messages and echoes the declared protocol version during initialization.
    /// 接收客户端消息，在初始化时回传其声明的协议版本。
    /// The input is the official client message; returns only queue or serialization failures.
    /// 输入为官方客户端消息；仅返回队列或序列化失败。
    fn send(
        &mut self,
        message: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        // Own both values so the service may send concurrently with receiving.
        // 同时持有两个值，使服务能够并发发送与接收。
        let reply_tx = self.reply_tx.clone();
        let request = serde_json::to_value(message);
        async move {
            let request = request.map_err(io::Error::other)?;
            if request["method"] == "initialize" {
                // The real client's request supplies the exact ID and supported version.
                // 真实客户端请求提供精确 ID 及支持的版本。
                let response = serde_json::from_value(json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": {
                        "protocolVersion": request["params"]["protocolVersion"],
                        "capabilities": {},
                        "serverInfo": { "name": "close-result-fixture", "version": "1" }
                    }
                }))
                .map_err(io::Error::other)?;
                reply_tx.send(response).await.map_err(io::Error::other)?;
            }
            Ok(())
        }
    }

    /// Returns the next protocol reply, or closure after the fixture sender disappears.
    /// 返回下一条协议回复，或在测试发送者消失后返回关闭。
    /// No inputs; preserves the official sequential receive contract.
    /// 无输入；保持官方顺序接收契约。
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        self.replies.recv().await
    }

    /// Reports the configured close result after recording one actual backend invocation.
    /// 记录一次实际后端调用后报告配置的关闭结果。
    /// No inputs; returns the exact injected I/O error or terminal success.
    /// 无输入；返回精确注入的输入输出错误或最终成功。
    async fn close(&mut self) -> Result<(), Self::Error> {
        self.close_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = self.close_gate.take() {
            gate.await.map_err(io::Error::other)?;
        }
        if self.fail_close {
            Err(io::Error::other("fixture backend close failed"))
        } else {
            Ok(())
        }
    }
}

/// Dropping a backend without calling close cannot manufacture a successful cleanup receipt.
/// 未调用关闭即丢弃后端，不能伪造成功清理回执。
/// No inputs or return value; checks repeated observations and zero actual close calls.
/// 无输入与返回值；检查重复观察及实际关闭调用为零。
#[tokio::test]
async fn backend_drop_without_close_is_explicit() {
    // The receipt is intentionally kept after its exact writer is destroyed.
    // 精确写入者销毁后，刻意继续保留回执。
    let close_calls = Arc::new(AtomicUsize::new(0));
    let backend = CloseResultBackend::new(false, Arc::clone(&close_calls));
    let (backend, receipt) = observe_backend_close(backend, "fixture.drop");
    drop(backend);
    for _ in 0..2 {
        let result = tokio::time::timeout(Duration::from_secs(5), receipt.wait())
            .await
            .expect("missing completion must resolve explicitly");
        assert!(
            result
                .expect_err("no close occurred")
                .to_string()
                .contains("backend dropped before close completion")
        );
    }
    assert_eq!(close_calls.load(Ordering::SeqCst), 0);
}

/// A polled but cancelled close stays incomplete when its backend disappears.
/// 已轮询但取消的关闭，在后端消失时仍属于未完成。
/// No inputs or return value; verifies cancellation does not publish an early success.
/// 无输入与返回值；验证取消不会提前发布成功。
#[tokio::test]
async fn cancelled_backend_close_has_no_success_receipt() {
    // Retain the sender so cancellation is the only reason the pending close disappears.
    // 保留发送端，使取消成为待完成关闭消失的唯一原因。
    let (_release, gate) = oneshot::channel();
    let close_calls = Arc::new(AtomicUsize::new(0));
    let mut backend = CloseResultBackend::new(false, Arc::clone(&close_calls));
    backend.close_gate = Some(gate);
    let (mut backend, receipt) = observe_backend_close(backend, "fixture.cancel");
    {
        // Poll once to prove the original method entered before cancellation.
        // 轮询一次，证明取消前已进入原方法。
        let close = backend.close();
        tokio::pin!(close);
        assert!(futures::poll!(close.as_mut()).is_pending());
        assert_eq!(close_calls.load(Ordering::SeqCst), 1);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(10), receipt.wait())
            .await
            .is_err()
    );
    drop(backend);
    assert!(
        tokio::time::timeout(Duration::from_secs(5), receipt.wait())
            .await
            .expect("dropped backend receipt")
            .expect_err("cancelled close has no completion")
            .to_string()
            .contains("backend dropped before close completion")
    );
}

/// A joined official service must not turn its backend close error into host cleanup success.
/// 官方服务任务结束不能将后端关闭错误转换为宿主清理成功。
/// No inputs or return value; verifies the actual service boundary and repeated observation.
/// 无输入与返回值；验证实际服务边界及重复观测。
#[tokio::test]
async fn real_service_preserves_backend_close_result() {
    for fail_close in [false, true] {
        // Keep the counter outside the transport to observe its destruction safely.
        // 将计数器留在传输外部，以便安全观测其析构。
        let close_calls = Arc::new(AtomicUsize::new(0));
        let backend = CloseResultBackend::new(fail_close, Arc::clone(&close_calls));
        let (backend, backend_close) = observe_backend_close(backend, "fixture.close");
        let (events, _) = broadcast::channel(1);
        let handler = Arc::new(EventingClientHandler::new(events));
        let service = tokio::time::timeout(Duration::from_secs(5), handler.serve(backend))
            .await
            .expect("real initialize deadline")
            .expect("real initialize");
        let shutdown = TransportShutdown::for_service("fixture.close", service, backend_close);
        for _ in 0..2 {
            let result =
                tokio::time::timeout(Duration::from_secs(5), shutdown.wait_until_complete())
                    .await
                    .expect("final cleanup deadline");
            if fail_close {
                assert!(
                    result
                        .expect_err("backend failure must reach host")
                        .to_string()
                        .contains("fixture backend close failed")
                );
            } else {
                result.expect("successful backend close");
            }
        }
        assert_eq!(close_calls.load(Ordering::SeqCst), 1);
    }
}
