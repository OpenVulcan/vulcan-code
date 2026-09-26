//! Production owner of the LuaSkills install/update progress callback.
//! LuaSkills 安装/更新进度回调的生产所有者。
//!
//! Upstream emits fine-grained progress through one process-wide callback slot
//! whose event carries only an upstream-generated operation id. The service needs
//! its own correlation identity so a WebUI refresh or reconnect can resume a
//! known operation, and it must never guess that identity from `skill_id`, a
//! timestamp, or a thread id.
//!
//! 上游通过一个进程级回调槽发出细粒度进度，其事件只携带上游自己生成的操作标识。服务需要
//! 自己的关联身份，使 WebUI 刷新或重连后能恢复已知操作，且绝不从 `skill_id`、时间戳或
//! 线程标识猜测该身份。
//!
//! The service therefore generates the correlation id before issuing the request
//! and binds it for exactly the span in which the LuaSkills management mutation
//! lock is held. Because that lock already serializes every LuaSkills management
//! RPC, at most one LuaSkills operation can be in flight, so attribution inside
//! the bound span is exact rather than inferred.
//!
//! 因此服务在发起请求前生成关联标识，并只在持有 LuaSkills 管理变更锁的区间内绑定它。
//! 由于该锁已经串行化每个 LuaSkills 管理 RPC，同一时刻最多只有一个 LuaSkills 操作在途，
//! 所以绑定区间内的归属是精确的，而不是推断的。
//!
//! Boundaries enforced here:
//! - Events outside a bound operation are counted and dropped, never attributed.
//! - `source_locator` and free-form upstream messages never reach the projection.
//! - The queue is bounded; overflow keeps the terminal status and is diagnosed.
//! - The callback only appends to a bounded in-memory queue, so it cannot block,
//!   re-enter LuaSkills, or wait on the engine.
//!
//! 本模块强制的边界：
//! - 绑定区间之外的事件只被计数并丢弃，绝不归属。
//! - `source_locator` 与上游自由文本绝不进入投影。
//! - 队列有界；溢出时保留终态并给出诊断。
//! - 回调只向有界内存队列追加，因此不会阻塞、不会重入 LuaSkills、不会等待引擎。

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use luaskills::{
    RuntimeSkillOperationProgressCallback, RuntimeSkillOperationProgressEvent,
    SkillLifecycleAction, SkillOperationPlane, clear_skill_operation_progress_callback_if,
    set_skill_operation_progress_callback,
};
use sha2::{Digest, Sha256};
use tokio::sync::watch;

use crate::state::SharedServiceBridgeState;
use vulcan_service_api::{LuaSkillsOperationProgressSummary, ServiceEventEmitter};

/// Maximum progress events retained for one operation.
/// 单个操作保留的进度事件上限。
///
/// A long download can emit many `progress` events; only the tail plus the
/// terminal event matter to a projection.
/// 长时间下载可能发出大量 `progress` 事件；投影只关心尾部以及终态事件。
const MAX_EVENTS_PER_OPERATION: usize = 64;

/// Maximum finished operations retained for late status queries.
/// 为迟到状态查询保留的已完成操作上限。
const MAX_RETAINED_OPERATIONS: usize = 32;

/// Maximum characters retained from one machine-readable upstream field.
/// 单个机器可读上游字段保留的最大字符数。
///
/// Real phases, statuses, skill ids, and root names are far shorter than this, so the
/// bound never alters a legitimate value; it only stops a malformed one from being
/// retained once per event.
/// 真实的阶段、状态、技能标识与根名远短于此，因此该上界绝不改动合法取值，只是阻止畸形取值
/// 被每个事件保留一次。
const MAX_FIELD_CHARS: usize = 128;

/// Sentinel used when no operation may publish an in-flight projection.
/// 在没有操作可以发布在途投影时使用的哨兵值。
const NO_ACTIVE_PROJECTION_ORDINAL: u64 = u64::MAX;

/// Process-wide monotonic sequence used to identify exact LuaSkills management requests.
/// 用于标识精确 LuaSkills 管理请求的进程级单调序列。
///
/// Callback owners may be replaced while the desktop process remains alive. Keeping the
/// sequence outside an owner prevents a reconnect holding an older correlation id from
/// resolving a different request after service re-bootstrap.
/// 桌面进程仍存活时可能替换回调所有者。把序列放在所有者之外，可防止持有旧关联标识的
/// 重连请求在服务重新引导后解析到另一条请求。
static LUASKILLS_OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One ordered, sanitized projection update queued for the service runtime.
/// 排队给服务运行时的一条有序、脱敏投影更新。
#[derive(Debug, Clone, PartialEq)]
struct LuaSkillsProgressProjectionUpdate {
    /// Process-wide monotonic ordinal of the operation that produced this update.
    /// 产生本次更新的操作在进程范围内的单调序号。
    operation_ordinal: u64,
    /// Sanitized public progress summary.
    /// 脱敏后的公开进度摘要。
    progress: LuaSkillsOperationProgressSummary,
}

/// Bounds one machine-readable upstream field for retention.
/// 为一个机器可读的上游字段施加保留上界。
fn bound_field(value: &str) -> String {
    value.chars().take(MAX_FIELD_CHARS).collect()
}

/// Computes a fixed-size identity digest from one complete upstream operation id.
/// 根据完整的上游操作标识计算定长身份摘要。
fn operation_identity_digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

/// Locks one mutex while recovering its protected value after poisoning.
/// 锁定一个互斥量，并在中毒后恢复其受保护的值。
///
/// # Parameters
/// 参数
/// `mutex` is the synchronization primitive whose value must be accessed.
/// `mutex` 是需要访问其受保护值的同步原语。
///
/// # Returns
/// 返回值
/// Returns the acquired guard.
/// 返回已取得的守卫。
fn lock_mutex<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Status text upstream uses for a completed phase.
/// 上游用于"阶段完成"的状态文本。
const STATUS_COMPLETED: &str = "completed";

/// Status text upstream uses for a failed phase.
/// 上游用于"阶段失败"的状态文本。
const STATUS_FAILED: &str = "failed";

/// One sanitized progress event retained for projection.
/// 为投影保留的一条脱敏进度事件。
///
/// `source_locator` is deliberately absent: it can carry a private source URL,
/// a local absolute directory, or an embedded token.
/// `source_locator` 被刻意移除：它可能携带私有来源 URL、本地绝对目录或内嵌令牌。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LuaSkillsProgressEvent {
    /// Monotonic sequence number within the operation.
    /// 操作内单调递增的序号。
    pub(crate) sequence: u64,
    /// Operation plane reported by upstream.
    /// 上游报告的操作平面。
    pub(crate) plane: SkillOperationPlane,
    /// Lifecycle action reported by upstream.
    /// 上游报告的生命周期动作。
    pub(crate) action: SkillLifecycleAction,
    /// Machine-readable phase name.
    /// 机器可读阶段名称。
    pub(crate) phase: String,
    /// Machine-readable status.
    /// 机器可读状态。
    pub(crate) status: String,
    /// Optional target skill id.
    /// 可选目标技能标识。
    pub(crate) skill_id: Option<String>,
    /// Optional named skill root.
    /// 可选命名技能根。
    pub(crate) root_name: Option<String>,
    /// Optional completed byte count.
    /// 可选已完成字节数。
    pub(crate) bytes_done: Option<u64>,
    /// Optional total byte count.
    /// 可选总字节数。
    pub(crate) bytes_total: Option<u64>,
    /// Optional determinate percentage.
    /// 可选确定性百分比。
    pub(crate) percent: Option<f64>,
    /// Optional sanitized human-readable message.
    /// 可选脱敏后的人类可读消息。
    pub(crate) message: Option<String>,
}

impl LuaSkillsProgressEvent {
    /// Returns whether this event reports a terminal phase status.
    /// 返回本事件是否报告终态阶段状态。
    pub(crate) fn is_terminal(&self) -> bool {
        (self.phase == STATUS_COMPLETED && self.status == STATUS_COMPLETED)
            || (self.phase == STATUS_FAILED && self.status == STATUS_FAILED)
    }
}

/// Terminal outcome of one install or update operation.
/// 单次安装或更新操作的终态结果。
///
/// The RPC and its committed transaction are authoritative, so this is recorded
/// from the request result and never inferred from progress events.
/// RPC 及其已提交事务是权威，因此本值来自请求结果，绝不从进度事件推断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LuaSkillsOperationOutcome {
    /// The request returned success.
    /// 请求返回成功。
    Succeeded,
    /// The request returned an error.
    /// 请求返回错误。
    Failed,
}

