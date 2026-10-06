//! One service-owned native observation window with bounded nonblocking callback handoff.
//! 单次服务拥有的原生观测窗口，回调采用有界非阻塞交接。
//! Only Core JSON with luaskills_embedded_diagnostic equal to 1 is selected; ordinary logs are ignored.
//! 仅选择 luaskills_embedded_diagnostic 等于 1 的 Core JSON；普通日志直接忽略。

use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};
use std::time::{Duration, Instant};
use vulcan_service_kernel::luaskills_diagnostics::{
    RuntimeLogCallback, RuntimeLogEvent, RuntimeLogLevel, VulcanPluginNativeIdentityCallback,
    VulcanPluginNativeIdentityEvent, VulcanPluginNativeIdentitySubscription, emit_runtime_log,
    install_native_identity_observer, set_log_callback,
};

/// Sole maximum record footprint for both callback families, including JSON escaping overhead.
/// 两类回调唯一的记录占用上限，包含 JSON 转义开销。
const RECORD_BYTES: usize = 16 * 1024;
/// Sole handoff capacity; a stalled disk cannot retain more metadata records than this bound.
/// 唯一交接容量；磁盘停顿不能保留超过此上限的元数据记录。
const QUEUE_RECORDS: usize = 256;
/// One deadline bounds callback snapshot settlement and consumer drain together.
/// 单一截止时间共同限制回调快照退出与消费者排空。
const FINISH_TIMEOUT: Duration = Duration::from_secs(5);
/// Consumer checkpoint interval also bounds visibility of losses when native output is quiet.
/// 消费者检查点间隔也限制原生输出安静时丢失的可见延迟。
const CHECKPOINT_INTERVAL: Duration = Duration::from_millis(250);
/// Current HOST has exactly one Core setter owner; live replacement is rejected locally.
/// 当前 HOST 只有一个 Core setter 所有者；本地拒绝活动替换。
static CAPTURE_OWNED: AtomicBool = AtomicBool::new(false);
/// Private exact-string probe verifies Core's void setter without inventing a native business event.
/// 私有精确字符串探针验证 Core 的无返回 setter，不虚构原生业务事件。
const INSTALL_PROBE: &str = "vulcan_host_native_diagnostic_callback_install_probe_v1";

/// Loss counters are exchanged at consumer checkpoints, never presented as lifetime totals.
/// 丢失计数在消费者检查点交换，绝不作为生命周期总数展示。
#[derive(Default)]
struct Losses {
    /// Queue full; the callback discarded a bounded record without waiting.
    /// 队列已满；回调不等待地丢弃有界记录。
    full: AtomicU64,
    /// Receiver disconnected before a callback handed off its metadata.
    /// 回调交接元数据前接收端已断开。
    disconnected: AtomicU64,
    /// Metadata exceeds the sole byte bound and was never copied into the queue.
    /// 元数据超过唯一字节上限，未复制入队。
    oversize: AtomicU64,
    /// Missing original authority or observer failure within the checkpoint interval.
    /// 检查点区间内原权威缺失或观察器失败。
    observer_missing: AtomicU64,
    /// Private cumulative baseline only derives checkpoint deltas; it is never emitted as a total.
    /// 私有累计基线仅派生检查点差值；绝不作为总数输出。
    observer_seen: AtomicU64,
    /// Conservative capture latch; any loss permanently prevents a clean terminal receipt.
    /// 保守采集标记；任意丢失永久禁止干净终态回执。
    incomplete: AtomicBool,
    /// True only after the real owner confirms every native shutdown receipt.
    /// 仅真实所有者确认全部原生关闭回执后为真。
    native_closed: AtomicBool,
}

impl Losses {
    /// Increment `counter` and mark the window incomplete without logging, allocation, or IO.
    /// 增加 `counter` 并标记窗口不完整，不记录日志、不分配、不执行 IO。
    fn lose(&self, counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
        self.incomplete.store(true, Ordering::Release);
    }

    /// Exchange pending producer counts into one bounded consumer checkpoint document.
    /// 将待处理生产者计数交换到一个有界消费者检查点文档。
    fn checkpoint(&self) -> Value {
        json!({
            "full": self.full.swap(0, Ordering::AcqRel),
            "disconnected": self.disconnected.swap(0, Ordering::AcqRel),
            "oversize": self.oversize.swap(0, Ordering::AcqRel),
            "observer_missing": self.observer_missing.swap(0, Ordering::AcqRel),
        })
    }

