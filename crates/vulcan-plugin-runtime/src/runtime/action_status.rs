//! Invocation ownership and payload-free diagnostics survive a disconnected waiter.
//! 调用归属与无载荷诊断在等待方断开后继续保留。

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Maximum retained completed invocation records; active records are bounded by admission.
/// 完成调用记录的最大保留数；活动记录由入场容量限制。
const COMPLETED_RECORD_LIMIT: usize = 128;
/// Completed records expire instead of retaining identifiers for the service lifetime.
/// 完成记录到期移除，避免在整个服务生命周期内保留标识。
const COMPLETED_RECORD_TTL: Duration = Duration::from_secs(30 * 60);

/// Host renderer nonce and invocation id bind status/cancellation to its initiating client.
/// 宿主渲染器随机标识与调用标识将状态和取消绑定到发起客户端。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginActionIdentity {
    /// Unpredictable renderer nonce, never selected by plugin arguments.
    /// 不可预测的渲染器随机标识，不能由插件参数选择。
    pub client_id: String,
    /// One exact invocation, never reused for replay.
    /// 单次精确调用，不复用来重放。
    pub invocation_id: String,
}

/// Safe status projection contains no arguments, SQL, secrets, results or raw exception text.
/// 安全状态投影不含参数、SQL、密钥、结果或原始异常文本。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginActionStatus {
    /// Stable invocation and action identifiers.
    /// 稳定调用与 Action 标识。
    pub invocation_id: String,
    /// Exact manifest owner.
    /// 精确清单所有者。
    pub plugin_id: String,
    /// Exact declared action.
    /// 精确声明的 Action。
    pub action_id: String,
    /// Current stage or final outcome, updated by the execution owner.
    /// 当前阶段或最终结果，由执行所有者更新。
    pub phase: String,
    /// Whether real execution and its cleanup have returned.
    /// 实际执行及其清理是否已经返回。
    pub terminal: bool,
    /// Budget exhaustion does not imply that native execution has stopped.
    /// 预算耗尽不代表原生执行已停止。
    pub deadline_exceeded: bool,
    /// Total and accumulated stage durations use monotonic clocks.
    /// 总耗时与累计阶段耗时使用单调时钟。
    pub elapsed_ms: u64,
    /// Accumulated durations by phase; no business data is recorded.
    /// 按阶段累计的耗时；不记录业务数据。
    pub stages_ms: BTreeMap<String, u64>,
}

/// One supervised invocation owns this record until real completion.
/// 单个受监督调用持有此记录直至实际完成。
pub(super) struct ActionRecord {
    /// Exact initiating client and invocation UUIDs.
    /// 精确发起客户端及调用 UUID。
    pub(super) identity: VulcanPluginActionIdentity,
    /// Original service epoch and catalog generation.
    /// 原服务纪元及目录代次。
    pub(super) runtime_identity: String,
    /// Immutable manifest owner for diagnostic authorization.
    /// 诊断授权使用的不可变清单所有者。
    pub(super) plugin_id: String,
    /// Exact declared action selected at admission.
    /// 入场选择的精确声明 Action。
    pub(super) action_id: String,
    /// Original monotonic start of the complete invocation.
    /// 完整调用的原单调开始时刻。
    pub(super) entered: Instant,
    /// Frozen absolute budget for all stages.
    /// 全部阶段的冻结绝对预算。
    pub(super) deadline: Instant,
    /// Only declared reads allow withdrawal after submission.
    /// 仅声明为读取的操作允许提交后撤回。
    pub(super) cancellable: bool,
    /// State changes and execution admission share this mutex.
    /// 状态变更与执行准入共享此互斥锁。
    state: Mutex<RecordState>,
    /// Wakes queued work when its originating client cancels it.
    /// 原始客户端取消时唤醒排队任务。
    pub(super) cancelled: tokio::sync::Notify,
}