/// One retained operation projection keyed by the service correlation id.
/// 以服务关联标识为键的一条保留操作投影。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LuaSkillsOperationProgress {
    /// Monotonic owner-local ordinal used only to reject stale asynchronous projections.
    /// 仅用于拒绝陈旧异步投影的当前所有者内单调序号。
    operation_ordinal: u64,
    /// Service-generated correlation identity for this exact request.
    /// 为本次精确请求生成的服务关联身份。
    pub(crate) correlation_id: String,
    /// Lifecycle action the service requested.
    /// 服务请求的生命周期动作。
    pub(crate) action: SkillLifecycleAction,
    /// Whether the operation is still inside its bound span.
    /// 该操作是否仍在其绑定区间内。
    pub(crate) in_flight: bool,
    /// Retained sanitized events in emission order.
    /// 按发出顺序保留的脱敏事件。
    pub(crate) events: VecDeque<LuaSkillsProgressEvent>,
    /// Terminal outcome once the request returned.
    /// 请求返回后的终态结果。
    pub(crate) outcome: Option<LuaSkillsOperationOutcome>,
    /// Whether any event was dropped because the queue was full.
    /// 是否因队列已满而丢弃过事件。
    pub(crate) overflowed: bool,
    /// Whether an upstream terminal event was observed.
    /// 是否观察到上游终态事件。
    pub(crate) saw_upstream_terminal: bool,
    /// Upstream operation identity bound by the first observed event.
    /// 由首个观察到的事件绑定的上游操作身份。
    ///
    /// The upstream event carries only its own operation id, so binding it on the
    /// first event is what keeps attribution exact: every later event must match
    /// this id or it belongs to a different operation and is dropped rather than
    /// misattributed.
    /// 上游事件只携带自己的操作标识，因此在首个事件上绑定它就是归属精确的原因：之后每个
    /// 事件都必须匹配该标识，否则属于另一操作，将被丢弃而不是被错误归属。
    pub(crate) upstream_operation_id: Option<String>,
    /// Fixed-size digest used to compare the complete upstream operation identity.
    /// 用于比较完整上游操作身份的定长摘要。
    ///
    /// The user-visible id is bounded, so comparing that projection would merge two
    /// distinct long ids with the same prefix. This digest preserves full-input
    /// identity without retaining an unbounded duplicate string.
    /// 面向用户的标识受长度限制，因此比较该投影会把具有相同前缀的两个不同长标识合并。
    /// 本摘要保留完整输入的身份语义，同时不额外保留无界字符串副本。
    upstream_operation_digest: Option<[u8; 32]>,
}

impl LuaSkillsOperationProgress {
    /// Returns the most recent retained event.
    /// 返回最近保留的事件。
    pub(crate) fn latest(&self) -> Option<&LuaSkillsProgressEvent> {
        self.events.back()
    }
}

/// Mutable process-wide progress state.
/// 进程级可变进度状态。
#[derive(Default)]
struct ProgressState {
    /// Correlation id currently bound to the LuaSkills management lock.
    /// 当前绑定到 LuaSkills 管理锁的关联标识。
    active: Option<String>,
    /// Retained operations keyed by correlation id.
    /// 按关联标识保留的操作。
    operations: HashMap<String, LuaSkillsOperationProgress>,
    /// Insertion order used to evict finished operations.
    /// 用于淘汰已完成操作的插入顺序。
    order: VecDeque<String>,
    /// Events observed with no bound operation.
    /// 在无绑定操作时观察到的事件数。
    orphaned_events: u64,
    /// Events whose upstream operation identity did not match the bound record.
    /// 上游操作身份与已绑定记录不匹配的事件数。
    ///
    /// A non-zero value is a real defect signal: it means an upstream event source
    /// exists that the management lock does not serialize.
    /// 非零值是真实的缺陷信号：说明存在管理锁未能串行化的上游事件来源。
    foreign_events: u64,
}

/// Process-wide owner of the LuaSkills progress callback.
/// LuaSkills 进度回调的进程级所有者。
pub(crate) struct LuaSkillsInstallProgress {
    /// Shared state mutated by the callback and the service path.
    /// 由回调与服务路径共同改写的共享状态。
    ///
    /// Held behind an `Arc` because the installed callback owns its own clone.
    /// 使用 `Arc` 持有，因为已安装的回调持有自己的克隆。
    state: Arc<Mutex<ProgressState>>,
    /// Callback handle kept so the owner can re-install or clear it.
    /// 保留的回调句柄，使所有者能够重新安装或清除它。
    callback: RuntimeSkillOperationProgressCallback,
    /// Latest-value channel used to leave the upstream callback without blocking on WebUI projection.
    /// 用于在不等待 WebUI 投影的情况下退出上游回调的最新值通道。
    projection_sender: Option<watch::Sender<Option<LuaSkillsProgressProjectionUpdate>>>,
    /// Service-runtime task that commits projected progress after the callback returns.
    /// 在回调返回后提交进度投影的服务运行时任务。
    projection_task: Option<tokio::task::JoinHandle<()>>,
    /// Orders asynchronous in-flight commits against begin and terminal transitions.
    /// 在异步在途提交、开始转换与终态转换之间建立顺序。
    projection_order: Arc<Mutex<()>>,
    /// Ordinal currently allowed to publish an in-flight projection.
    /// 当前获准发布在途投影的操作序号。
    active_projection_ordinal: Arc<AtomicU64>,
}

/// Projects one free-form upstream message onto a finite host-owned vocabulary.
/// 把一条上游自由文本消息投影到有限的宿主自有词表。
///
/// Arbitrary text cannot be proven free of credentials, paths, private URLs, or package
/// content by token heuristics. The structured phase, status, byte counters, and request
/// error already carry the actionable state, so only a small set of host-authored messages
/// crosses the service boundary. The upstream message is used solely as a presence signal;
/// none of its bytes are copied.
/// 任意文本无法通过 token 启发式规则证明不含凭据、路径、私有 URL 或技能包内容。结构化阶段、
/// 状态、字节计数与请求错误已经携带可操作状态，因此只有少量宿主自有文案跨越服务边界。上游
/// 消息仅作为“存在”信号使用，其中任何字节都不会被复制。
fn project_safe_message(event: &RuntimeSkillOperationProgressEvent) -> Option<String> {
    event.message.as_ref()?;
    match (event.phase.as_str(), event.status.as_str()) {
        ("downloading_archive", "cached") => Some(String::from(
            "LuaSkills archive resolved from the local cache.",
        )),
        (_, "failed") => Some(String::from(
            "LuaSkills operation failed. Review the request error for recovery details.",
        )),
        (_, "blocked") => Some(String::from(
            "LuaSkills operation was blocked. Review the request error for recovery details.",
        )),
        _ => None,
    }
}

/// Projects one upstream event into the sanitized retained shape.
/// 把一个上游事件投影为脱敏后的保留形状。
fn project_event(event: &RuntimeSkillOperationProgressEvent) -> LuaSkillsProgressEvent {
    LuaSkillsProgressEvent {
        sequence: event.sequence,
        plane: event.plane,
        action: event.action,
        // Machine-readable fields are bound for the same reason the message is: upstream
        // does not bound them either, and these are retained across every event of every
        // operation, so one oversized value is paid many times over.
        // 机器可读字段与消息出于同一原因受界：上游同样不限制它们，而这些值会被跨每个操作的
        // 每个事件保留，因此一个超长值会被反复付出代价。
        phase: bound_field(&event.phase),
        status: bound_field(&event.status),
        skill_id: event.skill_id.as_deref().map(bound_field),
        root_name: event.root_name.as_deref().map(bound_field),
        bytes_done: event.bytes_done,
        bytes_total: event.bytes_total,
        percent: event.percent,
        message: project_safe_message(event),
    }
}

