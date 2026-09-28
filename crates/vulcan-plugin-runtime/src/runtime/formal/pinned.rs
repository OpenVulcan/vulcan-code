//! Pinned production sessions retain host identities before preparation and until physical retirement.
//! 固定生产会话在准备前保留宿主身份，直至物理退役。

use super::calls::{OBSERVATION_INTERVAL, core_error};
use super::*;
use crate::{VulcanPluginDescriptor, VulcanPluginToolDescriptor, VulcanPluginToolExecutionKind};
use luaskills::LuaInvocationContext;
use luaskills::runtime::embedded::{
    EmbeddedErrorCode, EmbeddedPluginConfig, EmbeddedSessionPhase, EmbeddedSessionSnapshot,
    ExecutionBackend, InstanceReuse, ModuleFinalizer, OperationHandle, OperationSnapshot,
    PluginPoolConfig, PoolKind, json_size,
};
use serde_json::{Map, Value, json};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

mod admission;
mod control;
mod monitor;

/// A stable owner-bound host identity pins the original descriptor and resource reservation.
/// 稳定的所有者绑定宿主身份固定原描述符及资源预留。
pub(super) struct PinnedSession {
    /// Host-issued identity exists before the first blocking preparation task.
    /// 宿主签发身份在首个阻塞准备任务前存在。
    id: String,
    /// The package generation never follows a later catalog refresh.
    /// 包代次绝不跟随后续目录刷新。
    plugin: VulcanPluginDescriptor,
    /// Controller ownership is supplied by the host, never by Lua.
    /// 控制器归属由宿主提供，绝不来自 Lua。
    owner: String,
    /// Conservative input reservation is shared with stateless admission.
    /// 保守输入预留与无状态入场共享。
    pub(super) input_bytes: usize,
    /// Explicit or abandoned observations permanently stop this exact session.
    /// 显式请求或放弃观测会永久停止此精确会话。
    closing: AtomicBool,
    /// Short synchronous state protects publication and cleanup evidence.
    /// 短同步状态保护发布及清理证据。
    state: Mutex<PinnedState>,
}

/// Exact preparation, core ownership and retirement checkpoints; no alternate VM scheduler is created.
/// 精确准备、核心所有权及退役检查点；不创建另一 VM 调度器。
struct PinnedState {
    /// Actual blocking preparation owns this flag until its closure ends.
    /// 实际阻塞准备持有此标志至其闭包结束。
    preparing: bool,
    /// A stopped executor leaves the original record available to a later drain.
    /// 已停止执行器使原记录继续可由后续排空处理。
    monitored: bool,
    /// Preparation result is published even when its original waiter has gone away.
    /// 即使原等待方已离开，准备结果仍会发布。
    prepared: Option<Result<(), VulcanPluginRuntimeError>>,
    /// Immutable pool identity remains queryable after physical forgetting.
    /// 不可变池身份在物理遗忘后继续可查询。
    pool: Option<String>,
    /// Core-issued pinned identity; absent only before admission or after preparation rejection.
    /// 核心签发固定身份；仅入场前或准备拒绝后省略。
    core_session: Option<String>,
    /// Initialization operation is acknowledged separately from the opening business export.
    /// 初始化操作与打开业务导出分别确认。
    initialization: Option<OperationHandle>,
    /// Opening business completion gates later tool admission.
    /// 打开业务完成用于门控后续工具入场。
    opened: bool,
    /// Monotonic activity time avoids wall-clock jumps during idle retirement.
    /// 单调活动时间避免闲置退役受墙钟跳变影响。
    activity: Instant,
    /// Frozen declared idle duration, if this package enables idle retirement.
    /// 冻结的声明闲置时长，仅在包启用闲置退役时存在。
    idle: Option<Duration>,
    /// Core session record was forgotten successfully before pool retirement.
    /// 核心会话记录已在池退役前成功遗忘。
    session_forgotten: bool,
    /// Exact pool was physically forgotten before host accounting release.
    /// 精确池已在宿主记账释放前物理遗忘。
    pool_forgotten: bool,
    /// Host accounting and physical cleanup both completed.
    /// 宿主记账与物理清理均已完成。
    retired: bool,
    /// Explicit acknowledgement serializes result forgetting against remaining tool observers.
    /// 显式确认使结果遗忘与剩余工具观测者串行。
    acknowledged: bool,
    /// Automatic idle closure may acknowledge a clean session once no earlier results remain.
    /// 自动闲置关闭可在没有较早保留结果后确认干净会话。
    idle_closed: bool,
    /// Independent finalization evidence is copied before the core session record is forgotten.
    /// 独立关闭证据在核心会话记录遗忘前复制。
    finalization: Option<OperationSnapshot>,
    /// Last explicit cleanup error is retained instead of pretending ownership was released.
    /// 保留最后明确清理错误，不伪称所有权已经释放。
    cleanup_error: Option<String>,
}

