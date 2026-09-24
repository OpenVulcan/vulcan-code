//! Invocation ownership and payload-free diagnostics survive a disconnected waiter.
//! 调用归属与无载荷诊断在等待方断开后继续保留。

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
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
    pub(super) identity: VulcanPluginActionIdentity,
    pub(super) runtime_identity: String,
    pub(super) plugin_id: String,
    pub(super) action_id: String,
    pub(super) entered: Instant,
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
    phase: String,
    changed: Instant,
    terminal: bool,
    cancel_requested: bool,
    started: bool,
    stages: BTreeMap<String, u64>,
}

tokio::task_local! {
    /// Async supervisor context propagated explicitly onto engine blocking threads.
    /// 异步监督上下文，显式传播到引擎阻塞线程。
    pub(super) static ACTION_RECORD: Arc<ActionRecord>;
}
thread_local! {
    /// Only the current synchronous engine call may populate this callback context.
    /// 仅当前同步引擎调用可以设置此回调上下文。
    static BLOCKING_RECORD: RefCell<Option<Arc<ActionRecord>>> = const { RefCell::new(None) };
}

/// Restores the preceding callback context on normal return and panic alike.
/// 正常返回与异常展开时都恢复先前回调上下文。
pub(super) struct BlockingActionScope(Option<Arc<ActionRecord>>);
impl BlockingActionScope {
    /// Installs an optional supervisor record for one blocking closure.
    /// 为单个阻塞闭包安装可选监督记录。
    pub(super) fn enter(record: Option<Arc<ActionRecord>>) -> Self {
        Self(BLOCKING_RECORD.with(|slot| slot.replace(record)))
    }
}
impl Drop for BlockingActionScope {
    fn drop(&mut self) {
        BLOCKING_RECORD.with(|slot| slot.replace(self.0.take()));
    }
}

/// Returns the actual host deadline for the currently executing plugin callback.
/// 返回当前执行插件回调的实际宿主截止时间。
pub(super) fn callback_deadline() -> Option<Instant> {
    BLOCKING_RECORD.with(|slot| slot.borrow().as_ref().map(|record| record.deadline))
}

impl ActionRecord {
    /// Records waiter cancellation without interrupting an already running VM.
    /// 记录等待方取消，不中断已经运行的 VM。
    pub(super) fn withdraw(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.cancellable && !state.started {
            state.cancel_requested = true;
        }
    }
    /// Records a distinct final outcome after actual execution and cleanup returned.
    /// 实际执行与清理返回后记录明确区分的最终结果。
    pub(super) fn finish(&self, succeeded: bool) {
        let cancelled = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cancel_requested;
        self.phase(
            if succeeded {
                "succeeded"
            } else if cancelled {
                "cancelled"
            } else if Instant::now() >= self.deadline {
                "timed_out"
            } else {
                "failed"
            },
            true,
        );
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
    pub(super) fn phase(&self, phase: &str, terminal: bool) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = state.phase.clone();
        let elapsed = state.changed.elapsed().as_millis() as u64;
        *state.stages.entry(previous).or_default() += elapsed;
        state.phase = phase.to_owned();
        state.changed = Instant::now();
        state.terminal = terminal;
    }
    /// Rejects client cancellation or an expired deadline before entering Lua.
    /// 进入 Lua 前拒绝客户端取消或已过期截止时间。
    pub(super) fn check(&self) -> Result<(), VulcanPluginRuntimeError> {
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
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.cancel_requested || Instant::now() >= self.deadline {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin action cancelled or expired before execution.",
            ));
        }
        let previous = state.phase.clone();
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
        identity: VulcanPluginActionIdentity,
        plugin_id: &str,
        action_id: &str,
        runtime_identity: String,
        entered: Instant,
        deadline: Instant,
        cancellable: bool,
    ) -> Result<Arc<ActionRecord>, VulcanPluginRuntimeError> {
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
            plugin_id: plugin_id.into(),
            action_id: action_id.into(),
            entered,
            deadline,
            cancellable,
            state: Mutex::new(RecordState {
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
            let mut state = record
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !state.terminal && !state.started {
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
    /// Captures only the current trusted blocking context and start instant.
    /// 仅捕获当前可信阻塞上下文与开始时刻。
    pub(super) fn start() -> Self {
        Self(
            BLOCKING_RECORD.with(|slot| slot.borrow().clone()),
            Instant::now(),
        )
    }
}
impl Drop for HostCallTimer {
    fn drop(&mut self) {
        if let Some(record) = &self.0 {
            record.observe("host_callback", self.1.elapsed());
        }
    }
}

/// Marks abnormal supervisor exits terminal without releasing ownership before stack cleanup.
/// 将异常监督退出标为终态，不在栈清理前释放所有权。
pub(super) struct ActionCompletionGuard(pub(super) Arc<ActionRecord>);
impl Drop for ActionCompletionGuard {
    fn drop(&mut self) {
        if !self.0.snapshot().terminal {
            self.0.phase("supervisor_failed", true);
        }
    }
}