impl LuaSkillsInstallProgress {
    /// Creates the owner and its process-wide callback.
    /// 创建所有者及其进程级回调。
    ///
    /// # Parameters
    /// 参数
    /// `projection_sender` publishes only the newest sanitized summary without waiting for a consumer.
    /// `projection_sender` 仅发布最新的脱敏摘要且不等待消费者。
    /// `projection_task` owns the service-side consumer when production projection is enabled.
    /// `projection_task` 在启用生产投影时持有服务侧消费者。
    ///
    /// # Returns
    /// 返回值
    /// Returns the callback owner ready for process-wide installation.
    /// 返回可安装到进程级槽的回调所有者。
    fn new(
        projection_sender: Option<watch::Sender<Option<LuaSkillsProgressProjectionUpdate>>>,
        projection_task: Option<tokio::task::JoinHandle<()>>,
        projection_order: Arc<Mutex<()>>,
        active_projection_ordinal: Arc<AtomicU64>,
    ) -> Arc<Self> {
        // The callback captures only a weak-free shared state: no engine, no
        // service emitter, no network, so it can never block or re-enter LuaSkills.
        // 回调只捕获共享状态：没有引擎、没有服务事件发射器、没有网络，因此它绝不会阻塞
        // 或重入 LuaSkills。
        let state_slot: Arc<Mutex<ProgressState>> = Arc::new(Mutex::new(ProgressState::default()));
        let callback_state = Arc::clone(&state_slot);
        let callback_projection_sender = projection_sender.clone();
        let callback: RuntimeSkillOperationProgressCallback = Arc::new(move |event| {
            let mut state = match callback_state.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let Some(correlation_id) = state.active.clone() else {
                // No bound operation: counting is the honest response, because
                // attributing this event would be a guess.
                // 无绑定操作：计数是诚实的处理，因为归属该事件只能是猜测。
                state.orphaned_events = state.orphaned_events.saturating_add(1);
                return;
            };
            let projected = project_event(event);
            let Some(operation) = state.operations.get_mut(&correlation_id) else {
                state.orphaned_events = state.orphaned_events.saturating_add(1);
                return;
            };
            // Bind the upstream operation identity on the first event, then accept
            // only events carrying that identity. Two overlapping service requests
            // can therefore never cross-attribute: the stranger's events are
            // counted and dropped instead of being folded into this record.
            // 在首个事件上绑定上游操作身份，此后只接受携带该身份的事件。因此两个重叠的
            // 服务请求绝不会互相归属：外来事件只被计数并丢弃，而不会被并入本记录。
            // Compare the full identity through a fixed-size digest. The bounded text is
            // retained only for the user-visible projection and never decides ownership.
            // 通过定长摘要比较完整身份。受界文本仅为面向用户的投影保留，绝不参与所有权判定。
            let incoming_operation_digest = operation_identity_digest(&event.operation_id);
            match operation.upstream_operation_digest {
                None => {
                    operation.upstream_operation_id = Some(bound_field(&event.operation_id));
                    operation.upstream_operation_digest = Some(incoming_operation_digest);
                }
                Some(bound) if bound == incoming_operation_digest => {}
                Some(_) => {
                    state.foreign_events = state.foreign_events.saturating_add(1);
                    return;
                }
            }
            if projected.is_terminal() {
                operation.saw_upstream_terminal = true;
            }
            if operation.events.len() >= MAX_EVENTS_PER_OPERATION {
                operation.overflowed = true;
                // Evict the oldest non-terminal event so a terminal status is
                // never the thing that gets dropped.
                // 淘汰最旧的非终态事件，使终态状态绝不会成为被丢弃的那一条。
                let evict = operation
                    .events
                    .iter()
                    .position(|retained| !retained.is_terminal())
                    .unwrap_or(0);
                operation.events.remove(evict);
            }
            operation.events.push_back(projected);
            // progress_view contains only the bounded, sanitized service contract. It is
            // published after releasing the operation-state lock, so projection never
            // lengthens the callback's critical section.
            // progress_view 只包含受界且已脱敏的服务契约；它在释放操作状态锁后发布，因此投影
            // 不会延长回调临界区。
            let projection_update = LuaSkillsProgressProjectionUpdate {
                operation_ordinal: operation.operation_ordinal,
                progress: summary_from_record(operation),
            };
            drop(state);
            if let Some(sender) = callback_projection_sender.as_ref() {
                sender.send_replace(Some(projection_update));
            }
        });
        Arc::new(Self {
            state: state_slot,
            callback,
            projection_sender,
            projection_task,
            projection_order,
            active_projection_ordinal,
        })
    }

    /// Binds one freshly generated correlation id and starts its record.
    /// 绑定一个新生成的关联标识并开始其记录。
    fn begin(&self, action: SkillLifecycleAction) -> String {
        let ordinal = LUASKILLS_OPERATION_SEQUENCE.fetch_add(1, Ordering::SeqCst);
        let correlation_id = format!("luaskills-op-{ordinal}");
        // Serialize the new active ordinal and its first queued snapshot against a
        // previous operation's terminal transition. A pending older update can then
        // never become current again after this point.
        // 将新的活动序号及其首个排队快照与上一操作的终态转换串行化。此后任何待处理的旧更新
        // 都无法重新成为当前状态。
        let _projection_guard = lock_mutex(&self.projection_order);
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.operations.insert(
            correlation_id.clone(),
            LuaSkillsOperationProgress {
                operation_ordinal: ordinal,
                correlation_id: correlation_id.clone(),
                action,
                in_flight: true,
                events: VecDeque::new(),
                outcome: None,
                overflowed: false,
                saw_upstream_terminal: false,
                upstream_operation_id: None,
                upstream_operation_digest: None,
            },
        );
        state.order.push_back(correlation_id.clone());
        while state.order.len() > MAX_RETAINED_OPERATIONS {
            if let Some(evicted) = state.order.pop_front() {
                state.operations.remove(&evicted);
            }
        }
        state.active = Some(correlation_id.clone());
        self.active_projection_ordinal
            .store(ordinal, Ordering::SeqCst);
        // progress_view announces the in-flight identity before the blocking operation
        // can emit its first upstream phase.
        // progress_view 在阻塞操作发出首个上游阶段前公布进行中身份。
        let projection_update = state.operations.get(&correlation_id).map(|operation| {
            LuaSkillsProgressProjectionUpdate {
                operation_ordinal: ordinal,
                progress: summary_from_record(operation),
            }
        });
        drop(state);
        if let Some(projection_update) = projection_update
            && let Some(sender) = self.projection_sender.as_ref()
        {
            sender.send_replace(Some(projection_update));
        }
        correlation_id
    }

    /// Records the authoritative request outcome and releases the binding.
    /// 记录权威请求结果并释放绑定。
    fn finish(&self, correlation_id: &str, outcome: LuaSkillsOperationOutcome) {
        // Any projection worker that already passed the ordinal check completes before
        // this transition. Workers scheduled later observe the sentinel and drop their
        // stale in-flight value, so the synchronous terminal commit cannot regress.
        // 已通过序号检查的投影工作器会先于本转换完成。之后调度的工作器会观察到哨兵并丢弃
        // 陈旧在途值，因此同步终态提交不会发生回退。
        let _projection_guard = lock_mutex(&self.projection_order);
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if state.active.as_deref() == Some(correlation_id) {
            state.active = None;
            self.active_projection_ordinal
                .store(NO_ACTIVE_PROJECTION_ORDINAL, Ordering::SeqCst);
        }
        if let Some(operation) = state.operations.get_mut(correlation_id) {
            operation.in_flight = false;
            operation.outcome = Some(outcome);
        }
    }

    /// Returns one retained operation projection by correlation id.
    /// 按关联标识返回一条保留的操作投影。
    fn progress(&self, correlation_id: &str) -> Option<LuaSkillsOperationProgress> {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.operations.get(correlation_id).cloned()
    }

    /// Returns the number of events observed with no bound operation.
    /// 返回在无绑定操作时观察到的事件数。
    fn orphaned_events(&self) -> u64 {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.orphaned_events
    }

    /// Returns the number of events whose identity did not match the binding.
    /// 返回身份与绑定不匹配的事件数。
    fn foreign_events(&self) -> u64 {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.foreign_events
    }

    /// Returns the newest retained operation, preferring the currently bound one.
    /// 返回最新的保留操作，优先返回当前绑定的那个。
    ///
    /// A bound operation always wins: once a request starts, its progress is the
    /// only thing a reader may be shown, even if an earlier finished record is newer
    /// in insertion order.
    /// 已绑定操作始终优先：一旦请求开始，它的进度就是读取方唯一应看到的内容，即使某个更早
    /// 完成的记录在插入顺序上更靠后。
    fn latest(&self) -> Option<LuaSkillsOperationProgress> {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        // One guard clause keeps the two lookups on a single level, so the active
        // operation and the recency fallback read as alternatives rather than as a
        // nested condition.
        // 单个守卫子句把两次查找放在同一层，使「当前活动操作」与「最近记录回退」读起来是
        // 并列的两种选择，而不是嵌套条件。
        if let Some(active) = state.active.as_ref()
            && let Some(record) = state.operations.get(active)
        {
            return Some(record.clone());
        }
        state
            .order
            .back()
            .and_then(|correlation_id| state.operations.get(correlation_id))
            .cloned()
    }