/// Mutable timing state excludes all business payloads.
/// 可变计时状态排除全部业务载荷。
struct RecordState {
    /// The diagnostic owner retains capacity through physical work, not just its supervisor.
    /// 诊断所有者跨物理工作保留容量，而非仅跨监督任务。
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
    /// Outstanding real operations include queued blocking closures and admitted core work.
    /// 未结束真实操作包含排队阻塞闭包和已入场核心工作。
    active_work: usize,
    /// The first logical completion remains pending until all physical work leaves.
    /// 首次逻辑完成保持待定，直至全部物理工作离开。
    completion: Option<&'static str>,
    phase: String,
    changed: Instant,
    terminal: bool,
    cancel_requested: bool,
    started: bool,
    stages: BTreeMap<String, u64>,
}

tokio::task_local! {
    /// Async supervisor context used to attach preparation to the originating Action.
    /// 用于将准备关联至原始 Action 的异步监督上下文。
    pub(super) static ACTION_RECORD: Arc<ActionRecord>;
}

/// Validated admission inputs include the exact owned capacity permit.
/// 已校验入场输入包含精确持有的容量许可。
pub(super) struct ActionAdmission {
    /// Client and invocation UUIDs select one diagnostic record.
    /// 客户端及调用 UUID 选择一条诊断记录。
    pub identity: VulcanPluginActionIdentity,
    /// Catalog plugin identity is immutable for this invocation.
    /// 本次调用的目录插件身份不可变。
    pub plugin_id: String,
    /// Manifest action identity is immutable for this invocation.
    /// 本次调用的清单 Action 身份不可变。
    pub action_id: String,
    /// Service epoch and generation bind the original UI projection.
    /// 服务纪元与代次绑定原界面投影。
    pub runtime_identity: String,
    /// Original monotonic admission instant starts all timing.
    /// 原单调入场时刻开启全部计时。
    pub entered: Instant,
    /// Original absolute deadline cannot be extended by scheduling.
    /// 原绝对截止时间不能因调度而延长。
    pub deadline: Instant,
    /// Only read actions permit withdrawal before execution admission.
    /// 只有读取型 Action 允许执行入场前撤回。
    pub cancellable: bool,
    /// Physical completion releases the outstanding action admission.
    /// 物理完成释放未结束 Action 的入场容量。
    pub permit: tokio::sync::OwnedSemaphorePermit,
}

/// One real operation retains its action record and optional persistent admission gate.
/// 一个真实操作保留其 Action 记录及可选持久入场门。
pub(super) struct ActionWorkGuard {
    /// The record owns the capacity permit without retaining a runtime or manager.
    /// 记录拥有容量许可，但不保留运行时或管理器。
    record: Arc<ActionRecord>,
    /// A persistent UI gate follows real work even when its observer is dropped.
    /// 即使观察者被丢弃，持久 UI 门仍跟随真实工作。
    gate: Option<tokio::sync::OwnedMutexGuard<()>>,
}
impl ActionWorkGuard {
    /// Retain record and optional gate before dispatch; reject completion or counter overflow.
    /// 在派发前保留 record 与可选 gate；拒绝已完成记录或计数溢出。
    pub(super) fn new(
        record: Arc<ActionRecord>,
        gate: Option<tokio::sync::OwnedMutexGuard<()>>,
    ) -> Result<Self, VulcanPluginRuntimeError> {
        {
            // Registration and logical completion share one atomic state transition.
            // 注册及逻辑完成共享一个原子状态转换。
            let mut state = record
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.completion.is_some() {
                return Err(VulcanPluginRuntimeError::new(
                    "Action ownership already completed.",
                ));
            }
            state.active_work = state.active_work.checked_add(1).ok_or_else(|| {
                VulcanPluginRuntimeError::new("Action physical ownership count overflow.")
            })?;
        }
        Ok(Self { record, gate })
    }

