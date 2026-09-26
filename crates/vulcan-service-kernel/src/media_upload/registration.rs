//! Unique cancellation-registration ownership through preparation and known-result publication.
//! 跨准备及已知结果发布的唯一取消登记所有权。

use super::{MediaUploadCancellation, pending_media_uploads};

/// Shared read-side state for an active operation: cancellation request and actual settlement are independent facts.
/// 活动操作的共享读取状态：取消请求及实际结算是独立事实。
#[derive(Clone)]
pub(super) struct PendingMediaUpload {
    /// Original draft reservation when this entry is a physical upload or one of its view aliases.
    /// 当此条目是实体上传或其视图别名时，持有原草稿许可。
    pub(super) draft: Option<std::sync::Arc<super::draft_execution::DraftUploadReservation>>,
    /// Exact cancellation token for the active source/network owner.
    /// 活动来源/网络所有者的精确取消令牌。
    pub(super) cancellation: MediaUploadCancellation,
    /// Becomes true only after the unique owner has finished and removed its registration.
    /// 仅在唯一所有者完成并移除登记后变为真。
    pub(super) settled: tokio::sync::watch::Receiver<bool>,
}

/// Owns one exact pending registry entry; dropping it cancels only its token and never removes a replacement entry.
/// 持有一个精确待处理注册条目；丢弃时仅取消自身令牌，绝不移除替换条目。
pub(crate) struct PendingMediaUploadRegistration {
    /// Exact opaque transfer id used by the pending cancellation registry.
    /// 待处理取消注册表使用的精确不透明传输ID。
    transfer_id: String,
    /// Shared token observed by this operation's source and network work.
    /// 本操作来源及网络工作观察的共享令牌。
    pub(super) cancellation: MediaUploadCancellation,
    /// Sole completion publisher, retained through any already-known result worker.
    /// 唯一完成发布者，在任何已知结果工作线程期间保持持有。
    settled: tokio::sync::watch::Sender<bool>,
    /// Physical source lifetime retained through network use and any known-result publisher.
    /// 在网络使用及任何已知结果发布期间保留的实体来源寿命。
    source_hold: Option<vulcan_agent_runtime::attachment_asset_index::AttachmentSourceHold>,
}

impl PendingMediaUploadRegistration {
    /// Takes ownership of the already-registered `transfer_id` and exact `cancellation`; the caller must have atomically inserted that token.
    /// 接管已经登记的`transfer_id`及精确`cancellation`；调用方必须已原子插入该令牌。
    /// `settled` is the sole terminal acknowledgement sender paired with the registered read-side state.
    /// `settled`是与已登记读取状态配对的唯一终态确认发送端。
    pub(super) fn new(
        transfer_id: String,
        cancellation: MediaUploadCancellation,
        settled: tokio::sync::watch::Sender<bool>,
    ) -> Self {
        Self {
            transfer_id,
            cancellation,
            settled,
            source_hold: None,
        }
    }

    /// Atomically validates the exact draft origin and retains its source before physical work starts.
    /// 在实体工作开始前原子校验精确草稿来源并保留其文件。
    /// Parameters name the canonical attachment, original draft scope and immutable intent; returns that original proof.
    /// 参数指定规范附件、原草稿作用域及不可变意图；返回原证明。
    pub(super) fn retain_draft_source(
        &mut self,
        attachment: &vulcan_core::types::ConversationAttachment,
        draft_scope: &str,
        intent_id: &str,
    ) -> Result<vulcan_agent_runtime::attachment_asset_index::AttachmentDraftMediaOrigin, String>
    {
        if self.source_hold.is_some() {
            return Err(String::from(
                "This physical upload already retains its source.",
            ));
        }
        let (original, source_hold) =
            vulcan_agent_runtime::attachment_asset_index::retain_draft_attachment_source(
                attachment,
                draft_scope,
                intent_id,
            )?;
        self.source_hold = Some(source_hold);
        Ok(original)
    }

    /// Runs already-known `publish` once in a blocking worker that retains this registration until publication settles.
    /// 在阻塞工作线程中仅一次执行已知结果`publish`，并保留本登记直到发布结算。
    /// Returns the worker handle; dropping its awaiting RPC does not discard the validated result or release ownership early.
    /// 返回工作线程句柄；丢弃等待它的RPC不会丢弃已验证结果或提前释放所有权。
    pub(super) fn spawn_known_completion<T: Send + 'static>(
        self,
        publish: impl FnOnce() -> T + Send + 'static,
    ) -> tokio::task::JoinHandle<T> {
        tokio::task::spawn_blocking(move || {
            // Keep the exact guard through the entire publisher, including all of its early error returns.
            // 在整个发布器期间保留精确守卫，包含发布器的全部提前错误返回。
            let result = publish();
            drop(self);
            result
        })
    }
}

impl Drop for PendingMediaUploadRegistration {
    /// Revokes this operation and removes only its still-current registry entry on success, error, panic or future drop.
    /// 在成功、错误、异常或Future丢弃时撤销本操作，并仅移除仍为当前的自身注册条目。
    fn drop(&mut self) {
        // Source use and cleanup settle before cancellation acknowledgement; never hold the pending lock while taking an asset lock.
        // 来源使用及清理先于取消确认结算；取得资产锁时绝不持有待处理锁。
        drop(self.source_hold.take());
        // Compare actual token ownership under the one pending lock before removing an entry with the same textual id.
        // 删除具有相同文本ID的条目之前，在唯一待处理锁内比较实际令牌所有权。
        let Ok(mut pending) = pending_media_uploads().lock() else {
            self.cancellation.cancel();
            tracing::warn!(
                "[webui-service] media upload registration `{}` cleanup could not acquire its owner",
                self.transfer_id
            );
            return;
        };
        // Aliases share this exact token, while replacements and unrelated operations do not.
        // 别名共享此精确令牌，替换条目和其他操作则不共享。
        pending.retain(|_, current| !current.cancellation.same_operation(&self.cancellation));
        drop(pending);
        // On terminal drop, publish cleanup before notifying cancellation observers; an explicit earlier cancel still remains non-terminal.
        // 终态丢弃时先发布清理再通知取消观察者；更早的显式取消仍保持非终态。
        self.cancellation.cancel();
        // No receiver means nobody awaits this terminal acknowledgement; cleanup is already committed in either case.
        // 没有接收者表示无人等待本终态确认；无论哪种情况，清理已经提交。
        let _ = self.settled.send(true);
    }
}

#[cfg(test)]
mod tests;