    /// Convert the observer's cumulative `total` into one deduplicated checkpoint delta.
    /// 将观察器累计 `total` 转为去重的检查点差值。
    fn observe_failures(&self, total: u64) {
        // The consumer and finish boundary can race safely without counting the same failure twice.
        // 消费者及结束边界可安全竞态，不会重复计入同一次失败。
        let previous = self.observer_seen.fetch_max(total, Ordering::AcqRel);
        if total > previous {
            self.observer_missing
                .fetch_add(total - previous, Ordering::Relaxed);
            self.incomplete.store(true, Ordering::Release);
        }
    }
}

/// The queue owns only bounded strings or safe typed metadata, never native owner handles.
/// 队列仅拥有有界字符串或安全类型元数据，绝不拥有原生所有者句柄。
enum Observation {
    /// Unparsed Core log text is private to the consumer and is never printed directly.
    /// 未解析 Core 日志文本仅消费者可见，绝不直接输出。
    Core(String),
    /// Already-whitelisted immutable host identity copied before forget.
    /// 遗忘前已复制且已列白名单的不可变宿主身份。
    Identity(Box<VulcanPluginNativeIdentityEvent>),
}

/// Nonblocking handoff shared by both immutable callback identities.
/// 两个不可变回调身份共享的非阻塞交接器。
#[derive(Clone)]
struct Ingress {
    /// Bounded standard-library handoff; send is exclusively try_send.
    /// 标准库有界交接；发送仅使用 try_send。
    sender: SyncSender<Observation>,
    /// Atomic loss accounting cannot wait on the log writer.
    /// 原子丢失计数不会等待日志写入器。
    losses: Arc<Losses>,
}

impl Ingress {
    /// Copy bounded `event.message`; return immediately when too large, full, or disconnected.
    /// 复制有界 `event.message`；过大、已满或断开时立即返回。
    fn core(&self, event: &RuntimeLogEvent) {
        if event
            .message
            .len()
            .saturating_add(std::mem::size_of::<Observation>())
            > RECORD_BYTES
        {
            self.losses.lose(&self.losses.oversize);
            return;
        }
        self.send(Observation::Core(event.message.clone()));
    }

    /// Move the owned safe `event` into the queue after bounding its worst-case JSON footprint.
    /// 限制安全 `event` 最坏 JSON 占用后，将其所有权移入队列。
    fn identity(&self, event: VulcanPluginNativeIdentityEvent) {
        if identity_bytes(&event) > RECORD_BYTES {
            self.losses.lose(&self.losses.oversize);
            return;
        }
        if matches!(
            &event,
            VulcanPluginNativeIdentityEvent::ObservationUnavailable { .. }
        ) {
            // The observer's own ledger counts this once, including failures without emitted events.
            // 观察器自身账本仅计数一次，也包含没有发出事件的失败。
            self.losses.incomplete.store(true, Ordering::Release);
        }
        self.send(Observation::Identity(Box::new(event)));
    }

    /// Try to deliver `observation` without waiting; classify every rejected handoff.
    /// 尝试无等待交付 `observation`；分类所有被拒绝的交接。
    fn send(&self, observation: Observation) {
        match self.sender.try_send(observation) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.losses.lose(&self.losses.full),
            Err(TrySendError::Disconnected(_)) => self.losses.lose(&self.losses.disconnected),
        }
    }
}

/// Return a conservative escaped JSON byte bound for the exact public identity schema.
/// 返回精确公开身份 Schema 的保守转义 JSON 字节上限。
fn identity_bytes(event: &VulcanPluginNativeIdentityEvent) -> usize {
    // Every JSON string byte needs at most six bytes of escaping; fixed syntax covers all field names.
    // 每个 JSON 字符串字节最多需要六个转义字节；固定语法覆盖全部字段名。
    let strings = match event {
        VulcanPluginNativeIdentityEvent::PoolRegistered {
            runtime_id,
            pool_id,
            plugin_id,
            package_generation,
            execution_revision,
            ..
        } => vec![
            runtime_id.as_str(),
            pool_id,
            plugin_id,
            package_generation,
            execution_revision,
        ],
        VulcanPluginNativeIdentityEvent::OperationBound {
            runtime_id,
            pool_id,
            operation_id,
            plugin_id,
            package_generation,
            execution_revision,
            core_session_id,
            controller_session_id,
            host_invocation_id,
            host_session_id,
            trusted_request_id,
            finalization_instance_id,
            ..
        } => {
            // Optional IDs are explicitly optional in the public frozen-authority schema.
            // 可选身份在公开冻结权威 Schema 中明确规定为可选。
            let mut strings = vec![
                runtime_id.as_str(),
                pool_id,
                operation_id,
                plugin_id,
                package_generation,
                execution_revision,
            ];
            for id in [
                core_session_id,
                controller_session_id,
                host_invocation_id,
                host_session_id,
                trusted_request_id,
                finalization_instance_id,
            ]
            .into_iter()
            .flatten()
            {
                strings.push(id);
            }
            strings
        }
        VulcanPluginNativeIdentityEvent::ObservationUnavailable {
            runtime_id,
            operation_id,
            ..
        } => vec![runtime_id.as_str(), operation_id],
    };
    strings.into_iter().fold(1024usize, |bytes, value| {
        bytes.saturating_add(value.len().saturating_mul(6))
    })
}

