//! Tracks committed vector batches and the server-directed quota wait.
//! 跟踪已提交的向量批次和服务端要求的限流等待。

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::{sync::watch, time::Instant};
use vulcan_ai_contract::{HostAiCallContext, HostAiError, HostAiErrorCode};
use vulcan_service_api::VmmNativeVectorRebuildProgress;

/// Progress for one confirmed operation, shared by its batch workers and status reads.
/// 单次已确认操作的进度，由其批次工作器与状态查询共享。
pub(super) struct VectorRebuildProgress {
    /// Stable command ID used by independent status requests.
    /// 独立状态请求使用的稳定命令标识。
    pub(super) operation_id: String,
    /// Counters and monotonic wait deadline, updated without taking the storage gate.
    /// 不持有存储门即可更新的计数及单调时钟等待截止时间。
    state: watch::Sender<(u64, u64, Option<Instant>)>,
}

impl VectorRebuildProgress {
    /// Creates the zero-progress tracker for the supplied confirmed operation ID.
    /// 为指定的已确认操作标识创建进度为零的跟踪器。
    pub(super) fn new(operation_id: String) -> Self {
        Self {
            operation_id,
            state: watch::Sender::new((0, 0, None)),
        }
    }

    /// Records a committed batch of `count` memories and returns the latest counters to readers.
    /// 记录已提交的 `count` 条记忆批次，使查询方可读取最新计数。
    pub(super) fn record_batch(&self, count: u64) {
        self.state.send_modify(|(memories, batches, _)| {
            *memories += count;
            *batches += 1;
        });
    }

    /// Returns committed counts and the remaining quota wait rounded up to seconds.
    /// 返回已提交计数及向上取整到秒的限流剩余等待时间。
    pub(super) fn snapshot(&self) -> VmmNativeVectorRebuildProgress {
        let (rebuilt_memory_count, batch_count, retry_at) = *self.state.borrow();
        VmmNativeVectorRebuildProgress {
            rebuilt_memory_count,
            batch_count,
            retry_after_seconds: retry_at
                .and_then(|at| {
                    u64::try_from(
                        at.saturating_duration_since(Instant::now())
                            .as_nanos()
                            .div_ceil(1_000_000_000),
                    )
                    .ok()
                })
                .filter(|seconds| *seconds > 0),
        }
    }

    /// Waits for a confirmed quota rejection within the effect deadline.
    /// 在 Effect 截止时间内等待已确认的限流拒绝。
    /// Returns true when the same batch can be retried, false for other failures, or a cancellation error.
    /// 同一批次可重试时返回 true，其他失败返回 false，取消时返回取消错误。
    pub(super) async fn wait_for_quota(
        &self,
        context: &HostAiCallContext,
        error: &HostAiError,
    ) -> Result<bool, HostAiError> {
        if error.code != HostAiErrorCode::RateLimited {
            return Ok(false);
        }
        let Some(hint) = &error.retry_after else {
            return Ok(false);
        };
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok());
        let remaining_ms = context
            .deadline_unix_ms
            .zip(now_ms)
            .map_or(0, |(deadline, now)| deadline.saturating_sub(now));
        if hint.delay_milliseconds == 0 || hint.delay_milliseconds >= remaining_ms {
            return Ok(false);
        }
        let retry_at = Instant::now() + Duration::from_millis(hint.delay_milliseconds);
        self.state.send_modify(|state| state.2 = Some(retry_at));
        let cancelled = tokio::select! {
            biased;
            () = context.cancellation.cancelled() => true,
            () = tokio::time::sleep_until(retry_at) => false,
        };
        self.state.send_modify(|state| state.2 = None);
        if cancelled {
            return Err(HostAiError::new(
                HostAiErrorCode::Cancelled,
                "Cloud memory vector rebuild cancelled.",
            ));
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::{Future, poll_fn};
    use std::task::Poll;
    use vulcan_ai_contract::{RetryAfterHint, RetryAfterSource};

    /// Exposes a quota countdown without advancing committed counts, then allows the same batch to continue.
    /// 限流等待期间展示倒计时且不增加提交计数，等待结束后允许同一批次继续。
    #[tokio::test]
    async fn quota_wait_keeps_committed_progress_and_resumes()
    -> Result<(), Box<dyn std::error::Error>> {
        let progress = VectorRebuildProgress::new(String::from("fixture-rebuild"));
        progress.record_batch(10);
        let context = HostAiCallContext::new(String::from("fixture-batch")).with_deadline(Some(
            u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())? + 60_000,
        ));
        let mut error =
            HostAiError::new(HostAiErrorCode::RateLimited, "Cloud memory quota reached");
        error.retry_after = Some(RetryAfterHint {
            delay_milliseconds: 1000,
            source: RetryAfterSource::HeaderSeconds,
        });
        let wait = progress.wait_for_quota(&context, &error);
        tokio::pin!(wait);
        assert!(poll_fn(|cx| Poll::Ready(wait.as_mut().poll(cx).is_pending())).await);
        let waiting = progress.snapshot();
        assert_eq!(waiting.rebuilt_memory_count, 10);
        assert_eq!(waiting.batch_count, 1);
        assert_eq!(waiting.retry_after_seconds, Some(1));
        assert!(wait.await?);
        assert_eq!(progress.snapshot().retry_after_seconds, None);
        progress.record_batch(4);
        assert_eq!(progress.snapshot().rebuilt_memory_count, 14);
        assert_eq!(progress.snapshot().batch_count, 2);
        Ok(())
    }
}
