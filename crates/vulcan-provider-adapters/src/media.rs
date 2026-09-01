//! Shared media-transfer primitives used by provider upload adapters.
//! 供应商上传适配器共用的媒体传输基础能力。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

/// Reports the number of source bytes transferred and the optional source size.
/// 报告已传输的源字节数与可选的源文件大小。
pub type MediaUploadProgressCallback = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Closed lifecycle phase emitted by a conversation or AI-extension media transfer.
/// 会话或AI扩展媒体传输发出的封闭生命周期阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMediaUploadPhase {
    /// The route and local source have been validated before transfer.
    /// 传输前已完成路由与本地来源校验。
    Preparing,
    /// Source bytes are being uploaded or materialized.
    /// 正在上传或物化源字节。
    Uploading,
    /// The provider reference was prepared successfully.
    /// 供应商引用已成功准备。
    Completed,
    /// Preparation failed with one controlled diagnostic.
    /// 准备过程以一条受控诊断失败。
    Failed,
    /// The owning request or tool execution was cancelled or dropped before completion.
    /// 所属请求或工具执行在完成前被取消或丢弃。
    Cancelled,
}

/// One provider-neutral progress event for media prepared after a user request starts.
/// 用户请求开始后准备媒体时的一条供应商中立进度事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMediaUploadProgressEvent {
    /// Unique transfer identity used to replace one progress bar in place.
    /// 用于原地替换单个进度条的唯一传输标识。
    pub transfer_id: String,
    /// Conversation or task session that owns this upload.
    /// 持有本次上传的会话或任务会话。
    pub owner_session_id: String,
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Exact service-entry identifier.
    /// 精确服务入口标识。
    pub service_entry_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    pub model_id: String,
    /// Exact protocol profile, or the stable provider-tool carrier identifier.
    /// 精确协议 Profile，或稳定的供应商工具载体标识。
    pub protocol_profile_id: String,
    /// Frozen manifest revision.
    /// 冻结的 Manifest 修订号。
    pub manifest_revision: u64,
    /// Frozen runtime manifest generation when available.
    /// 可用时冻结的运行时 Manifest 代际。
    pub manifest_generation: u64,
    /// Content-addressed or target-addressed non-secret route identity.
    /// 内容寻址或目标寻址的非秘密路由身份。
    pub route_revision: String,
    /// Stable operation that owns the upload.
    /// 持有本次上传的稳定操作。
    pub operation_id: String,
    /// Exact upload channel selected by the frozen route.
    /// 冻结路由选择的精确上传通道。
    pub upload_channel_id: String,
    /// User-visible source file name.
    /// 面向用户展示的源文件名。
    pub file_name: String,
    /// Stable image, video, audio or file kind.
    /// 稳定的图片、视频、音频或文件类型。
    pub media_kind: String,
    /// Current transfer lifecycle phase.
    /// 当前传输生命周期阶段。
    pub phase: RuntimeMediaUploadPhase,
    /// Source bytes transferred or materialized so far.
    /// 当前已传输或物化的源字节数。
    pub bytes_sent: u64,
    /// Optional exact source byte length.
    /// 可选的精确源字节长度。
    pub total_bytes: Option<u64>,
    /// Controlled terminal diagnostic for the failed phase.
    /// 失败阶段携带的受控终态诊断。
    pub error: Option<String>,
}

/// Existing host observer shared by conversation and AI-extension uploads.
/// 会话和AI扩展上传共用的既有宿主观察器。
pub type RuntimeMediaUploadProgressObserver =
    Arc<dyn Fn(RuntimeMediaUploadProgressEvent) + Send + Sync>;

/// Coordinates cooperative cancellation for one pre-dispatch media transfer.
/// 协调单次分派前媒体传输的协作式取消。
#[derive(Clone, Default)]
pub struct MediaUploadCancellation {
    /// Atomic state makes cancellation observable before waiting on the notifier.
    /// 原子状态保证在等待通知器前也能观察到取消。
    cancelled: Arc<AtomicBool>,
    /// Notifier wakes a request that is waiting for network headers or a response body.
    /// 通知器唤醒正在等待网络响应头或响应体的请求。
    notify: Arc<Notify>,
}

impl MediaUploadCancellation {
    /// Creates one cancellation handle in the active state.
    /// 创建一个处于活动状态的取消句柄。
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the transfer cancelled and wakes every waiter.
    /// 标记传输已取消并唤醒所有等待者。
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    /// Returns whether cancellation has already been requested.
    /// 返回是否已经请求取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Returns whether `other` is a clone of this exact operation's state and notifier, not merely equally cancelled.
    /// 返回`other`是否克隆自本精确操作的状态及通知器，而非仅具有相同取消状态。
    pub fn same_operation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cancelled, &other.cancelled) && Arc::ptr_eq(&self.notify, &other.notify)
    }

    /// Waits for a cancellation notification.
    /// 等待取消通知。
    pub async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            // Register before reading the cancellation flag because cancel uses notify_waiters.
            // 在读取取消标记前完成注册，因为 cancel 使用 notify_waiters。
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.as_mut().await;
            if self.is_cancelled() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MediaUploadCancellation;
    use std::time::Duration;

    /// Equal cancellation values do not imply shared operation ownership; only a clone shares both signal components.
    /// 相同取消值不表示共享操作所有权；只有克隆共享两个信号组件。
    #[test]
    fn cancellation_identity_is_not_its_boolean_state() {
        // Distinct operations deliberately start with the same uncancelled state.
        // 不同操作有意从相同未取消状态开始。
        let first = MediaUploadCancellation::new();
        let cloned = first.clone();
        let second = MediaUploadCancellation::new();
        assert!(first.same_operation(&cloned));
        assert!(!first.same_operation(&second));
        first.cancel();
        second.cancel();
        assert!(first.same_operation(&cloned));
        assert!(!first.same_operation(&second));
    }

    /// Verifies that a cancellation requested before waiting is still observed immediately.
    /// 验证在等待前发出的取消请求仍会被立即观察到。
    #[tokio::test]
    async fn wait_observes_preexisting_cancellation() {
        let cancellation = MediaUploadCancellation::new();
        cancellation.cancel();
        tokio::time::timeout(Duration::from_millis(100), cancellation.wait())
            .await
            .expect("preexisting cancellation must wake the waiter");
    }
}