    /// Return the exact trusted diagnostic owner for a native callback.
    /// 返回原生回调的精确可信诊断所有者。
    pub(super) fn record(&self) -> Arc<ActionRecord> {
        Arc::clone(&self.record)
    }
}
impl Drop for ActionWorkGuard {
    /// Release the actual gate before publishing terminal status and returning action capacity.
    /// 发布终态及归还 Action 容量之前释放真实入场门。
    fn drop(&mut self) {
        drop(self.gate.take());
        // Every guard increments once and is moved, never cloned.
        // 每个守卫恰好递增一次，且只移动、不克隆。
        let mut state = self
            .record
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active_work -= 1;
        state.publish_completion();
    }
}
impl RecordState {
    /// Publish the recorded outcome only after the last physical guard leaves; return no value.
    /// 仅在最后一个物理守卫离开后发布已记录结果；不返回值。
    fn publish_completion(&mut self) {
        if self.terminal || self.active_work != 0 {
            return;
        }
        let Some(outcome) = self.completion else {
            return;
        };
        // Freeze final timing before releasing capacity for a successor.
        // 为后继释放容量之前冻结最终计时。
        let previous = self.phase.clone();
        *self.stages.entry(previous).or_default() += self.changed.elapsed().as_millis() as u64;
        self.phase = outcome.into();
        self.changed = Instant::now();
        self.terminal = true;
        self.permit.take();
    }
}