/// Service owner preserves the exact Core Arc across every plugin and parent generation.
/// 服务所有者跨全部插件和父级代次保留精确 Core Arc。
pub(crate) struct NativeDiagnosticCapture {
    /// Mutex is used only at startup/shutdown boundaries, never from either callback.
    /// 互斥锁仅用于启动和关闭边界，两类回调均不使用它。
    owner: Mutex<Option<CaptureOwner>>,
    /// Store the first terminal result so repeated shutdown cannot fabricate completion.
    /// 保存首次终态结果，避免重复关闭虚构完成。
    result: Mutex<Option<Result<(), String>>>,
}

/// Tail ownership includes the original callbacks and one explicit consumer acknowledgement.
/// 尾部所有权包含原回调及一个明确的消费者确认。
struct CaptureOwner {
    /// Core strong snapshots are counted before the queue is closed.
    /// 队列关闭前计数 Core 强快照。
    core: RuntimeLogCallback,
    /// Exact detachable observer; its outstanding Sink snapshots are independently counted.
    /// 可精确撤销的观察器；其尚未退出的 Sink 快照独立计数。
    identity: Arc<VulcanPluginNativeIdentitySubscription>,
    /// Final sender closes the consumer only after both callback snapshot families settle.
    /// 最终发送器仅在两类回调快照退出后关闭消费者。
    ingress: Ingress,
    /// Consumer's actual write/flush result, never a tracing emission acknowledgement.
    /// 消费者的真实写入和刷新结果，绝不是 tracing 发送确认。
    drained: Receiver<Result<(), String>>,
}