    /// Installs the callback into the upstream process-wide slot.
    /// 把回调安装进上游进程级槽。
    fn install_callback(&self) {
        set_skill_operation_progress_callback(Some(Arc::clone(&self.callback)));
    }

    /// Clears the upstream slot only while this owner still owns its exact callback.
    /// 仅当本所有者仍持有精确回调时才清空上游槽。
    fn clear_callback(&self) {
        clear_skill_operation_progress_callback_if(&self.callback);
    }

    /// Stops publication and clears the exact callback owned by this generation.
    /// 停止发布并清除当前代次持有的精确回调。
    ///
    /// The ordering guard drains a synchronous commit already in progress before
    /// invalidating every queued update. Replacement can therefore install a new
    /// owner without an obsolete task overwriting its retained state afterwards.
    /// 顺序守卫先排空已经开始的同步提交，再让所有排队更新失效。因此替换方可以安装新
    /// 所有者，而不会随后被旧任务覆盖其保留状态。
    fn shutdown(&self) {
        let _projection_guard = lock_mutex(&self.projection_order);
        self.active_projection_ordinal
            .store(NO_ACTIVE_PROJECTION_ORDINAL, Ordering::SeqCst);
        self.clear_callback();
        if let Some(task) = self.projection_task.as_ref() {
            task.abort();
        }
    }
}

impl Drop for LuaSkillsInstallProgress {
    /// Stops projection and clears the exact upstream callback owned by this value.
    /// 停止投影并清理本值精确持有的上游回调。
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Process-wide slot holding the single owner of the progress callback.
/// 持有进度回调唯一所有者的进程级槽。
static GLOBAL_PROGRESS: Mutex<Option<Arc<LuaSkillsInstallProgress>>> = Mutex::new(None);

/// Locks the process-wide progress slot, recovering from a poisoned mutex.
/// 锁定进程级进度槽，并在互斥锁中毒时恢复。
fn lock_global_progress() -> std::sync::MutexGuard<'static, Option<Arc<LuaSkillsInstallProgress>>> {
    match GLOBAL_PROGRESS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Returns the stable label for one LuaSkills lifecycle action.
/// 返回某个 LuaSkills 生命周期动作的稳定标签。
pub(crate) fn action_label(action: SkillLifecycleAction) -> &'static str {
    match action {
        SkillLifecycleAction::Install => "install",
        SkillLifecycleAction::Update => "update",
        SkillLifecycleAction::Reload => "reload",
        SkillLifecycleAction::Uninstall => "uninstall",
        SkillLifecycleAction::Enable => "enable",
        SkillLifecycleAction::Disable => "disable",
    }
}

/// Projects one retained operation into the WebUI service API summary.
/// 把一条保留操作投影为 WebUI service API 摘要。
///
/// The summary is the service API wire type so the by-id query, the catalog snapshot
/// and the progress delta all expose exactly one contract instead of three lookalikes.
/// 该摘要是 service API 线类型，因此按标识查询、目录快照与进度增量只暴露一份契约，而不是
/// 三份相似类型。
fn summary_from_record(
    record: &LuaSkillsOperationProgress,
) -> vulcan_service_api::LuaSkillsOperationProgressSummary {
    let latest = record.latest();
    vulcan_service_api::LuaSkillsOperationProgressSummary {
        correlation_id: record.correlation_id.clone(),
        action: String::from(action_label(record.action)),
        in_flight: record.in_flight,
        outcome: record.outcome.map(|outcome| {
            String::from(match outcome {
                LuaSkillsOperationOutcome::Succeeded => "succeeded",
                LuaSkillsOperationOutcome::Failed => "failed",
            })
        }),
        overflowed: record.overflowed,
        saw_upstream_terminal: record.saw_upstream_terminal,
        phases: record.events.len(),
        latest_phase: latest.map(|event| event.phase.clone()),
        latest_status: latest.map(|event| event.status.clone()),
        skill_id: latest.and_then(|event| event.skill_id.clone()),
        root_name: latest.and_then(|event| event.root_name.clone()),
        bytes_done: latest.and_then(|event| event.bytes_done),
        bytes_total: latest.and_then(|event| event.bytes_total),
        // The service API summary carries a whole-number percentage because the whole
        // catalog derives `Eq`, which `f64` cannot satisfy. Clamping also stops a
        // malformed upstream percentage from reaching the WebUI as 300%.
        // service API 摘要使用整数百分比，因为整个目录 derive 了 `Eq`，而 `f64` 无法满足。
        // 夹取同时防止畸形的上游百分比以 300% 到达 WebUI。
        percent: latest
            .and_then(|event| event.percent)
            .map(|percent| percent.round().clamp(0.0, 100.0) as u8),
        message: latest.and_then(|event| event.message.clone()),
        upstream_operation_id: record.upstream_operation_id.clone(),
    }
}

/// Returns one retained operation as a service API summary by correlation id.
/// 按关联标识把一条保留操作作为 service API 摘要返回。
pub(crate) fn global_operation_progress_view(
    correlation_id: &str,
) -> Option<vulcan_service_api::LuaSkillsOperationProgressSummary> {
    global_operation_progress(correlation_id).map(|record| summary_from_record(&record))
}

/// Returns the newest retained operation summary, preferring the bound one.
/// 返回最新的保留操作摘要，优先返回当前绑定的那个。
///
/// The catalog snapshot uses this so a reader sees the operation that is running, or
/// the most recently finished one, without knowing any correlation id.
/// 目录快照使用它，使读取方无需知道任何关联标识就能看到正在运行的操作或最近完成的操作。
pub(crate) fn global_latest_operation_view()
-> Option<vulcan_service_api::LuaSkillsOperationProgressSummary> {
    let guard = lock_global_progress();
    guard
        .as_ref()
        .and_then(|owner| owner.latest())
        .map(|record| summary_from_record(&record))
}

/// Installs the progress callback as the process-wide owner.
/// 以进程级所有者身份安装进度回调。
///
/// Replacing an existing owner drops it first so its cleanup cannot clear the
/// callback this call just installed.
/// 替换既有所有者时先丢弃它，使其清理动作不会清掉本次刚安装的回调。
#[cfg(test)]
pub(crate) fn install_global() {
    let mut guard = lock_global_progress();
    if let Some(previous) = guard.take() {
        previous.shutdown();
    }
    let owner = LuaSkillsInstallProgress::new(
        None,
        None,
        Arc::new(Mutex::new(())),
        Arc::new(AtomicU64::new(NO_ACTIVE_PROJECTION_ORDINAL)),
    );
    owner.install_callback();
    *guard = Some(owner);
}