impl ActionRecord {
    /// Record the first logical outcome; physical work must finish before terminal publication.
    /// 记录首次逻辑结果；物理工作必须先结束，才能发布终态。
    fn complete(&self, outcome: &'static str) {
        // Idempotence also prevents a completion guard from overwriting a successful result.
        // 幂等性同时防止完成守卫覆盖成功结果。
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.completion.is_some() {
            return;
        }
        state.completion = Some(outcome);
        if state.active_work != 0 {
            // Accumulate elapsed time under the previous phase before transitioning.
            // 转换之前，在上一阶段下累计经过时间。
            let previous = state.phase.clone();
            // Measure the phase with the same monotonic clock as admission.
            // 使用与入场相同的单调时钟测量阶段。
            let elapsed = state.changed.elapsed().as_millis() as u64;
            *state.stages.entry(previous).or_default() += elapsed;
            state.phase = "cleanup_wait".into();
            state.changed = Instant::now();
        }
        state.publish_completion();
    }
    /// Records waiter cancellation without interrupting an already running VM.
    /// 记录等待方取消，不中断已经运行的 VM。
    pub(super) fn withdraw(&self) {
        // Serialize cancellation with business admission; late observer loss preserves timeout classification.
        // 将取消与业务入场排序；观测者迟到丢失保留超时分类。
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.cancellable && !state.started && !state.terminal && Instant::now() < self.deadline {
            state.cancel_requested = true;
        }
    }
    /// Record the supervisor result; publish it only after registered physical work finishes.
    /// 记录监督结果；仅在已登记物理工作结束后发布。
    pub(super) fn finish(&self, succeeded: bool) {
        // Read cancellation admission before choosing the logical outcome.
        // 选择逻辑结果之前读取取消入场状态。
        let cancelled = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cancel_requested;
        self.complete(if succeeded {
            "succeeded"
        } else if cancelled {
            "cancelled"
        } else if Instant::now() >= self.deadline {
            "timed_out"
        } else {
            "failed"
        });
    }
    /// Accumulates a measured boundary without changing the execution state.
    /// 累计实测边界耗时，不改变执行状态。
    pub(super) fn observe(&self, phase: &str, duration: Duration) {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stages
            .entry(phase.to_owned())
            .or_default() += duration.as_millis() as u64;
    }
    /// Records one stage transition and accumulates repeated stage durations.
    /// 记录阶段转换，并累加重复阶段的耗时。
    pub(super) fn phase(&self, phase: &str) {
        // Read or update one exact ownership record under its short evidence lock.
        // 在短证据锁下读取或更新一条精确所有权记录。
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.completion.is_some() {
            return;
        }
        // Accumulate elapsed time under the previous phase before transitioning.
        // 转换之前，在上一阶段下累计经过时间。
        let previous = state.phase.clone();
        // Measure the phase with the same monotonic clock as admission.
        // 使用与入场相同的单调时钟测量阶段。
        let elapsed = state.changed.elapsed().as_millis() as u64;
        *state.stages.entry(previous).or_default() += elapsed;
        state.phase = phase.to_owned();
        state.changed = Instant::now();
    }
    /// Rejects client cancellation or an expired deadline before entering Lua.
    /// 进入 Lua 前拒绝客户端取消或已过期截止时间。
    pub(super) fn check(&self) -> Result<(), VulcanPluginRuntimeError> {
        // Read or update one exact ownership record under its short evidence lock.
        // 在短证据锁下读取或更新一条精确所有权记录。
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.cancel_requested {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin action cancelled before execution.",
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin action deadline exceeded before execution.",
            ));
        }
        Ok(())
    }
    /// Atomically closes cancellation admission before any action-side effects start.
    /// 在任何 Action 副作用开始前原子关闭取消准入。
    pub(super) fn start(&self) -> Result<(), VulcanPluginRuntimeError> {
        // Read or update one exact ownership record under its short evidence lock.
        // 在短证据锁下读取或更新一条精确所有权记录。
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.cancel_requested || Instant::now() >= self.deadline {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin action cancelled or expired before execution.",
            ));
        }
        // Accumulate elapsed time under the previous phase before transitioning.
        // 转换之前，在上一阶段下累计经过时间。
        let previous = state.phase.clone();
        // Measure the phase with the same monotonic clock as admission.
        // 使用与入场相同的单调时钟测量阶段。
        let elapsed = state.changed.elapsed().as_millis() as u64;
        *state.stages.entry(previous).or_default() += elapsed;
        state.phase = "executing".into();
        state.started = true;
        state.changed = Instant::now();
        Ok(())
    }
    /// Returns a current safe projection, including time spent in the active stage.
    /// 返回当前安全投影，包含当前阶段已经消耗的时间。
    pub(super) fn snapshot(&self) -> VulcanPluginActionStatus {
        // Read or update one exact ownership record under its short evidence lock.
        // 在短证据锁下读取或更新一条精确所有权记录。
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stages_ms = state.stages.clone();
        if !state.terminal {
            *stages_ms.entry(state.phase.clone()).or_default() +=
                state.changed.elapsed().as_millis() as u64;
        }
        let end = if state.terminal {
            state.changed
        } else {
            Instant::now()
        };
        VulcanPluginActionStatus {
            invocation_id: self.identity.invocation_id.clone(),
            plugin_id: self.plugin_id.clone(),
            action_id: self.action_id.clone(),
            phase: state.phase.clone(),
            terminal: state.terminal,
            deadline_exceeded: end >= self.deadline,
            elapsed_ms: end.duration_since(self.entered).as_millis() as u64,
            stages_ms,
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Returns absence explicitly during registration races; existing records still require ownership.
    /// 注册竞态期间明确返回不存在；已有记录仍须通过归属校验。
    pub fn find_action_status(
        &self,
        plugin_id: &str,
        runtime_identity: &str,
        identity: &VulcanPluginActionIdentity,
        cancel: bool,
    ) -> Result<Option<VulcanPluginActionStatus>, VulcanPluginRuntimeError> {
        let records = self.state.action_records.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Plugin invocation registry is poisoned.")
        })?;
        let Some(record) = records.get(&identity.invocation_id).cloned() else {
            return Ok(None);
        };
        drop(records);
        Self::owned_action_status(record, plugin_id, runtime_identity, identity, cancel).map(Some)
    }
    /// Creates a bounded diagnostic record, refusing reused invocation ids.
    /// 创建有界诊断记录，拒绝复用调用标识。
    pub(super) fn register_action(
        &self,
        admission: ActionAdmission,
    ) -> Result<Arc<ActionRecord>, VulcanPluginRuntimeError> {
        // All identity, budget and capacity fields belong to the same admission.
        // 全部身份、预算与容量字段属于同一次入场。
        let ActionAdmission {
            identity,
            plugin_id,
            action_id,
            runtime_identity,
            entered,
            deadline,
            cancellable,
            permit,
        } = admission;
        if uuid::Uuid::parse_str(&identity.client_id).is_err()
            || uuid::Uuid::parse_str(&identity.invocation_id).is_err()
        {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin invocation identity requires UUIDs.",
            ));
        }
        let mut records = self.state.action_records.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Plugin invocation registry is poisoned.")
        })?;
        records.retain(|_, record| {
            !record.snapshot().terminal || record.entered.elapsed() < COMPLETED_RECORD_TTL
        });
        let mut completed = records
            .iter()
            .filter(|(_, record)| record.snapshot().terminal)
            .map(|(id, record)| (record.entered, id.clone()))
            .collect::<Vec<_>>();
        completed.sort();
        let excess = completed
            .len()
            .saturating_sub(COMPLETED_RECORD_LIMIT.saturating_sub(1));
        for (_, id) in completed.into_iter().take(excess) {
            records.remove(&id);
        }
        if records.contains_key(&identity.invocation_id) {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin invocation id was already used; replay is not allowed.",
            ));
        }
        let record = Arc::new(ActionRecord {
            identity,
            runtime_identity,
            plugin_id,
            action_id,
            entered,
            deadline,
            cancellable,
            state: Mutex::new(RecordState {
                permit: Some(permit),
                active_work: 0,
                completion: None,
                phase: "admitted".into(),
                changed: entered,
                terminal: false,
                cancel_requested: false,
                started: false,
                stages: BTreeMap::new(),
            }),
            cancelled: tokio::sync::Notify::new(),
        });
        records.insert(record.identity.invocation_id.clone(), Arc::clone(&record));
        Ok(record)
    }

    /// Queries or cancels only the exact caller-owned invocation; never replays business work.
    /// 仅查询或取消精确调用方持有的调用；绝不重放业务操作。
    pub fn action_status(
        &self,
        plugin_id: &str,
        runtime_identity: &str,
        identity: &VulcanPluginActionIdentity,
        cancel: bool,
    ) -> Result<VulcanPluginActionStatus, VulcanPluginRuntimeError> {
        self.find_action_status(plugin_id, runtime_identity, identity, cancel)?.ok_or_else(|| VulcanPluginRuntimeError::new("Plugin invocation status is unavailable or expired; do not infer mutation failure."))
    }

    /// Checks ownership and atomically withdraws admission only before execution begins.
    /// 校验归属，并仅在执行开始前原子撤回准入。
    fn owned_action_status(
        record: Arc<ActionRecord>,
        plugin_id: &str,
        runtime_identity: &str,
        identity: &VulcanPluginActionIdentity,
        cancel: bool,
    ) -> Result<VulcanPluginActionStatus, VulcanPluginRuntimeError> {
        if record.identity.client_id != identity.client_id
            || record.plugin_id != plugin_id
            || record.runtime_identity != runtime_identity
        {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin invocation is owned by another client or runtime.",
            ));
        }
        if cancel && record.cancellable {
            // Read or update one exact ownership record under its short evidence lock.
            // 在短证据锁下读取或更新一条精确所有权记录。
            let mut state = record
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !state.terminal && !state.started && Instant::now() < record.deadline {
                state.cancel_requested = true;
                record.cancelled.notify_one();
            }
        }
        Ok(record.snapshot())
    }
}

/// Measures a synchronous host callback without retaining its arguments.
/// 测量同步宿主回调，不保留其参数。
pub(super) struct HostCallTimer(Option<Arc<ActionRecord>>, Instant);
impl HostCallTimer {
    /// Measure a callback attributed to an explicitly authenticated record, or no action.
    /// 测量归属于明确已认证记录的回调，或无 Action 的回调。
    pub(super) fn for_record(record: Option<Arc<ActionRecord>>) -> Self {
        Self(record, Instant::now())
    }
}
impl Drop for HostCallTimer {
    fn drop(&mut self) {
        if let Some(record) = &self.0 {
            record.observe("host_callback", self.1.elapsed());
        }
    }
}

/// Records abnormal supervision while physical guards retain status and capacity.
/// 记录异常监督退出，物理守卫继续保留状态及容量。
pub(super) struct ActionCompletionGuard(pub(super) Arc<ActionRecord>);
impl Drop for ActionCompletionGuard {
    /// Preserve the first result, or record supervisor failure pending actual physical completion.
    /// 保留首次结果，或记录等待真实物理完成的监督失败。
    fn drop(&mut self) {
        self.0.complete("supervisor_failed");
    }
}