impl NativeDiagnosticCapture {
    /// Install after the service lock and logger, before any native admission; reject a live owner.
    /// 在服务锁及日志器之后、任何原生准入之前安装；拒绝活动所有者。
    /// Return one immutable lifetime owner or an explicit startup error without replacing callbacks.
    /// 返回一个不可变寿命所有者或明确启动错误，不替换回调。
    pub(crate) fn install() -> Result<Self, String> {
        // Read the shared sink before acquiring callback ownership; no second log is ever opened.
        // 获得回调所有权前读取共享接收器；绝不打开第二份日志。
        let writer = crate::logging::shared_writer()?;
        if CAPTURE_OWNED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("Native diagnostic capture is already installed".into());
        }
        // Both callbacks share the sole queue bound and loss ledger.
        // 两类回调共享唯一队列上限及丢失账本。
        let (sender, receiver) = mpsc::sync_channel(QUEUE_RECORDS);
        // The shared bounded metadata handoff.
        // 共享有界元数据交接器。
        let ingress = Ingress {
            sender,
            losses: Arc::new(Losses::default()),
        };
        // Owned handoff for the identity callback.
        // 身份回调拥有的交接器。
        let identity_ingress = ingress.clone();
        // The immutable identity callback installed for this service.
        // 为本服务安装的不可变身份回调。
        let callback: VulcanPluginNativeIdentityCallback =
            Arc::new(move |event| identity_ingress.identity(event));
        // The exact original identity subscription.
        // 精确原身份订阅。
        let identity = match install_native_identity_observer(callback) {
            Ok(identity) => Arc::new(identity),
            Err(error) => {
                CAPTURE_OWNED.store(false, Ordering::Release);
                return Err(error.to_string());
            }
        };
        // Owned handoff for the original Core callback.
        // 原 Core 回调拥有的交接器。
        let core_ingress = ingress.clone();
        // The private probe has no business metadata and is never admitted into the queue or file.
        // 私有探针没有业务元数据，绝不进入队列或文件。
        let probe_seen = Arc::new(AtomicBool::new(false));
        // Shared installation acknowledgement visible to the callback.
        // 回调可见的共享安装确认。
        let callback_probe = Arc::clone(&probe_seen);
        // The exact original Core callback Arc.
        // 精确原 Core 回调 Arc。
        let core: RuntimeLogCallback = Arc::new(move |event| {
            if event.message == INSTALL_PROBE {
                callback_probe.store(true, Ordering::Release);
            } else {
                core_ingress.core(event);
            }
        });
        let (acknowledge, drained) = mpsc::sync_channel(1);
        // Shared capture loss accounting without native owners.
        // 不含原生所有者的共享采集丢失账本。
        let losses = Arc::clone(&ingress.losses);
        // A weak observer permits checkpoint deltas without retaining its sender or forming a cycle.
        // 弱观察器允许检查点差值，不保留其发送器，也不形成引用环。
        let observer = Arc::downgrade(&identity);
        if let Err(error) = std::thread::Builder::new()
            .name("native-diagnostic-consumer".into())
            .spawn(move || {
                // The consumer owns parsing and actual IO; callbacks never recurse into tracing or runtime.
                // 消费者拥有解析及真实 IO；回调绝不重入 tracing 或运行时。
                let result = consume(receiver, writer, losses, observer, native_enabled);
                let _ = acknowledge.try_send(result);
            })
        {
            identity.detach();
            CAPTURE_OWNED.store(false, Ordering::Release);
            return Err(format!(
                "Native diagnostic consumer startup failed: {error}"
            ));
        }
        // Current HOST source has no other Core setter. Core offers no getter or CAS; this owner is unique.
        // 当前 HOST 源码不存在其他 Core setter。Core 不提供 getter 或 CAS；此所有者唯一。
        set_log_callback(Some(Arc::clone(&core)));
        emit_runtime_log(RuntimeLogLevel::Info, INSTALL_PROBE);
        // The service lifetime owner of both subscriptions.
        // 两个订阅的服务寿命所有者。
        let capture = Self {
            owner: Mutex::new(Some(CaptureOwner {
                core,
                identity,
                ingress,
                drained,
            })),
            result: Mutex::new(None),
        };
        if !probe_seen.load(Ordering::Acquire) {
            let _ = capture.finish(false);
            return Err(
                "Core native diagnostic callback installation probe failed; capture is incomplete"
                    .into(),
            );
        }
        Ok(capture)
    }

    /// Finish only after the actual native shutdown succeeds; return drain/IO/incompleteness failures.
    /// 仅真实原生关闭成功后结束；返回排空、IO 或不完整失败。
    pub(crate) fn finish_after_native_shutdown(&self) -> Result<(), String> {
        self.finish(true)
    }

    /// Detach both original subscriptions, settle already-cloned snapshots and bound the final drain.
    /// 撤销两个原订阅，等待已克隆快照退出，并限制最终排空等待。
    fn finish(&self, native_closed: bool) -> Result<(), String> {
        // Serialize boundary calls so an already-taken owner cannot be mistaken for a clean finish.
        // 串行化边界调用，避免把已取出的所有者误当作干净结束。
        let mut result = self
            .result
            .lock()
            .map_err(|_| "Native capture result lock is poisoned")?;
        if let Some(result) = result.as_ref() {
            return result.clone();
        }
        // The original subscription owner taken only at the finish boundary.
        // 仅结束边界取出的原订阅所有者。
        let owner = self
            .owner
            .lock()
            .map_err(|_| "Native capture owner lock is poisoned")?
            .take()
            .ok_or_else(|| {
                "Native capture owner is absent without a terminal receipt".to_owned()
            })?;
        // The common bounded deadline for snapshot settlement and drain.
        // 快照退出及排空共用的有界截止时间。
        let deadline = Instant::now() + FINISH_TIMEOUT;
        owner
            .ingress
            .losses
            .native_closed
            .store(native_closed, Ordering::Release);
        if !native_closed {
            owner
                .ingress
                .losses
                .incomplete
                .store(true, Ordering::Release);
        }
        set_log_callback(None);
        owner.identity.detach();
        // Counting entered callbacks misses a snapshot paused before entry. The original Arc and Sink
        // counts include those snapshots after detachment; neither closure captures its own Arc.
        // 仅计数已进入回调会遗漏入口前暂停快照。撤销后的原 Arc 及 Sink 计数覆盖这些快照；闭包不捕获自身 Arc。
        while Arc::strong_count(&owner.core) != 1 || owner.identity.pending_observations() != 0 {
            if Instant::now() >= deadline {
                owner
                    .ingress
                    .losses
                    .incomplete
                    .store(true, Ordering::Release);
                *result = Some(Err("Native diagnostic callback snapshots exceeded the finish deadline; tail is incomplete".into()));
                // Detached snapshots still own senders; the consumer remains alive until they actually exit.
                // 撤销的快照仍拥有发送器；消费者保持存活直至快照实际退出。
                drop(owner);
                return result.as_ref().expect("terminal result stored").clone();
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        // Capture observer failures only after all original observations settled; never query native owners.
        // 全部原观测退出后才读取观察器失败；绝不查询原生所有者。
        let failures = owner.identity.observation_failures();
        owner.ingress.losses.observe_failures(failures);
        let CaptureOwner {
            core,
            identity,
            ingress,
            drained,
        } = owner;
        drop(core);
        drop(identity);
        drop(ingress);
        // No thread join can block indefinitely. The receipt is sent only after real file flush returns.
        // 不允许线程 join 无限阻塞。仅真实文件刷新返回后发送回执。
        let completed = drained
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|_| {
                Err(
                    "Native diagnostic consumer did not acknowledge drain; tail is incomplete"
                        .into(),
                )
            });
        CAPTURE_OWNED.store(false, Ordering::Release);
        *result = Some(completed.clone());
        completed
    }
}