/// Owner-filtered live session evidence, including unstarted preparation and interrupted cleanup.
/// 按所有者过滤的实时会话证据，包含未开始准备及中断清理。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginFormalSessionStatus {
    /// Stable public host identity accepted by session tools and control methods.
    /// 会话工具及控制方法接受的稳定公开宿主身份。
    pub session_id: String,
    /// Original owning plugin identity.
    /// 原所属插件身份。
    pub plugin_id: String,
    /// Original immutable host routing generation.
    /// 原不可变宿主路由代次。
    pub plugin_generation: u64,
    /// Current core evidence; absent before admission or after successful forgetting.
    /// 当前核心证据；入场前或成功遗忘后省略。
    pub core: Option<EmbeddedSessionSnapshot>,
    /// True only after physical cleanup and host accounting both finish.
    /// 仅在物理清理及宿主记账均完成后为真。
    pub retired: bool,
    /// A later explicit drain must resume supervision interrupted by executor shutdown.
    /// 后续显式排空须恢复执行器关闭所中断的监督。
    pub requires_cleanup_recovery: bool,
    /// Independent closing result survives core session forgetting.
    /// 独立关闭结果在核心会话遗忘后保留。
    pub finalization: Option<OperationSnapshot>,
    /// Preparation or cleanup failure remains explicit.
    /// 准备或清理失败保持明确。
    pub error: Option<String>,
}

/// An abandoned tool observer stops its exact pinned session without releasing capacity.
/// 被放弃的工具观测者停止其精确固定会话，不释放容量。
struct PinnedObservation {
    /// Original registered owner survives future cancellation.
    /// 原注册所有者在 future 取消后存活。
    session: Arc<PinnedSession>,
    /// Only a delivered terminal result disarms implicit cancellation.
    /// 仅已交付终态结果解除隐式取消。
    completed: bool,
}

impl Drop for PinnedObservation {
    /// Preserve ownership while requesting cancellation if result observation was abandoned.
    /// 结果观测被放弃时保留所有权并请求取消。
    fn drop(&mut self) {
        if !self.completed {
            self.session.closing.store(true, Ordering::Release);
        }
    }
}

/// A monitor guard records executor interruption even if its task never starts polling.
/// 即使任务从未开始轮询，监控守卫也会记录执行器中断。
struct PinnedMonitorGuard(Arc<PinnedSession>);

/// Queued preparation owns its checkpoint even if the executor drops the closure before execution.
/// 即使执行器在执行前丢弃闭包，排队准备也拥有其检查点。
struct PinnedPreparationGuard(Arc<PinnedSession>);

impl Drop for PinnedPreparationGuard {
    /// Publish interrupted preparation only if the actual closure did not publish its own result.
    /// 仅在实际闭包未发布自身结果时发布准备中断。
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock()
            && state.preparing
        {
            state.prepared = Some(Err(VulcanPluginRuntimeError::new(
                "Formal session preparation was interrupted.",
            )));
            state.preparing = false;
            self.0.closing.store(true, Ordering::Release);
        }
    }
}

impl Drop for PinnedMonitorGuard {
    /// Release only supervision ownership; the original core resources remain retained.
    /// 仅释放监督所有权；原核心资源继续保留。
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.monitored = false;
        }
    }
}

impl PinnedSession {
    /// Acquire exact evidence or return explicit poison; never fabricate empty state.
    /// 获取精确证据或明确返回中毒；绝不伪造空状态。
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, PinnedState>, VulcanPluginRuntimeError> {
        self.state
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Formal session evidence lock is poisoned."))
    }
}

impl ManagedFormalRuntime {
    /// Stop every registered host session, including those whose core preparation has not started.
    /// 停止全部注册宿主会话，包含尚未开始核心准备的会话。
    pub(super) fn cancel_pinned(&self) -> Result<(), VulcanPluginRuntimeError> {
        for session in self.pinned.lock().map_err(|_| registry_error())?.values() {
            session.closing.store(true, Ordering::Release);
        }
        Ok(())
    }