/// Installs the production progress callback with a non-blocking retained-state projector.
/// 安装带非阻塞保留状态投影器的生产进度回调。
///
/// # Parameters
/// 参数
/// `app` broadcasts changed progress to connected service clients.
/// `app` 向已连接服务客户端广播发生变化的进度。
/// `state` owns the retained workbench snapshot used by reconnects.
/// `state` 持有重连使用的保留工作台快照。
/// `runtime` runs the latest-value projector outside the upstream callback.
/// `runtime` 在上游回调之外运行最新值投影器。
///
/// # Returns
/// 返回值
/// This function returns no value after replacing the process-wide owner.
/// 替换进程级所有者后不返回值。
pub(crate) fn install_global_with_projection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    runtime: tokio::runtime::Handle,
) {
    // A watch channel is a one-value bounded queue: rapid byte progress coalesces to
    // the newest sanitized state instead of blocking the upstream callback or growing memory.
    // watch 通道是一槽有界队列：高频字节进度会合并为最新脱敏状态，既不阻塞上游回调，
    // 也不会无限增长内存。
    let projection_order = Arc::new(Mutex::new(()));
    let active_projection_ordinal = Arc::new(AtomicU64::new(NO_ACTIVE_PROJECTION_ORDINAL));
    let task_projection_order = Arc::clone(&projection_order);
    let task_active_projection_ordinal = Arc::clone(&active_projection_ordinal);
    let (sender, mut receiver) = watch::channel::<Option<LuaSkillsProgressProjectionUpdate>>(None);
    let projection_task = runtime.spawn(async move {
        while receiver.changed().await.is_ok() {
            let Some(update) = receiver.borrow_and_update().clone() else {
                continue;
            };
            // Hold the same ordering guard used by begin/finish until the commit has
            // completed. The ordinal check is therefore atomic with respect to both
            // terminal invalidation and the next operation becoming active.
            // 持有 begin/finish 共用的顺序守卫直到提交结束。因此序号检查相对于终态失效和
            // 下一操作变为活动状态都是原子的。
            let _projection_guard = lock_mutex(&task_projection_order);
            if task_active_projection_ordinal.load(Ordering::SeqCst) != update.operation_ordinal {
                continue;
            }
            if let Err(error) =
                crate::settings_runtime_projection::commit_and_emit_luaskills_operation_progress(
                    &app,
                    &state,
                    Some(update.progress),
                )
            {
                tracing::warn!(
                    "[webui-service] LuaSkills operation progress projection failed: {error}"
                );
            }
        }
    });
    let mut guard = lock_global_progress();
    if let Some(previous) = guard.take() {
        previous.shutdown();
    }
    let owner = LuaSkillsInstallProgress::new(
        Some(sender),
        Some(projection_task),
        projection_order,
        active_projection_ordinal,
    );
    owner.install_callback();
    *guard = Some(owner);
}

/// Clears the process-wide progress owner and its exact upstream callback.
/// 清除进程级进度所有者及其精确上游回调。
///
/// Bootstrap rollback and successful application shutdown both call this boundary so
/// no callback survives the service runtime that owns its operation projections.
/// 启动回滚与应用正常关闭都会调用此边界，确保回调不会超过持有其操作投影的服务运行时寿命。
pub(crate) fn shutdown_global() {
    let owner = {
        let mut guard = lock_global_progress();
        guard.take()
    };
    if let Some(owner) = owner {
        owner.shutdown();
    }
}

/// Binds a new correlation id for one LuaSkills management request.
/// 为一次 LuaSkills 管理请求绑定新的关联标识。
///
/// Call this only while holding the LuaSkills management mutation lock, and
/// release it with [`finish_global_operation`] before dropping that lock.
/// 仅在持有 LuaSkills 管理变更锁时调用，并在释放该锁之前用
/// [`finish_global_operation`] 结束它。
pub(crate) fn begin_global_operation(action: SkillLifecycleAction) -> Option<String> {
    let guard = lock_global_progress();
    guard.as_ref().map(|owner| owner.begin(action))
}

/// Records the authoritative request outcome for one correlation id.
/// 记录某个关联标识的权威请求结果。
///
/// A missing owner or an unknown id is not an error because progress reporting is
/// advisory; read the finalized projection with [`global_operation_progress`].
/// 缺少所有者或未知标识不是错误，因为进度上报是辅助性的；用
/// [`global_operation_progress`] 读取最终投影。
pub(crate) fn finish_global_operation(correlation_id: &str, outcome: LuaSkillsOperationOutcome) {
    let guard = lock_global_progress();
    if let Some(owner) = guard.as_ref() {
        owner.finish(correlation_id, outcome);
    }
}

/// Returns one retained operation projection by correlation id.
/// 按关联标识返回一条保留的操作投影。
pub(crate) fn global_operation_progress(
    correlation_id: &str,
) -> Option<LuaSkillsOperationProgress> {
    let guard = lock_global_progress();
    guard.as_ref()?.progress(correlation_id)
}

/// Returns the number of progress events observed with no bound operation.
/// 返回在无绑定操作时观察到的进度事件数。
pub(crate) fn global_orphaned_event_count() -> u64 {
    let guard = lock_global_progress();
    match guard.as_ref() {
        Some(owner) => owner.orphaned_events(),
        None => 0,
    }
}