impl Drop for NativeDiagnosticCapture {
    /// Startup failure or uncoordinated disposal explicitly ends an incomplete window.
    /// 启动失败或未经协调的释放明确结束一个不完整窗口。
    fn drop(&mut self) {
        let _ = self.finish(false);
    }
}

/// Return the existing tracing target decision; all native writes obey the same RUST_LOG policy.
/// 返回既有 tracing 目标过滤决定；全部原生写入遵守同一 RUST_LOG 策略。
fn native_enabled() -> bool {
    tracing::enabled!(target: "vulcan_code_service::native_diagnostics", tracing::Level::INFO)
}

/// Exact Core diagnostic shapes; unknown keys are rejected before any field reaches the service file.
/// 精确 Core 诊断形状；任何字段进入服务文件前拒绝未知键。
#[derive(Deserialize)]
#[serde(untagged)]
enum NativeRecord {
    /// Ordinary operation-stage observations.
    /// 普通操作阶段观测。
    Stage(StageRecord),
    /// Physically retired instance with explicitly absent operation identity.
    /// 物理退役实例，操作身份明确缺失。
    Retired(RetiredRecord),
}

/// Fixed operation record emitted by Core OperationDiagnostics::emit.
/// Core OperationDiagnostics::emit 发出的固定操作记录。
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct StageRecord {
    /// Frozen Core schema marker.
    /// 冻结 Core Schema 标记。
    luaskills_embedded_diagnostic: u8,
    /// Original runtime namespace.
    /// 原运行时命名空间。
    runtime_id: String,
    /// Original allocation domain.
    /// 原分配域。
    pool_id: String,
    /// Actual operation, required for every non-retirement phase.
    /// 实际操作，所有非退役阶段均要求存在。
    operation_id: String,
    /// Actual instance; queue events legitimately have none.
    /// 实际实例；队列事件可明确为空。
    instance_id: Option<String>,
    /// Closed list of current Core phase names.
    /// 当前 Core 阶段名称的封闭列表。
    phase: Phase,
    /// Actual interval or absent measurement.
    /// 实际区间或缺失测量。
    elapsed_ns: Option<u64>,
    /// Live allocator sample, never a peak.
    /// 存活分配器采样，绝不是峰值。
    lua_heap_bytes: Option<u64>,
    /// Nested host wait, which overlaps its surrounding Lua stage.
    /// 嵌套宿主等待，与外围 Lua 阶段重叠。
    host_wait: Option<HostWait>,
    /// Actual returned outcome where present.
    /// 存在时的实际返回结果。
    succeeded: Option<bool>,
}

/// Phase names verified against current Core pool and queue emission sites.
/// 已按当前 Core 池和队列发送点核对的阶段名称。
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Queue,
    Allocation,
    VmCreation,
    Initialization,
    InitializationSkipped,
    BundleCompileAndEvaluate,
    InitializationRequestCleanup,
    InvocationBefore,
    Business,
    BusinessRequestCleanup,
}

/// Core's exact nested host wait shape; extra fields cannot leak into logs.
/// Core 精确嵌套宿主等待形状；额外字段不能泄露到日志。
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct HostWait {
    /// Actual accumulated wait nanoseconds.
    /// 实际累计等待纳秒。
    elapsed_ns: u64,
    /// Actual host calls within the stage.
    /// 阶段内实际宿主调用数。
    calls: u64,
}

/// Core's retired event has different measurements and preserves operation_id=null.
/// Core 退役事件具有不同测量，并保留 operation_id=null。
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct RetiredRecord {
    /// Frozen Core schema marker.
    /// 冻结 Core Schema 标记。
    luaskills_embedded_diagnostic: u8,
    /// Original runtime namespace.
    /// 原运行时命名空间。
    runtime_id: String,
    /// Original domain, not the last business owner.
    /// 原执行域，而非最后业务所有者。
    pool_id: String,
    /// Unit deserializes JSON null exclusively.
    /// unit 类型仅接受 JSON null。
    operation_id: (),
    /// Actually destroyed instance identity.
    /// 实际销毁实例身份。
    instance_id: String,
    /// Literal retired phase enforced during deserialization.
    /// 反序列化时强制要求 retired 阶段字面值。
    phase: RetirementPhase,
    /// Actual close interval.
    /// 实际关闭区间。
    close_elapsed_ns: u64,
    /// Actual destruction interval.
    /// 实际销毁区间。
    destruction_elapsed_ns: u64,
    /// Live heap before close, with no fabricated post-destruction zero.
    /// 关闭前存活堆值，不虚构销毁后的零值。
    lua_heap_before_close_bytes: u64,
}

