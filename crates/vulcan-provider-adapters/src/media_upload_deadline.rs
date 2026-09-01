//! One frozen upload deadline shared by source preparation, cache lookup and physical transfer.
//! 来源准备、缓存查询及物理传输共享的单一冻结上传截止时间。

use crate::{ConversationMediaUploadContract, MediaUploadCancellation, MediaUploadFailure};
use std::future::Future;
use std::time::Duration;

/// Absolute per-upload budget from the selected manifest channel, never a fresh budget for each stage.
/// 来自选中清单通道的单次上传绝对预算，绝不是各阶段的新预算。
#[derive(Debug, Clone)]
pub struct MediaUploadDeadline {
    /// Exact channel used in safe timeout diagnostics.
    /// 安全超时诊断使用的精确通道。
    channel_id: String,
    /// Original declared duration, not remaining time relabeled as a new timeout.
    /// 原声明时长，不把剩余时间改称新超时。
    timeout_ms: u64,
    /// One immutable monotonic end shared by every stage.
    /// 每个阶段共享的单一不可变单调时钟终点。
    expires_at: tokio::time::Instant,
}

impl MediaUploadDeadline {
    /// Starts the absolute budget from exact frozen `contract`; rejects invalid declarations without a default.
    /// 按精确冻结`contract`启动绝对预算；拒绝无效声明，不使用默认值。
    pub fn for_contract(
        contract: &ConversationMediaUploadContract,
    ) -> Result<Self, MediaUploadFailure> {
        Self::for_channel(&contract.upload_channel_id, contract.timeout_ms)
    }

    /// Builds the same budget for exact internal `channel_id` and declared `timeout_ms`, validating manifest bounds.
    /// 为精确内部`channel_id`及声明`timeout_ms`构建相同预算，并校验清单边界。
    pub(crate) fn for_channel(
        channel_id: &str,
        timeout_ms: u64,
    ) -> Result<Self, MediaUploadFailure> {
        if channel_id.trim().is_empty()
            || channel_id.trim() != channel_id
            || channel_id.chars().any(char::is_control)
            || !(1_000..=crate::provider_manifest::MAX_ASYNC_TIMEOUT_MS).contains(&timeout_ms)
        {
            return Err(MediaUploadFailure::InvalidContract);
        }
        Ok(Self {
            channel_id: channel_id.to_string(),
            timeout_ms,
            expires_at: tokio::time::Instant::now() + Duration::from_millis(timeout_ms),
        })
    }

    /// Verifies that `contract` still names this exact channel and original duration; no replacement or reset is permitted.
    /// 校验`contract`仍指向本精确通道及原时长；不允许替换或重置。
    pub fn validate_contract(
        &self,
        contract: &ConversationMediaUploadContract,
    ) -> Result<(), MediaUploadFailure> {
        if self.channel_id != contract.upload_channel_id || self.timeout_ms != contract.timeout_ms {
            return Err(MediaUploadFailure::InvalidContract);
        }
        Ok(())
    }

    /// Checks parent `cancellation` before the original deadline; returns a typed failure without cancelling later files.
    /// 先检查父`cancellation`再检查原截止时间；返回强类型失败，不取消后续文件。
    pub fn check(&self, cancellation: &MediaUploadCancellation) -> Result<(), MediaUploadFailure> {
        if cancellation.is_cancelled() {
            Err(MediaUploadFailure::Cancelled)
        } else if tokio::time::Instant::now() >= self.expires_at {
            Err(self.timeout_failure())
        } else {
            Ok(())
        }
    }

    /// Awaits stage `future` under the same end time and parent `cancellation`, preserving its output or returning typed cancellation/timeout.
    /// 在相同终点及父`cancellation`约束下等待阶段`future`，保留其输出或返回强类型取消/超时。
    /// Expiry drops only this stage; owned atomic completion workers must be settled by their existing owner outside this wait.
    /// 到期仅丢弃本阶段；自有原子完成工作线程必须由既有所有者在本等待之外结算。
    pub fn wait<'a, T: 'a>(
        &'a self,
        cancellation: &'a MediaUploadCancellation,
        future: impl Future<Output = T> + 'a,
    ) -> impl Future<Output = Result<T, MediaUploadFailure>> + 'a {
        // Box the stage before constructing the outer future; large source-read frames must not multiply across deadline wrappers.
        // 构造外围Future之前将阶段装箱；大型来源读取帧不能跨截止时间包装层倍增。
        let stage = Box::pin(future);
        async move {
            self.check(cancellation)?;
            // Parent cancellation wins when it becomes ready together with expiry or a successful stage.
            // 父取消与到期或成功阶段同时就绪时，父取消优先。
            let result = tokio::select! {
                biased;
                _ = cancellation.wait() => Err(MediaUploadFailure::Cancelled),
                _ = tokio::time::sleep_until(self.expires_at) => Err(self.timeout_failure()),
                result = stage => Ok(result),
            };
            // Non-yielding final work cannot report success after exhausting the original budget.
            // 不让出的最终工作不能在耗尽原预算后报告成功。
            self.check(cancellation)?;
            result
        }
    }

    /// Returns the original safe timeout identity, without exposing local paths or provider references.
    /// 返回原安全超时身份，不暴露本地路径或供应商引用。
    fn timeout_failure(&self) -> MediaUploadFailure {
        MediaUploadFailure::Timeout {
            channel_id: self.channel_id.clone(),
            timeout_ms: self.timeout_ms,
        }
    }
}

#[cfg(test)]
mod tests;