    /// Return true only when every host session has actually retired and preparation ended.
    /// 仅在每个宿主会话实际退役且准备结束后返回真值。
    pub(super) fn pinned_drained(&self) -> Result<bool, VulcanPluginRuntimeError> {
        for session in self.pinned.lock().map_err(|_| registry_error())?.values() {
            let state = session.lock()?;
            if !state.retired || state.preparing {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Project the exact host record and current core evidence without inferring a vanished core identity.
/// 投影精确宿主记录与当前核心证据，不推断已消失核心身份。
fn pinned_status(
    owner: &ManagedFormalRuntime,
    session: &PinnedSession,
) -> Result<VulcanPluginFormalSessionStatus, VulcanPluginRuntimeError> {
    let state = session.lock()?;
    let core = if state.session_forgotten {
        None
    } else {
        state
            .core_session
            .as_ref()
            .map(|id| owner.host.runtime().session(id))
            .transpose()
            .map_err(core_error)?
    };
    Ok(VulcanPluginFormalSessionStatus {
        session_id: session.id.clone(),
        plugin_id: session.plugin.id.clone(),
        plugin_generation: session.plugin.generation,
        core,
        retired: state.retired,
        requires_cleanup_recovery: !state.monitored && !state.retired,
        finalization: state.finalization.clone(),
        error: state.cleanup_error.clone().or_else(|| {
            state
                .prepared
                .as_ref()
                .and_then(|result| result.as_ref().err().map(ToString::to_string))
        }),
    })
}

/// Observe the already-submitted core operation asynchronously; this function never owns a separate execution thread.
/// 异步观测已提交核心操作；此函数绝不拥有单独执行线程。
async fn wait_operation(
    operation: &OperationHandle,
) -> Result<OperationSnapshot, VulcanPluginRuntimeError> {
    loop {
        let snapshot = operation.snapshot().map_err(core_error)?;
        if snapshot.phase.is_terminal() {
            return Ok(snapshot);
        }
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    }
}

/// Decode the fixed version-one business frame from a terminal core operation.
/// 从终态核心操作解码固定版本一业务帧。
fn project_business(snapshot: &OperationSnapshot) -> Result<Value, VulcanPluginRuntimeError> {
    let frame = core_result(snapshot)?;
    super::super::lease::parse_plugin_result(&json!({"result":frame}))
}

/// Read the independent closing frame instead of the lifecycle operation's null business baseline.
/// 读取独立关闭帧，而非生命周期操作的空值业务基线。
fn project_closing(snapshot: &OperationSnapshot) -> Result<Value, VulcanPluginRuntimeError> {
    let stages = snapshot.finalization.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Session closing operation has no finalization evidence.")
    })?;
    let result = stages
        .outcome
        .as_ref()
        .ok_or_else(|| VulcanPluginRuntimeError::new("Session closing outcome is not confirmed."))?
        .result()
        .map_err(core_error)?;
    super::super::lease::parse_plugin_result(&json!({"result":result}))
}

/// Decode an already-terminal core result, retaining explicit null and rejecting inconsistent evidence.
/// 解码已终态核心结果，保留明确空值并拒绝矛盾证据。
fn core_result(snapshot: &OperationSnapshot) -> Result<Value, VulcanPluginRuntimeError> {
    match (&snapshot.value, &snapshot.error) {
        (Some(value), None) if snapshot.phase.is_terminal() => Ok(value.clone()),
        (None, Some(error)) if snapshot.phase.is_terminal() => Err(core_error(error.clone())),
        _ => Err(VulcanPluginRuntimeError::new(
            "Terminal session operation has inconsistent result evidence.",
        )),
    }
}

/// Freeze a positive millisecond budget as an absolute monotonic deadline or return an explicit range error.
/// 将正毫秒预算冻结为绝对单调截止时间，或返回明确范围错误。
fn deadline(timeout_ms: u64) -> Result<Instant, VulcanPluginRuntimeError> {
    if timeout_ms == 0 {
        return Err(VulcanPluginRuntimeError::new(
            "Formal session timeout must be positive.",
        ));
    }
    Instant::now()
        .checked_add(Duration::from_millis(timeout_ms))
        .ok_or_else(|| VulcanPluginRuntimeError::new("Formal session deadline is out of range."))
}

/// Derive remaining admission time from the original deadline without extending it during preparation.
/// 从原截止时间派生剩余入场时间，不在准备期间延长。
fn remaining(deadline: Instant) -> Result<Duration, VulcanPluginRuntimeError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|time| !time.is_zero())
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal session deadline expired before admission.")
        })
}

/// Preserve the established public session result envelope for `id`, business value and physical-close flag.
/// 为 `id`、业务值及物理关闭标志保留既有公开会话结果信封。
fn session_result(id: &str, result: Value, closed: bool) -> Value {
    json!({"session_id":id,"closed":closed,"result":result})
}

/// Attach the original host identity to an explicit failure so a later controller can query it.
/// 为明确失败附加原宿主身份，使后续控制器可以查询。
fn with_session(error: VulcanPluginRuntimeError, id: &str) -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new(format!("{error} [formal session {id}]"))
}

/// Report unavailable session ownership rather than substituting an empty registry.
/// 报告会话所有权不可用，而非替代为空注册表。
fn registry_error() -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new("Formal session registry lock is poisoned.")
}

/// Report unavailable catalog ordering without attempting alternate lock ownership.
/// 报告目录顺序不可用，不尝试替代锁所有权。
fn publication_error() -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
}