/// Only the exact physically retired phase is legal in the retirement schema.
/// 退役 Schema 中仅精确物理退役阶段合法。
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum RetirementPhase {
    Retired,
}

/// Parse only marker-bearing exact Core shapes; return None for ordinary possibly-sensitive logs.
/// 仅解析带标记的精确 Core 形状；普通可能敏感日志返回空。
fn parse_core(message: &str) -> Result<Option<Value>, ()> {
    // The marker is a literal Core field; other library log text is never forwarded or raw-printed.
    // 标记是 Core 固定字段；其他库日志文本绝不转发或直接输出。
    let Ok(value) = serde_json::from_str::<Value>(message) else {
        return Ok(None);
    };
    // Selection requires the exact version-one numeric marker, not a substring in ordinary log text.
    // 选择要求精确的数字一版本标记，而非普通日志文本中的子字符串。
    if value
        .get("luaskills_embedded_diagnostic")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Ok(None);
    }
    // Core always emits every nullable key. Missing keys are schema violations, not fallback defaults.
    // Core 始终发送全部可空键。缺失键违反 Schema，不使用默认兜底。
    let required: &[&str] = if value.get("phase").and_then(Value::as_str) == Some("retired") {
        &[
            "luaskills_embedded_diagnostic",
            "runtime_id",
            "pool_id",
            "operation_id",
            "instance_id",
            "phase",
            "close_elapsed_ns",
            "destruction_elapsed_ns",
            "lua_heap_before_close_bytes",
        ]
    } else {
        &[
            "luaskills_embedded_diagnostic",
            "runtime_id",
            "pool_id",
            "operation_id",
            "instance_id",
            "phase",
            "elapsed_ns",
            "lua_heap_bytes",
            "host_wait",
            "succeeded",
        ]
    };
    if required.iter().any(|key| value.get(*key).is_none()) {
        return Err(());
    }
    // Deserialize the original string to reject duplicate keys instead of normalizing them through Value.
    // 反序列化原字符串以拒绝重复键，而非经由 Value 将其归一化。
    let record: NativeRecord = serde_json::from_str(message).map_err(|_| ())?;
    match record {
        NativeRecord::Stage(record)
            if record.luaskills_embedded_diagnostic == 1
                && !record.runtime_id.is_empty()
                && !record.pool_id.is_empty()
                && !record.operation_id.is_empty() =>
        {
            serde_json::to_value(record).map(Some).map_err(|_| ())
        }
        NativeRecord::Retired(record)
            if record.luaskills_embedded_diagnostic == 1
                && !record.runtime_id.is_empty()
                && !record.pool_id.is_empty()
                && !record.instance_id.is_empty() =>
        {
            serde_json::to_value(record).map(Some).map_err(|_| ())
        }
        _ => Err(()),
    }
}

/// Consumer-local statistics reset when the shared file rotates; never count historical lifetimes.
/// 消费者本地统计在共享文件轮转时重置；绝不计数历史生命周期。
#[derive(Default, serde::Serialize)]
struct WindowCounts {
    /// Accepted whitelist records in this observed segment.
    /// 当前观测分段内接受的白名单记录。
    accepted: u64,
    /// Whitelist records excluded by the current RUST_LOG policy.
    /// 当前 RUST_LOG 策略排除的白名单记录。
    filtered: u64,
    /// Marker-bearing records that violate the frozen schema.
    /// 带标记但违反冻结 Schema 的记录。
    invalid: u64,
    /// Ordinary Core messages intentionally excluded before output.
    /// 输出前有意排除的普通 Core 消息。
    ordinary_ignored: u64,
    /// Actual IO failures in this observed segment.
    /// 当前观测分段内真实 IO 失败。
    io_errors: u64,
}

/// Dedicated checkpoint metadata whose segment placement is finalized under the original writer lock.
/// 专用检查点元数据，其分段落点在原写入器锁内最终确定。
pub(crate) struct NativeCheckpoint {
    /// Fixed safe checkpoint document; ordinary Core text cannot enter this wrapper.
    /// 固定安全检查点文档；普通 Core 文本不能进入此包装。
    record: Value,
    /// Exact segment to which the copied counts belong before placement is finalized.
    /// 落点最终确定之前，已复制计数所属的精确分段。
    counts_segment: u64,
}

