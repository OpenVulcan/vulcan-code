//! Host-neutral refresh observation and cooperative cancellation across source and commit phases.
//! 跨源码与提交阶段的宿主无关刷新观察和协作取消。

use crate::VceError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// A real processing phase; counters describe this phase only, never a simulated overall percent.
/// 真实处理阶段；计数仅描述此阶段，绝不模拟整体百分比。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VceRefreshPhase {
    /// Filesystem observations are being collected.
    /// 正在收集文件系统观察。
    Scanning,
    /// Observations are being reconciled with the previous workspace.
    /// 正在将观察与先前工作区进行差异协调。
    Reconciling,
    /// One bounded source batch is being analyzed and split.
    /// 正在分析和切分一个有界源码批次。
    Chunking,
    /// Validated embedding responses are completing for the identified operation.
    /// 标识操作的已校验向量响应正在完成。
    Embedding,
    /// Staged blobs are being acknowledged by host storage.
    /// 宿主存储正在确认暂存块对象。
    Writing,
    /// Publication has entered its non-cancellable transaction boundary.
    /// 发布已进入不可取消的事务边界。
    Committing,
    /// The refresh completed successfully, including a successful no-change synchronization.
    /// 刷新成功完成，包括成功的无变化同步。
    Complete,
}

/// Explicit counting unit for truthful phase-level progress.
/// 用于真实阶段进度的明确计数单位。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VceRefreshUnit {
    /// Source files actually observed.
    /// 实际已观察的源文件。
    Files,
    /// Logical blobs actually prepared or staged.
    /// 实际已准备或暂存的逻辑块对象。
    Blobs,
    /// Embedding batches whose responses passed validation.
    /// 响应已通过校验的向量批次。
    Batches,
    /// No numeric denominator is currently meaningful.
    /// 当前没有有意义的数字分母。
    None,
}

/// Immutable phase observation without host UI, database, or provider dependencies.
/// 不依赖宿主界面、数据库或供应商的不可变阶段观察。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VceRefreshProgress {
    /// Real phase reached by the caller.
    /// 调用方实际到达的阶段。
    pub phase: VceRefreshPhase,
    /// Stable child operation identity; resets of counters are scoped to this identity.
    /// 稳定子操作身份；计数重置以此身份为作用域。
    pub operation_id: String,
    /// Meaning of completed and total.
    /// 已完成量与总量的含义。
    pub unit: VceRefreshUnit,
    /// Number of confirmed completions, not initiated requests.
    /// 已确认完成的数量，而不是已发起请求数。
    pub completed: u64,
    /// Known denominator; absence is deliberately indeterminate.
    /// 已知分母；缺失明确表示不确定总量。
    pub total: Option<u64>,
    /// Confirmed units across the whole refresh operation.
    /// 整个刷新操作范围内已确认完成的单元数量。
    #[serde(default)]
    pub overall_completed: u64,
    /// Known denominator across the whole refresh operation.
    /// 整个刷新操作范围内已知的总单元数量。
    #[serde(default)]
    pub overall_total: Option<u64>,
    /// Optional validated source-relative path, never source content.
    /// 可选的已验证源码相对路径，绝不包含源码正文。
    pub current_path: Option<String>,
}

/// Non-blocking host observer; implementations must enqueue or copy metadata, never perform I/O.
/// 非阻塞宿主观察器；实现只能排队或复制元数据，绝不能执行输入输出。
pub trait VceRefreshObserver: Send + Sync {
    /// Observe one real phase event without changing engine ownership.
    /// 观察一次真实阶段事件，不改变引擎归属。
    ///
    /// `progress` is scoped to one operation and contains no code body. Returns no value.
    /// `progress` 以一个操作为作用域且不含代码正文。无返回值。
    fn observe(&self, progress: VceRefreshProgress);
}

impl<F> VceRefreshObserver for F
where
    F: Fn(VceRefreshProgress) + Send + Sync,
{
    /// Forward immutable progress to a non-blocking host callback.
    /// 将不可变进度转发到非阻塞宿主回调。
    ///
    /// `progress` is one actual phase update. Returns nothing.
    /// `progress` 是一次实际阶段更新。无返回值。
    fn observe(&self, progress: VceRefreshProgress) {
        self(progress);
    }
}