/// Returns the number of events whose upstream identity did not match the binding.
/// 返回上游身份与绑定不匹配的事件数。
///
/// A non-zero value proves an event source exists outside the management lock, so
/// it is surfaced as a defect signal rather than hidden.
/// 非零值证明存在管理锁之外的事件来源，因此作为缺陷信号上报而不是被隐藏。
pub(crate) fn global_foreign_event_count() -> u64 {
    let guard = lock_global_progress();
    match guard.as_ref() {
        Some(owner) => owner.foreign_events(),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that mutate the process-wide progress owner.
    /// 串行化会改动进程级进度所有者的测试。
    static PROGRESS_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Locks the progress test lock, recovering from poisoning.
    /// 锁定进度测试锁，并在中毒时恢复。
    fn progress_test_guard() -> std::sync::MutexGuard<'static, ()> {
        match PROGRESS_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Verifies begin and callback updates publish the newest sanitized view without waiting.
    /// 验证开始与回调更新会在不等待消费者的情况下发布最新脱敏视图。
    #[test]
    fn projection_channel_reports_in_flight_identity_and_live_phase() {
        let _guard = progress_test_guard();
        let (sender, mut receiver) =
            watch::channel::<Option<LuaSkillsProgressProjectionUpdate>>(None);
        let owner = LuaSkillsInstallProgress::new(
            Some(sender),
            None,
            Arc::new(Mutex::new(())),
            Arc::new(AtomicU64::new(NO_ACTIVE_PROJECTION_ORDINAL)),
        );
        owner.install_callback();

        let correlation_id = owner.begin(SkillLifecycleAction::Install);
        let started = receiver
            .borrow_and_update()
            .clone()
            .expect("begin must publish one in-flight view")
            .progress;
        assert_eq!(started.correlation_id, correlation_id);
        assert!(started.in_flight);
        assert_eq!(started.phases, 0);

        (owner.callback)(&event(1, "progress", "downloading_archive", None));
        let progressed = receiver
            .borrow_and_update()
            .clone()
            .expect("callback must publish the newest live phase")
            .progress;
        assert_eq!(progressed.correlation_id, correlation_id);
        assert_eq!(progressed.phases, 1);
        assert_eq!(
            progressed.latest_phase.as_deref(),
            Some("downloading_archive")
        );
        assert!(progressed.in_flight);

        owner.clear_callback();
    }

    /// Verifies a queued in-flight snapshot cannot overwrite an already committed terminal state.
    /// 验证排队中的在途快照无法覆盖已经提交的终态。
    #[test]
    #[ignore = "mutates the process-wide LuaSkills progress callback slot"]
    fn stale_in_flight_projection_cannot_regress_terminal_state() {
        let _guard = progress_test_guard();
        shutdown_global();
        let state = Arc::new(Mutex::new(
            crate::tests::test_support::test_service_state_with_parent_session(
                crate::SessionId::new_v4(),
            ),
        ));
        let app = ServiceEventEmitter::new(Vec::new());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("build stale projection test runtime");
        install_global_with_projection(app.clone(), Arc::clone(&state), runtime.handle().clone());

        let correlation_id = begin_global_operation(SkillLifecycleAction::Install)
            .expect("production progress owner should allocate an id");
        let owner = lock_global_progress()
            .as_ref()
            .cloned()
            .expect("production progress owner installed");
        let stale_update = owner
            .projection_sender
            .as_ref()
            .expect("production owner has a projection sender")
            .borrow()
            .clone()
            .expect("begin queued an in-flight update");

        finish_global_operation(&correlation_id, LuaSkillsOperationOutcome::Succeeded);
        let terminal = global_operation_progress(&correlation_id)
            .map(|progress| summary_from_record(&progress))
            .expect("finished operation remains retained");
        crate::settings_runtime_projection::commit_and_emit_luaskills_operation_progress(
            &app,
            &state,
            Some(terminal.clone()),
        )
        .expect("terminal projection should commit");

        owner
            .projection_sender
            .as_ref()
            .expect("production owner has a projection sender")
            .send_replace(Some(stale_update));
        runtime.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        });

        let retained = state
            .lock()
            .expect("service state should remain lockable")
            .workbench
            .lua_skills
            .operation_progress
            .clone()
            .expect("terminal progress remains visible");
        assert_eq!(retained, terminal);
        assert!(!retained.in_flight);

        shutdown_global();
    }

    /// Builds one upstream progress event.
    /// 构造一个上游进度事件。
    fn event(
        sequence: u64,
        status: &str,
        phase: &str,
        message: Option<&str>,
    ) -> RuntimeSkillOperationProgressEvent {
        RuntimeSkillOperationProgressEvent {
            operation_id: String::from("upstream-op"),
            sequence,
            plane: SkillOperationPlane::Skills,
            action: SkillLifecycleAction::Install,
            phase: String::from(phase),
            status: String::from(status),
            skill_id: Some(String::from("fixture-skill")),
            root_name: Some(String::from("user")),
            source_type: None,
            source_locator: Some(String::from("https://user:token@example.invalid/pkg.tgz")),
            bytes_done: Some(10),
            bytes_total: Some(100),
            percent: Some(10.0),
            message: message.map(String::from),
        }
    }

    /// Delivers one event through the installed callback.
    /// 通过已安装的回调投递一个事件。
    fn deliver(event: &RuntimeSkillOperationProgressEvent) {
        let guard = lock_global_progress();
        let owner = guard.as_ref().expect("progress owner installed");
        (owner.callback)(event);
    }

    /// Verifies free-form messages and source locators never reach the projection.
    /// 验证自由文本消息与来源定位值绝不进入投影。
    #[test]
    fn drops_free_form_messages_and_source_locators() {
        // The retained shape has no `source_locator` field at all, so a locator can
        // never reach a projection even if upstream starts sending new ones.
        // 保留形状根本没有 `source_locator` 字段，因此即使上游开始发送新的定位值，也
        // 绝不会进入投影。
        let projected = project_event(&event(
            1,
            "progress",
            "downloading_archive",
            Some(
                "reading C:\\Users\\someone\\private\\pkg.tgz Authorization: SECRET \
                 Bearer ANOTHERSECRET https://host/x ordinary words",
            ),
        ));
        assert!(
            projected.message.is_none(),
            "no byte of an ordinary upstream message may cross the boundary"
        );
    }

    /// Verifies an oversized message cannot allocate an oversized retained value.
    /// 验证超长消息无法分配一个超大的保留值。
    #[test]
    fn drops_oversized_messages_without_copying_them() {
        let long = "phase".repeat(MAX_FIELD_CHARS * 10_000);
        let projected = project_event(&event(1, "progress", "downloading_archive", Some(&long)));
        assert!(projected.message.is_none());
    }

    /// Verifies machine-readable fields are bounded before retention.
    /// 验证机器可读字段在保留之前受界。
    ///
    /// A malformed upstream value must not be retained once per event: these fields live
    /// on every retained event of every retained operation, so an unbounded one is paid
    /// 64 x 32 times over.
    /// 畸形的上游取值不得被每个事件保留一次：这些字段存在于每个被保留操作的每个被保留事件
    /// 上，因此一个无界值会被付出 64 × 32 次代价。
    #[test]
    fn machine_readable_fields_are_bounded() {
        let mut oversized = event(1, "progress", "downloading_archive", None);
        oversized.phase = "p".repeat(MAX_FIELD_CHARS * 100);
        oversized.status = "s".repeat(MAX_FIELD_CHARS * 100);
        oversized.skill_id = Some("i".repeat(MAX_FIELD_CHARS * 100));
        oversized.root_name = Some("r".repeat(MAX_FIELD_CHARS * 100));

        let projected = project_event(&oversized);

        assert_eq!(projected.phase.chars().count(), MAX_FIELD_CHARS);
        assert_eq!(projected.status.chars().count(), MAX_FIELD_CHARS);
        assert_eq!(
            projected.skill_id.as_deref().map(str::len),
            Some(MAX_FIELD_CHARS)
        );
        assert_eq!(
            projected.root_name.as_deref().map(str::len),
            Some(MAX_FIELD_CHARS)
        );
        // A legitimate short value must survive untouched, so the bound cannot be a
        // silent behavioural change for real upstream traffic.
        // 合法的短取值必须原样保留，因此该上界对真实上游流量不构成隐式行为变化。
        let ordinary = project_event(&event(2, "progress", "downloading_archive", None));
        assert_eq!(ordinary.phase, "downloading_archive");
        assert_eq!(ordinary.status, "progress");
        assert_eq!(ordinary.skill_id.as_deref(), Some("fixture-skill"));
        assert_eq!(ordinary.root_name.as_deref(), Some("user"));
    }

    /// Verifies events outside a bound operation are counted, never attributed.
    /// 验证绑定区间之外的事件只被计数，绝不归属。
    #[test]
    fn unbound_events_are_counted_not_attributed() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();
        let before = global_orphaned_event_count();

        deliver(&event(1, "progress", "downloading_archive", None));

        assert_eq!(
            global_orphaned_event_count(),
            before + 1,
            "an unbound event must be counted, because attributing it would be a guess"
        );
    }

    /// Verifies the recorded outcome comes from the request, not the event stream.
    /// 验证记录的结果来自请求，而不是事件流。
    #[test]
    fn outcome_is_the_request_result() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let correlation_id = begin_global_operation(SkillLifecycleAction::Install)
            .expect("correlation id while installed");
        // Upstream reports completion for one intermediate phase. That phase status
        // is not an operation terminal and must not be presented as one.
        // 上游报告一个中间阶段完成。该阶段状态不是操作终态，不得按操作终态呈现。
        deliver(&event(1, "started", "resolving_source", None));
        deliver(&event(2, "completed", "downloading_archive", None));

        let in_flight = global_operation_progress(&correlation_id).expect("record");
        assert!(in_flight.in_flight);
        assert_eq!(
            in_flight.outcome, None,
            "the event stream is not authoritative"
        );
        assert!(!in_flight.saw_upstream_terminal);

        // The request itself failed, so the recorded outcome is Failed even though
        // the download phase completed.
        // 请求本身失败，因此尽管下载阶段完成，记录的结果仍是 Failed。
        finish_global_operation(&correlation_id, LuaSkillsOperationOutcome::Failed);

        let finished = global_operation_progress(&correlation_id).expect("record");
        assert!(!finished.in_flight);
        assert_eq!(finished.outcome, Some(LuaSkillsOperationOutcome::Failed));
        assert_eq!(finished.events.len(), 2);
        assert_eq!(
            finished.latest().expect("latest").phase,
            "downloading_archive"
        );
    }

    /// Verifies releasing the binding stops further attribution.
    /// 验证释放绑定后不再归属后续事件。
    #[test]
    fn finish_releases_the_binding() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let correlation_id = begin_global_operation(SkillLifecycleAction::Update)
            .expect("correlation id while installed");
        deliver(&event(1, "started", "resolving_source", None));
        finish_global_operation(&correlation_id, LuaSkillsOperationOutcome::Succeeded);

        let before = global_orphaned_event_count();
        deliver(&event(2, "progress", "verifying_checksum", None));

        let record = global_operation_progress(&correlation_id).expect("record");
        assert_eq!(
            record.events.len(),
            1,
            "no event may attach after finishing"
        );
        assert_eq!(global_orphaned_event_count(), before + 1);
    }

    /// Verifies overflow evicts progress but keeps the terminal status.
    /// 验证溢出淘汰进度但保留终态。
    #[test]
    fn overflow_keeps_the_terminal_event() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let correlation_id = begin_global_operation(SkillLifecycleAction::Install)
            .expect("correlation id while installed");
        for sequence in 0..(MAX_EVENTS_PER_OPERATION as u64 + 20) {
            deliver(&event(sequence, "progress", "downloading_archive", None));
        }
        deliver(&event(999, "failed", "failed", None));

        let record = global_operation_progress(&correlation_id).expect("record");
        assert!(record.overflowed, "overflow must be diagnosed");
        assert_eq!(record.events.len(), MAX_EVENTS_PER_OPERATION);
        let latest = record.latest().expect("latest");
        assert_eq!(latest.status, "failed", "the terminal status must survive");
        assert!(latest.is_terminal());
    }

    /// Verifies correlation identities are stable, distinct, and sequential.
    /// 验证关联身份稳定、互不相同且递增。
    #[test]
    fn correlation_ids_are_stable_and_distinct() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let first = begin_global_operation(SkillLifecycleAction::Install).expect("first");
        finish_global_operation(&first, LuaSkillsOperationOutcome::Succeeded);
        let second = begin_global_operation(SkillLifecycleAction::Update).expect("second");

        assert_ne!(first, second);
        assert!(first.starts_with("luaskills-op-"));
        assert!(second.starts_with("luaskills-op-"));
        // Each id keeps its own record and action.
        // 每个标识保留自己的记录与动作。
        assert_eq!(
            global_operation_progress(&first)
                .expect("first record")
                .action,
            SkillLifecycleAction::Install
        );
        assert_eq!(
            global_operation_progress(&second)
                .expect("second record")
                .action,
            SkillLifecycleAction::Update
        );
        finish_global_operation(&second, LuaSkillsOperationOutcome::Failed);
        shutdown_global();
    }

    /// Verifies replacing the process-wide owner never reuses an earlier request identity.
    /// 验证替换进程级所有者后绝不会复用较早请求的身份。
    #[test]
    fn correlation_ids_remain_distinct_across_owner_replacement() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();
        // first belongs to the owner that will be replaced before another request begins.
        // first 属于将在下一条请求开始前被替换的所有者。
        let first = begin_global_operation(SkillLifecycleAction::Install).expect("first");
        finish_global_operation(&first, LuaSkillsOperationOutcome::Succeeded);

        shutdown_global();
        install_global();
        // second must identify only the request created by the replacement owner.
        // second 必须仅标识替代所有者创建的请求。
        let second = begin_global_operation(SkillLifecycleAction::Update).expect("second");

        assert_ne!(first, second);
        finish_global_operation(&second, LuaSkillsOperationOutcome::Succeeded);
        shutdown_global();
    }

    /// Verifies finished-operation retention is bounded.
    /// 验证已完成操作的保留量有界。
    #[test]
    fn retention_is_bounded() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let mut oldest = String::new();
        for index in 0..(MAX_RETAINED_OPERATIONS + 5) {
            let correlation_id =
                begin_global_operation(SkillLifecycleAction::Install).expect("correlation id");
            if index == 0 {
                oldest = correlation_id.clone();
            }
            finish_global_operation(&correlation_id, LuaSkillsOperationOutcome::Succeeded);
        }

        assert!(
            global_operation_progress(&oldest).is_none(),
            "the oldest finished operation must be evicted"
        );
        shutdown_global();
    }

    /// Builds one upstream progress event for an explicit upstream operation.
    /// 为指定上游操作构造一个上游进度事件。
    fn event_for_operation(
        operation_id: &str,
        status: &str,
        phase: &str,
    ) -> RuntimeSkillOperationProgressEvent {
        let mut value = event(1, status, phase, None);
        value.operation_id = String::from(operation_id);
        value
    }

    /// Verifies concurrent readers and one writer keep every invariant.
    /// 验证并发读方与单个写方保持全部不变量。
    ///
    /// Production delivers events from the LuaSkills callback while the by-id RPC handler
    /// and the catalog build read the same process-wide state, so readers and the writer
    /// genuinely overlap. Every other test here is single-threaded, which cannot show a
    /// torn read, a lost attribution, or a lock-order deadlock.
    /// 生产中由 LuaSkills 回调投递事件，而按 ID 查询的 RPC 处理器与目录构建同时读取同一份
    /// 进程级状态，因此读方与写方确实重叠。此处的其余测试都是单线程的，无法暴露撕裂读、
    /// 归属丢失或锁序死锁。
    ///
    /// A deadlock surfaces as this test never returning, which is the honest signal: no
    /// assertion can report a hang from inside the hung process.
    /// 死锁表现为本测试永不返回，这是诚实的信号：挂起进程内部没有任何断言能报告挂起。
    #[test]
    fn concurrent_readers_and_writer_keep_every_invariant() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let correlation_id = begin_global_operation(SkillLifecycleAction::Install)
            .expect("correlation id while installed");

        // Four writers of sixteen events each reach the per-operation cap exactly, so the
        // final count is deterministic regardless of interleaving.
        // 四个写方各投递十六个事件，恰好达到单操作上限，因此最终计数与交错顺序无关。
        const WRITERS: usize = 4;
        const PER_WRITER: usize = 16;
        const READERS: usize = 4;
        const READS_PER_READER: usize = 250;

        // A barrier makes every thread start inside the same window, so the readers are
        // guaranteed to overlap the writer rather than merely being spawned alongside it.
        // 屏障让每个线程在同一窗口内起跑，因此读方必然与写方重叠，而不只是恰好被同时创建。
        let start = std::sync::Barrier::new(WRITERS + READERS);
        // The barrier and the per-writer batches are borrowed rather than moved, so each
        // worker owns nothing it would have to hand over.
        // 屏障与各写方的批次都以借用方式传入而非移动，因此每个工作线程都不需要移交任何所有权。
        let start = &start;
        let batches: Vec<Vec<String>> = (0..WRITERS)
            .map(|writer| {
                (0..PER_WRITER)
                    .map(|index| format!("phase-{writer}-{index}"))
                    .collect()
            })
            .collect();
        let correlation_id = &correlation_id;

        std::thread::scope(|scope| {
            for batch in &batches {
                scope.spawn(move || {
                    start.wait();
                    for phase in batch {
                        deliver(&event_for_operation("op-shared", "progress", phase));
                    }
                });
            }
            for _ in 0..READERS {
                scope.spawn(move || {
                    start.wait();
                    for _ in 0..READS_PER_READER {
                        // A reader may only ever observe the bound operation, a count
                        // within the cap, and a percentage that stayed clamped.
                        // 读方只可能观察到已绑定操作、上限之内的计数，以及仍被夹取的百分比。
                        if let Some(view) = global_latest_operation_view() {
                            assert_eq!(view.correlation_id.as_str(), correlation_id.as_str());
                            assert!(
                                view.phases <= MAX_EVENTS_PER_OPERATION,
                                "a retained operation exceeded its event cap"
                            );
                            assert!(
                                view.percent.map_or(true, |percent| percent <= 100),
                                "a percentage escaped its clamp"
                            );
                        }
                        let _ = global_operation_progress_view(&correlation_id);
                    }
                });
            }
        });

        // Every event carried the bound operation's identity, so contention must not turn
        // any of them into a foreign event or drop them as orphaned.
        // 每个事件都携带已绑定操作的身份，因此争用不得把其中任何一个变成外来事件或按无主丢弃。
        assert_eq!(
            global_foreign_event_count(),
            0,
            "contention must not misattribute a bound operation's events"
        );
        assert_eq!(
            global_orphaned_event_count(),
            0,
            "a bound operation drops none"
        );
        let view = global_latest_operation_view().expect("retained operation");
        assert_eq!(view.correlation_id.as_str(), correlation_id.as_str());
        assert_eq!(
            view.phases,
            MAX_EVENTS_PER_OPERATION,
            "all {} events must be retained up to the cap",
            WRITERS * PER_WRITER
        );
    }

    /// Verifies an oversized upstream operation id is bounded without becoming foreign.
    /// 验证超长的上游操作标识受界，且不会因此变成外来事件。
    ///
    /// The retained text is bounded while ownership compares a digest of the complete id.
    /// Two events from the same long id therefore remain attributable without retaining an
    /// unbounded duplicate string.
    /// 保留文本受长度限制，而所有权通过完整标识的摘要比较。因此来自同一个长标识的两条事件仍可
    /// 精确归属，同时不保留无界字符串副本。
    #[test]
    fn oversized_operation_id_is_bounded_without_becoming_foreign() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        begin_global_operation(SkillLifecycleAction::Install).expect("correlation id");
        let foreign_before = global_foreign_event_count();
        let oversized = "op-".to_string() + &"z".repeat(MAX_FIELD_CHARS * 20);

        deliver(&event_for_operation(
            &oversized,
            "started",
            "resolving_source",
        ));
        deliver(&event_for_operation(
            &oversized,
            "progress",
            "downloading_archive",
        ));

        assert_eq!(
            global_foreign_event_count(),
            foreign_before,
            "the same operation's later events must not be counted as foreign"
        );
        let view = global_latest_operation_view().expect("retained operation");
        assert_eq!(
            view.phases, 2,
            "both events must be attributed to the record"
        );
        assert_eq!(
            view.upstream_operation_id.as_deref().map(str::len),
            Some(MAX_FIELD_CHARS),
            "the exposed identifier must be bounded"
        );
    }

    /// Verifies two oversized ids with the same visible prefix remain distinct owners.
    /// 验证两个具有相同可见前缀的超长标识仍属于不同所有者。
    #[test]
    fn oversized_operation_ids_do_not_collide_after_projection() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let correlation_id = begin_global_operation(SkillLifecycleAction::Install)
            .expect("correlation id while installed");
        let common = "z".repeat(MAX_FIELD_CHARS);
        let ours = format!("{common}-ours");
        let theirs = format!("{common}-theirs");
        let foreign_before = global_foreign_event_count();

        deliver(&event_for_operation(&ours, "started", "resolving_source"));
        deliver(&event_for_operation(
            &theirs,
            "progress",
            "downloading_archive",
        ));

        let record = global_operation_progress(&correlation_id).expect("record");
        assert_eq!(
            record.events.len(),
            1,
            "the colliding prefix is not identity"
        );
        assert_eq!(global_foreign_event_count(), foreign_before + 1);
        shutdown_global();
    }

    /// Verifies one record never absorbs another operation's events.
    /// 验证一条记录绝不吸收另一操作的事件。
    ///
    /// The first observed event binds the record to an upstream operation id, so a
    /// later stranger is counted and dropped rather than folded in. Combined with
    /// the management lock — which keeps at most one operation in flight — this is
    /// what makes attribution exact instead of inferred.
    /// 首个观察到的事件把记录绑定到某个上游操作标识，因此之后的外来事件只被计数并丢弃，
    /// 而不会被并入。与管理锁（至多一个操作在途）结合，这就是归属精确而非推断的原因。
    #[test]
    fn binding_rejects_foreign_operation_events() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let correlation_id = begin_global_operation(SkillLifecycleAction::Install)
            .expect("correlation id while installed");
        let foreign_before = global_foreign_event_count();

        deliver(&event_for_operation(
            "op-ours",
            "started",
            "resolving_source",
        ));
        deliver(&event_for_operation(
            "op-theirs",
            "progress",
            "downloading_archive",
        ));
        deliver(&event_for_operation(
            "op-ours",
            "completed",
            "verifying_checksum",
        ));

        let record = global_operation_progress(&correlation_id).expect("record");
        assert_eq!(
            record.events.len(),
            2,
            "the foreign event must not be retained"
        );
        assert_eq!(
            record.upstream_operation_id.as_deref(),
            Some("op-ours"),
            "the first event binds the upstream identity"
        );
        assert!(
            record
                .events
                .iter()
                .all(|retained| retained.phase != "downloading_archive"),
            "no foreign phase may appear in the record"
        );
        assert_eq!(
            global_foreign_event_count(),
            foreign_before + 1,
            "a foreign event is a defect signal and must be counted"
        );

        // The view exposes the binding so a caller can detect the anomaly.
        // 视图暴露该绑定，使调用方能够察觉异常。
        let view = global_operation_progress_view(&correlation_id).expect("view");
        assert_eq!(view.upstream_operation_id.as_deref(), Some("op-ours"));
        assert_eq!(view.phases, 2);
        finish_global_operation(&correlation_id, LuaSkillsOperationOutcome::Succeeded);
        shutdown_global();
    }

    /// Verifies the by-id view is exactly what a reconnect needs.
    /// 验证按标识视图正是重连所需的内容。
    #[test]
    fn view_carries_live_phase_and_authoritative_outcome() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let correlation_id = begin_global_operation(SkillLifecycleAction::Install)
            .expect("correlation id while installed");
        deliver(&event(
            1,
            "progress",
            "downloading_archive",
            Some("saved to C:\\Users\\me\\cache"),
        ));

        let in_flight = global_operation_progress_view(&correlation_id).expect("view");
        assert_eq!(in_flight.correlation_id, correlation_id);
        assert_eq!(in_flight.action, "install");
        assert!(in_flight.in_flight);
        assert_eq!(in_flight.outcome, None);
        assert_eq!(
            in_flight.latest_phase.as_deref(),
            Some("downloading_archive")
        );
        assert_eq!(in_flight.latest_status.as_deref(), Some("progress"));
        assert_eq!(in_flight.phases, 1);
        assert_eq!(in_flight.bytes_done, Some(10));
        assert_eq!(in_flight.bytes_total, Some(100));
        // Ordinary upstream free text is absent from the view, so no local
        // directory or unknown secret format can leak.
        // 普通上游自由文本不会进入视图，因此本地目录与未知格式密钥均无法泄露。
        assert!(in_flight.message.is_none());

        finish_global_operation(&correlation_id, LuaSkillsOperationOutcome::Succeeded);
        let finished = global_operation_progress_view(&correlation_id).expect("view");
        assert!(!finished.in_flight);
        assert_eq!(finished.outcome.as_deref(), Some("succeeded"));
        shutdown_global();
    }

    /// Verifies an unknown correlation id is an explicit absence, not a guess.
    /// 验证未知关联标识是显式缺失，而不是猜测。
    #[test]
    fn view_is_absent_for_an_unknown_id() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        assert!(global_operation_progress_view("luaskills-op-does-not-exist").is_none());
        let error = crate::skills_services::luaskill_operation_progress("luaskills-op-missing")
            .expect_err("an unknown id must not fabricate progress");
        assert!(error.contains("luaskills-op-missing"), "{error}");
        shutdown_global();
    }

    /// Verifies the serialized view uses the camelCase WebUI contract.
    /// 验证序列化视图使用 camelCase 的 WebUI 契约。
    #[test]
    fn view_serializes_with_the_webui_contract() {
        let _guard = progress_test_guard();
        shutdown_global();
        install_global();

        let correlation_id = begin_global_operation(SkillLifecycleAction::Update)
            .expect("correlation id while installed");
        deliver(&event(1, "started", "resolving_source", None));
        finish_global_operation(&correlation_id, LuaSkillsOperationOutcome::Failed);

        let view = global_operation_progress_view(&correlation_id).expect("view");
        let json = serde_json::to_value(&view).expect("serialize view");
        let object = json.as_object().expect("view serializes to an object");
        // The exact key set is asserted, not merely presence: an accidentally added
        // field is how a source locator or local directory would leak across the
        // service API.
        // 断言的是精确键集，而不仅是存在性：意外新增字段正是来源定位值或本地目录跨越服务
        // API 泄露的途径。
        let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "action",
                "bytesDone",
                "bytesTotal",
                "correlationId",
                "inFlight",
                "latestPhase",
                "latestStatus",
                "message",
                "outcome",
                "overflowed",
                "percent",
                "phases",
                "rootName",
                "sawUpstreamTerminal",
                "skillId",
                "upstreamOperationId",
            ],
            "the view contract must stay exact"
        );
        assert_eq!(json["action"], "update");
        assert_eq!(json["outcome"], "failed");
        shutdown_global();
    }

    /// Verifies failure and blocked messages use only host-owned fixed text.
    /// 验证失败与阻断消息只使用宿主自有的固定文本。
    ///
    /// Secrets can appear in arbitrary positions and formats, so the assertion checks
    /// exact output rather than sampling a growing list of credential patterns.
    /// 密钥可能以任意位置与格式出现，因此此处断言精确输出，而不是抽样一份不断增长的凭据模式表。
    #[test]
    fn failure_messages_use_a_finite_host_vocabulary() {
        let secret = "Authorization: raw-secret Bearer second-secret C:\\private\\file";
        let failed = project_event(&event(1, "failed", "verifying_archive", Some(secret)));
        let blocked = project_event(&event(2, "blocked", "resolving_source", Some(secret)));

        assert_eq!(
            failed.message.as_deref(),
            Some("LuaSkills operation failed. Review the request error for recovery details.")
        );
        assert_eq!(
            blocked.message.as_deref(),
            Some("LuaSkills operation was blocked. Review the request error for recovery details.")
        );
        for projected in [failed, blocked] {
            let message = projected.message.expect("controlled message");
            assert!(!message.contains("raw-secret"), "{message}");
            assert!(!message.contains("second-secret"), "{message}");
            assert!(!message.contains("private"), "{message}");
        }
    }

    /// Verifies the cache-hit projection is fixed and absent without an upstream message.
    /// 验证缓存命中投影固定，且上游没有消息时不会凭空生成。
    #[test]
    fn cache_hit_message_requires_presence_but_never_copies_input() {
        let cached = project_event(&event(
            1,
            "cached",
            "downloading_archive",
            Some("cache hit at https://secret.invalid/token"),
        ));
        assert_eq!(
            cached.message.as_deref(),
            Some("LuaSkills archive resolved from the local cache.")
        );
        let absent = project_event(&event(2, "cached", "downloading_archive", None));
        assert!(absent.message.is_none());
    }
}