impl NativeCheckpoint {
    /// Copy current `counts` and handoff deltas for `segment`; preserve the distinct terminal/flush scopes.
    /// 为 `segment` 复制当前 `counts` 和交接差值；保留分别定义的终态及刷新范围。
    /// Return metadata only; the writer subsequently determines the actual segment atomically with IO.
    /// 仅返回元数据；写入器随后与 IO 原子地确定真实分段。
    fn new(
        segment: u64,
        first_segment: u64,
        counts: &WindowCounts,
        losses: &Losses,
        terminal: bool,
        io_failed: bool,
    ) -> Self {
        Self {
            record: json!({
                "event":"native_capture_checkpoint", "segment_id":segment,
                "counter_scope":"observed_rotation_segment",
                "selected_core_scope":"luaskills_embedded_diagnostic_equals_1",
                "handoff_scope":"since_previous_consumer_checkpoint",
                "counts":counts, "handoff_losses":losses.checkpoint(),
                "terminal":terminal, "native_closed":losses.native_closed.load(Ordering::Acquire),
                "tail_drained_before_final_flush":terminal,
                "complete_confirmation":"owner_drain_and_file_flush_ack_required",
                "retained_window_requires_key_reconciliation":true,
                "rotation_observed":segment != first_segment,
                "incomplete":losses.incomplete.load(Ordering::Acquire) || io_failed || (terminal && !losses.native_closed.load(Ordering::Acquire)),
            }),
            counts_segment: segment,
        }
    }

    /// Bind to actual `segment` under the writer lock; reset only counts that belong to a different segment.
    /// 在写入器锁内绑定真实 `segment`；仅重置属于不同分段的计数。
    /// `first_segment` controls rotation disclosure; handoff deltas keep their independent checkpoint scope.
    /// `first_segment` 控制轮转披露；交接差值保留其独立检查点范围。
    pub(crate) fn bind_segment(&mut self, segment: u64, first_segment: u64) -> std::io::Result<()> {
        if self.counts_segment != segment {
            self.record["counts"] = serde_json::to_value(WindowCounts::default())?;
            self.counts_segment = segment;
        }
        self.record["segment_id"] = json!(segment);
        self.record["rotation_observed"] = json!(segment != first_segment);
        Ok(())
    }

