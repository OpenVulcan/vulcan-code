//! Retains the underlying rmcp transport close result independently of service join success.
//! 独立于服务任务成功，保留 rmcp 底层传输的关闭结果。

use std::borrow::Cow;

use rmcp::service::{RoleClient, RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;
use tokio::sync::watch;

use super::ShutdownResult;
use crate::McpRuntimeError;

/// Delegates the wire protocol unchanged and records the first completed backend close.
/// 原样委托线路协议，并记录首次完成的后端关闭。
pub(crate) struct ObservedBackend<T> {
    /// Exact backend passed to the official client service.
    /// 传给官方客户端服务的精确后端。
    transport: T,
    /// Context used only when projecting the original close error.
    /// 仅在投影原关闭错误时使用的上下文。
    label: &'static str,
    /// Sole completion writer; its loss without a result means interrupted cleanup.
    /// 唯一完成写入者；无结果即消失表示清理中断。
    result_tx: watch::Sender<Option<ShutdownResult>>,
}

/// Observes a backend result without owning, restarting, or declaring completion of its cleanup.
/// 观测后端结果，不持有、重启或自行宣告其清理完成。
pub(crate) struct BackendCloseReceipt {
    /// Exact backend context retained when the writer disappears before completion.
    /// 写入者完成前消失时仍保留的精确后端上下文。
    label: &'static str,
    /// Shared final result with independent cursors for repeated observers.
    /// 共享最终结果，重复观察者各有独立游标。
    result_rx: watch::Receiver<Option<ShutdownResult>>,
}

/// Wraps one backend before initialization and returns its independent close receipt.
/// 在初始化前包装单个后端，并返回其独立关闭回执。
/// The backend owns the transport; the static label identifies failures. Neither result starts cleanup.
/// 后端持有传输，静态标签标识失败；两个返回值均不会启动清理。
pub(crate) fn observe_backend_close<T: Transport<RoleClient>>(
    transport: T,
    label: &'static str,
) -> (ObservedBackend<T>, BackendCloseReceipt) {
    // Completion remains absent until the original backend method actually returns.
    // 原后端方法实际返回前，完成结果始终缺失。
    let (result_tx, result_rx) = watch::channel(None);
    (
        ObservedBackend {
            transport,
            label,
            result_tx,
        },
        BackendCloseReceipt { label, result_rx },
    )
}

impl BackendCloseReceipt {
    /// Waits for the first completed close or an explicit missing-completion error.
    /// 等待首次完成的关闭，或明确的缺失完成结果错误。
    /// No inputs; returns a retained result without changing the backend lifecycle.
    /// 无输入；返回保留结果，不改变后端生命周期。
    pub(super) async fn wait(&self) -> ShutdownResult {
        // Each observation has its own cursor and cannot consume another caller's update.
        // 每次观察有独立游标，不会消耗其他调用方的更新。
        let mut result_rx = self.result_rx.clone();
        loop {
            if let Some(result) = result_rx.borrow_and_update().clone() {
                return result;
            }
            result_rx.changed().await.map_err(|_| {
                McpRuntimeError::transport(format!(
                    "{} backend dropped before close completion",
                    self.label
                ))
            })?;
        }
    }
}

impl<T: Transport<RoleClient>> Transport<RoleClient> for ObservedBackend<T> {
    /// Preserves concrete transport errors and their authorization source chain.
    /// 保留具体传输错误及其鉴权来源链。
    type Error = T::Error;

    /// Preserves the backend diagnostic name without adding a wrapper identity to user errors.
    /// 保留后端诊断名称，不在用户错误中引入包装层身份。
    /// No inputs; returns the underlying transport name.
    /// 无输入；返回底层传输名称。
    fn name() -> Cow<'static, str> {
        T::name()
    }

    /// Forwards one original protocol message with the backend's unchanged error type.
    /// 转发一条原协议消息，保持后端错误类型不变。
    /// The message is moved to the backend; returns its independent send future.
    /// 消息移交后端；返回其独立发送任务。
    fn send(
        &mut self,
        message: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.transport.send(message)
    }

    /// Forwards sequential receive without consuming or rewriting protocol content.
    /// 转发顺序接收，不提前消费或改写协议内容。
    /// No inputs; returns the next original message or backend closure.
    /// 无输入；返回下一条原消息或后端关闭。
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        self.transport.receive().await
    }

    /// Records the original completed close before returning the same result to rmcp.
    /// 返回相同结果给 rmcp 前，记录原关闭的完成结果。
    /// No inputs; a cancelled close publishes no fabricated completion.
    /// 无输入；关闭取消时不发布伪造的完成结果。
    async fn close(&mut self) -> Result<(), Self::Error> {
        // The official service logs close errors but does not include them in its join result.
        // 官方服务仅记录关闭错误，不将其包含在任务等待结果中。
        let result = self.transport.close().await;
        self.result_tx.send_if_modified(|observed| {
            if observed.is_some() {
                return false;
            }
            *observed = Some(result.as_ref().map(|_| ()).map_err(|error| {
                McpRuntimeError::transport(format!("{} backend close: {error}", self.label))
            }));
            true
        });
        result
    }
}