/// Result of a cancellation request bound to one exact refresh control.
/// 绑定一个精确刷新控制器的取消请求结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VceCancelOutcome {
    /// Cancellation was accepted or had already been accepted before publication.
    /// 取消已受理，或已在发布前受理。
    Accepted,
    /// The commit boundary has started and its authoritative result must be awaited.
    /// 提交边界已开始，必须等待其权威结果。
    Committing,
    /// The operation is terminal; cancellation cannot undo its result.
    /// 操作已处于终态；取消不能撤销结果。
    Finished,
}

/// A single-use, shareable cancellation and observation context supplied by a host job.
/// 由宿主作业提供的一次性可共享取消与观察上下文。
#[derive(Clone, Default)]
pub struct VceRefreshControl {
    /// Atomic admission state: running=0, cancelled=1, committing=2, finished=3.
    /// 原子准入状态：运行零、已取消一、提交中二、已结束三。
    state: Arc<AtomicU8>,
    /// Optional host observation is absent for legacy direct business calls.
    /// 旧直接业务调用可以不提供宿主观察器。
    observer: Option<Arc<dyn VceRefreshObserver>>,
}

impl VceRefreshControl {
    /// Attach an observer while preserving the same job's shared cancellation identity.
    /// 附加观察器，同时保留同一作业的共享取消身份。
    ///
    /// `observer` receives real phase metadata. Returns this context with the supplied observer.
    /// `observer` 接收真实阶段元数据。返回带有提供观察器的此上下文。
    pub fn with_observer(mut self, observer: Arc<dyn VceRefreshObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// Request cancellation without racing an already admitted commit.
    /// 请求取消，不与已准入提交发生竞态。
    ///
    /// Takes no parameters. Returns accepted, committing, or finished based on atomic ownership.
    /// 无参数。按原子归属返回已受理、提交中或已结束。
    pub fn cancel(&self) -> VceCancelOutcome {
        match self
            .state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) | Err(1) => VceCancelOutcome::Accepted,
            Err(2) => VceCancelOutcome::Committing,
            Err(_) => VceCancelOutcome::Finished,
        }
    }

    /// Check cancellation before bounded source, CPU, or provider work.
    /// 在有界源码、计算或供应商工作前检查取消。
    ///
    /// Takes no parameters. Returns a typed cancellation or control-reuse error when not running.
    /// 无参数。非运行状态返回类型化取消或控制器复用错误。
    pub fn check(&self) -> Result<(), VceError> {
        match self.state.load(Ordering::Acquire) {
            0 => Ok(()),
            1 => Err(VceError::RefreshCancelled),
            _ => Err(VceError::InvalidRequest(String::from(
                "refresh control is no longer cancellable",
            ))),
        }
    }

    /// Publish an observed phase to the optional host sink.
    /// 向可选宿主接收器发布已观察阶段。
    ///
    /// `progress` contains real counters. Returns nothing and never invents timing-based progress.
    /// `progress` 包含真实计数。无返回值，绝不按时间生成进度。
    pub fn observe(&self, progress: VceRefreshProgress) {
        if let Some(observer) = &self.observer {
            observer.observe(progress);
        }
    }

    /// Atomically enter publication only if cancellation has not already won.
    /// 仅在取消尚未获胜时原子进入发布。
    ///
    /// Takes no parameters. Returns success once, otherwise an explicit cancellation/reuse error.
    /// 无参数。仅成功一次，否则返回明确取消或复用错误。
    pub(crate) fn begin_commit(&self) -> Result<(), VceError> {
        match self
            .state
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(()),
            Err(1) => Err(VceError::RefreshCancelled),
            Err(_) => Err(VceError::InvalidRequest(String::from(
                "refresh commit was already admitted or finished",
            ))),
        }
    }

    /// Atomically finish a successful no-change refresh only if cancellation has not won.
    /// 仅在取消尚未获胜时原子完成成功的无变化刷新。
    ///
    /// Takes no parameters. Returns cancellation or a reuse error instead of overwriting admission.
    /// 无参数。返回取消或复用错误，不覆盖已经确定的准入结果。
    pub(crate) fn finish_without_commit(&self) -> Result<(), VceError> {
        match self
            .state
            .compare_exchange(0, 3, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(()),
            Err(1) => Err(VceError::RefreshCancelled),
            Err(_) => Err(VceError::InvalidRequest(String::from(
                "refresh completion was already admitted or finished",
            ))),
        }
    }

    /// Mark the exact refresh terminal after success, failure, or cleanup has settled.
    /// 成功、失败或清理确定后将精确刷新标记为终态。
    ///
    /// Takes no parameters and returns nothing; terminal state never returns to running.
    /// 无参数、无返回值；终态绝不返回运行态。
    pub fn finish(&self) {
        self.state.store(3, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify no-change completion and cancellation have a single atomic winner after an earlier check.
    /// 验证无变化完成与取消在较早检查之后仍具有唯一原子获胜者。
    /// Takes no parameters and returns nothing; covers both orderings and rejects control reuse.
    /// 无参数、无返回值；覆盖两种顺序并拒绝控制器复用。
    #[test]
    fn no_change_completion_cannot_overwrite_accepted_cancellation() {
        // cancelled models cancellation arriving after the final ordinary check.
        // cancelled 模拟取消在最后一次普通检查之后到达。
        let cancelled = VceRefreshControl::default();
        cancelled.check().expect("running before cancellation");
        assert_eq!(cancelled.cancel(), VceCancelOutcome::Accepted);
        assert!(matches!(
            cancelled.finish_without_commit(),
            Err(VceError::RefreshCancelled)
        ));
        assert!(matches!(cancelled.check(), Err(VceError::RefreshCancelled)));
        cancelled.finish();
        assert_eq!(cancelled.cancel(), VceCancelOutcome::Finished);

        // completed models successful settlement winning before a late cancellation.
        // completed 模拟成功结算先于迟到取消获胜。
        let completed = VceRefreshControl::default();
        completed
            .finish_without_commit()
            .expect("settle no changes");
        assert_eq!(completed.cancel(), VceCancelOutcome::Finished);
        assert!(completed.finish_without_commit().is_err());
        assert!(completed.begin_commit().is_err());

        // committing must retain its publication ownership until the host acknowledges its result.
        // committing 必须在宿主确认结果之前保留发布归属。
        let committing = VceRefreshControl::default();
        committing.begin_commit().expect("publication admitted");
        assert!(committing.finish_without_commit().is_err());
        assert_eq!(committing.cancel(), VceCancelOutcome::Committing);
    }

    /// Verify cancellation and commit admission have a single deterministic winner.
    /// 验证取消与提交准入有唯一确定的获胜者。
    ///
    /// Takes no parameters and returns nothing; asserts both possible admission orderings.
    /// 无参数、无返回值；断言两种可能的准入顺序。
    #[test]
    fn refresh_control_cancellation_cannot_undo_commit() {
        // cancelled represents cancellation winning before any publication.
        // cancelled 表示取消先于任何发布获胜。
        let cancelled = VceRefreshControl::default();
        assert_eq!(cancelled.cancel(), VceCancelOutcome::Accepted);
        assert!(matches!(
            cancelled.begin_commit(),
            Err(VceError::RefreshCancelled)
        ));
        assert!(matches!(cancelled.check(), Err(VceError::RefreshCancelled)));
        // committing represents the opposite ordering and must reject cancellation.
        // committing 表示相反顺序，必须拒绝取消。
        let committing = VceRefreshControl::default();
        committing.begin_commit().expect("admit commit");
        assert_eq!(committing.cancel(), VceCancelOutcome::Committing);
        committing.finish();
        assert_eq!(committing.cancel(), VceCancelOutcome::Finished);
        assert!(committing.begin_commit().is_err());
    }
}