    /// Serialize the fixed checkpoint plus newline after its actual segment has been bound.
    /// 在绑定真实分段后，序列化固定检查点及换行。
    /// Return exact bytes for the writer's existing per-segment capacity decision, or serialization failure.
    /// 返回精确字节供写入器既有单段容量判断，或返回序列化失败。
    pub(crate) fn line(&self) -> std::io::Result<Vec<u8>> {
        // No caller text or dynamic business keys are accepted by this dedicated checkpoint type.
        // 此专用检查点类型不接收调用方文本或动态业务键。
        let mut bytes = serde_json::to_vec(&self.record)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// Drain `receiver` to the existing `writer`; `enabled` is the actual tracing filter or a test seam.
/// 将 `receiver` 排空到既有 `writer`；`enabled` 是实际 tracing 过滤器或测试接缝。
/// Return success only for a complete terminal tail with real write and flush acknowledgements.
/// 仅完整终态尾部且具有真实写入及刷新确认时返回成功。
fn consume(
    receiver: Receiver<Observation>,
    writer: crate::logging::LogWriter,
    losses: Arc<Losses>,
    observer: Weak<VulcanPluginNativeIdentitySubscription>,
    enabled: fn() -> bool,
) -> Result<(), String> {
    // These are current-segment counters, with no history map or native ownership references.
    // 这些是当前分段计数，不包含历史表或原生所有权引用。
    let mut segment = writer.native_segment().map_err(|error| error.to_string())?;
    // The initial rotation identity, used only to disclose window changes.
    // 初始轮转身份，仅用于披露窗口变化。
    let first_segment = segment;
    // Statistics restricted to the current observed rotation segment.
    // 仅当前观测轮转分段的统计。
    let mut counts = WindowCounts::default();
    // Last consumer checkpoint boundary.
    // 最近消费者检查点边界。
    let mut last_checkpoint = Instant::now();
    // Conservative actual IO failure state for terminal acknowledgement.
    // 用于终态确认的保守真实 IO 失败状态。
    let mut io_failed = false;
    // Idle capture never rotates the shared log merely to repeat an empty heartbeat.
    // 空闲采集不会仅为重复空心跳而轮转共享日志。
    let mut checkpoint_dirty = false;
    loop {
        // Observe only a copied counter. This temporary weak upgrade never queries an execution owner.
        // 仅观测复制计数。临时弱引用升级绝不查询执行所有者。
        if let Some(observer) = observer.upgrade() {
            losses.observe_failures(observer.observation_failures());
        }
        // Shared tracing writes may rotate independently; discard old counters instead of mixing windows.
        // 共享 tracing 写入可独立轮转；丢弃旧计数而非混合窗口。
        match writer.native_segment() {
            Ok(current) if current != segment => {
                segment = current;
                counts = WindowCounts::default();
            }
            Err(_) => {
                io_failed = true;
                counts.io_errors += 1;
                losses.incomplete.store(true, Ordering::Release);
            }
            _ => {}
        }
        let (observation, terminal) = match receiver.recv_timeout(CHECKPOINT_INTERVAL) {
            Ok(observation) => (Some(observation), false),
            Err(mpsc::RecvTimeoutError::Timeout) => (None, false),
            Err(mpsc::RecvTimeoutError::Disconnected) => (None, true),
        };
        if let Some(observation) = observation {
            checkpoint_dirty = true;
            // Whitelisted metadata only, ready for consumer-side persistence.
            // 仅白名单元数据，准备由消费者持久化。
            let record = match observation {
                Observation::Core(message) => match parse_core(&message) {
                    Ok(Some(record)) => Some(json!({"event":"native_phase", "native":record})),
                    Ok(None) => {
                        counts.ordinary_ignored += 1;
                        None
                    }
                    Err(()) => {
                        counts.invalid += 1;
                        losses.incomplete.store(true, Ordering::Release);
                        None
                    }
                },
                Observation::Identity(event) => match serde_json::to_value(event) {
                    Ok(record) => Some(json!({"event":"native_identity", "identity":record})),
                    Err(_) => {
                        counts.invalid += 1;
                        losses.incomplete.store(true, Ordering::Release);
                        None
                    }
                },
            };
            if let Some(record) = record {
                if enabled() {
                    match writer.write_native_record(&record) {
                        Ok(current) => {
                            // The actual write may itself rotate after the earlier segment observation.
                            // 真实写入可能在先前分段观测之后自行轮转。
                            if current != segment {
                                segment = current;
                                counts = WindowCounts::default();
                            }
                            counts.accepted += 1;
                        }
                        Err(_) => {
                            io_failed = true;
                            counts.io_errors += 1;
                            losses.incomplete.store(true, Ordering::Release);
                        }
                    }
                } else {
                    counts.filtered += 1;
                    losses.incomplete.store(true, Ordering::Release);
                }
            }
        }
        // Producer losses require a checkpoint even when no event could enter the full queue.
        // 即使事件无法进入已满队列，生产者丢失仍要求检查点。
        let pending_losses = losses.full.load(Ordering::Acquire) != 0
            || losses.disconnected.load(Ordering::Acquire) != 0
            || losses.oversize.load(Ordering::Acquire) != 0
            || losses.observer_missing.load(Ordering::Acquire) != 0;
        if terminal
            || ((checkpoint_dirty || pending_losses)
                && last_checkpoint.elapsed() >= CHECKPOINT_INTERVAL)
        {
            // A summary applies only to this observed rotation segment and this handoff checkpoint.
            // 摘要仅适用于当前观测轮转分段及当前交接检查点。
            let mut checkpoint = NativeCheckpoint::new(
                segment,
                first_segment,
                &counts,
                &losses,
                terminal,
                io_failed,
            );
            // Filtered output cannot claim evidence. The boundary returns the error directly to the owner.
            // 被过滤输出不能声称证据存在。边界直接向所有者返回错误。
            if enabled() {
                match writer.write_native_checkpoint(&mut checkpoint, first_segment) {
                    Ok(current) => {
                        // Keep consumer counters aligned with the same segment actually used for the checkpoint.
                        // 使消费者计数与检查点实际使用的同一分段保持一致。
                        if current != segment {
                            segment = current;
                            counts = WindowCounts::default();
                        }
                    }
                    Err(_) => {
                        io_failed = true;
                        counts.io_errors += 1;
                        losses.incomplete.store(true, Ordering::Release);
                    }
                }
            } else {
                counts.filtered += 1;
                losses.incomplete.store(true, Ordering::Release);
            }
            last_checkpoint = Instant::now();
            checkpoint_dirty = false;
        }
        if terminal {
            if writer.flush_native().is_err() {
                io_failed = true;
                losses.incomplete.store(true, Ordering::Release);
            }
            return if !io_failed
                && losses.native_closed.load(Ordering::Acquire)
                && !losses.incomplete.load(Ordering::Acquire)
            {
                Ok(())
            } else {
                Err("Native diagnostic capture is incomplete (loss, filter, schema, observer, IO, or unconfirmed native shutdown)".into())
            };
        }
    }
}

#[cfg(test)]
mod tests;
