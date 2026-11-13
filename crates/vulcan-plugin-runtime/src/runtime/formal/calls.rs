//! Bounded host ownership for one-shot tool, lifecycle, transient Action and probe execution.
//! 单次工具、生命周期、瞬时 Action 及探测执行的有界宿主所有权。

use super::*;
use crate::runtime::candidates::{ActivationExecution, CandidateOperation};
use crate::runtime::removal_lifecycle::{CallExecutionSource, RemovalLifecycleExecution};
use crate::{VulcanPluginDescriptor, VulcanPluginToolDescriptor};
use luaskills::LuaInvocationContext;
use luaskills::runtime::embedded::{
    EmbeddedCall, EmbeddedErrorCode, EmbeddedPrewarm, InstanceReuse, ModuleFinalizer,
    OperationContext, OperationHandle, OperationOutcome, OperationSnapshot, PluginPoolConfig,
};
use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

mod admission;
mod candidates;
pub(in crate::runtime::formal) mod diagnostics;
mod initialized;
mod removal;
pub use removal::VulcanPluginRemovalLifecycleCall;
mod predecessor;
mod prewarm;
mod registration;
pub(super) mod reuse;
pub use prewarm::{
    VulcanPluginAutomaticPrewarmOrigin, VulcanPluginAutomaticPrewarmPermit,
    VulcanPluginAutomaticPrewarmTrigger, VulcanPluginPrewarmTargetRequest,
    VulcanPluginPrewarmTargetStatus,
};

#[cfg(test)]
mod tests;

/// Caller ownership determines who may inspect and acknowledge retained evidence.
/// 调用方归属决定谁可以检查及确认保留证据。
#[derive(Serialize)]
enum CallOwner {
    /// A controller retains failed tool results until explicit acknowledgement.
    /// 控制器保留失败工具结果，直至显式确认。
    Controller(String),
    /// The lifecycle caller consumes its result; actual cleanup always precedes reclamation.
    /// 生命周期调用方消费其结果；实际清理始终先于回收。
    Lifecycle,
    /// A transient Action retains its own public action diagnostics while call history is reclaimed.
    /// 瞬时 Action 保留自身公开诊断，同时回收调用历史。
    Action,
    /// A probe validates module initialization without invoking any business export.
    /// 探测校验模块初始化，不调用任何业务导出。
    Probe,
}

impl CallOwner {
    /// Return whether admission must retain a published catalog session slot for this owner.
    /// 返回入场是否必须为此所有者保留已发布目录会话槽位。
    /// Unpublished work has no public row; original parent, plugin and purpose budgets still apply.
    /// 未发布工作没有公开行；原父级、插件及用途预算仍然生效。
    fn retains_published_slot(&self, unpublished: bool) -> bool {
        !unpublished && !matches!(self, Self::Probe)
    }

    /// Return the exact controller ID, or none for internal host calls.
    /// 返回精确控制器标识；宿主内部调用返回空值。
    fn controller_id(&self) -> Option<&str> {
        match self {
            Self::Controller(id) => Some(id),
            Self::Lifecycle | Self::Action | Self::Probe => None,
        }
    }
}

/// One control observation interval, also bounding direct asynchronous terminal observation.
/// 唯一控制观测间隔，同时限制直接异步终态观测。
pub(super) const OBSERVATION_INTERVAL: Duration = Duration::from_millis(10);

/// Observe `operation` until its real terminal signal or the existing observation interval expires.
/// 观测 `operation`，直至真实终态通知到达或既有观测间隔到期。
/// Return the original terminal snapshot or a fresh current snapshot; timeout never cancels or retires work.
/// 返回原终态快照或新的当前快照；观测超时绝不取消或退役工作。
pub(super) async fn observe_terminal(
    operation: &OperationHandle,
) -> Result<OperationSnapshot, VulcanPluginRuntimeError> {
    // Keep timers at the asynchronous host boundary while the core owns actual completion.
    // 将计时器保留在异步宿主边界，实际完成仍由核心拥有。
    match tokio::time::timeout(OBSERVATION_INTERVAL, operation.wait_terminal()).await {
        Ok(snapshot) => snapshot.map_err(core_error),
        Err(_) => operation.snapshot().map_err(core_error),
    }
}

/// One exact admitted host invocation; metadata never owns another VM scheduler.
/// 单个精确已接纳宿主调用；元数据绝不拥有另一个 VM 调度器。
pub(super) struct FormalCall {
    /// Stable host identity exists before any package preparation starts.
    /// 稳定宿主身份在任何包准备开始前存在。
    id: String,
    /// Frozen descriptor pins the original managed package generation.
    /// 冻结描述符固定原受管包代次。
    plugin: VulcanPluginDescriptor,
    /// One explicit source retains private request or namespace authority through actual cleanup.
    /// 单个显式来源在实际清理期间保留私有请求或命名空间权威。
    source: CallExecutionSource,
    /// Typed ownership prevents internal records from masquerading as controller calls.
    /// 类型化归属防止内部记录冒充控制器调用。
    ownership: CallOwner,
    /// Serialized input reservation changes only under the shared call registry lock and persists until acknowledgement.
    /// 序列化输入预留仅在共享调用注册表锁下改变，并保留至确认。
    pub(super) input_bytes: AtomicUsize,
    /// Cancellation remains live before and after core admission.
    /// 核心入场前后取消均保持实时。
    cancelled: AtomicBool,
    /// Internal results may be reclaimed only after the caller leaves and resources really retire.
    /// 内部结果仅在调用方离开且资源真实退役后可以回收。
    acknowledged: AtomicBool,
    /// Wake existing observers only after original supervision ends; notification owns no execution or retirement.
    /// 仅在原监督结束后唤醒既有观测者；通知不拥有执行或退役。
    finished: tokio::sync::Notify,
    /// Short state lock protects identity publication and cleanup evidence only.
    /// 短状态锁仅保护身份发布及清理证据。
    state: Mutex<CallState>,
}

/// Mutable control evidence for one host call, independent of its observer's lifetime.
/// 单个宿主调用的可变控制证据，独立于观测者寿命。
#[derive(Default)]
struct CallState {
    /// Original registered input waits for exactly one explicit scheduling transfer.
    /// 原已登记输入等待恰好一次明确调度转移。
    pending: Option<Preparation>,
    /// Target ownership retains completed operations separately from the current active operation.
    /// 目标归属将已完成操作与当前活动操作分别保留。
    prewarm: Option<prewarm::TargetProgress>,
    /// Pool identity precedes submission and lasts until exclusive retirement or completed reusable borrow release.
    /// 池身份先于提交，并保留至独占退役或已完成复用借用释放。
    pool: Option<String>,
    /// A borrowed reusable domain survives this call; its borrow ends only after request cleanup.
    /// 借用的可复用域在此调用后存活；借用仅在请求清理后结束。
    reusable_domain: Option<String>,
    /// Core-issued operation owns the actual execution and automatic finalization.
    /// 核心签发操作拥有实际执行及自动关闭。
    operation: Option<OperationHandle>,
    /// A temporary core session separates cancellable initialization from Action business admission.
    /// 临时核心会话将可取消初始化与 Action 业务入场分离。
    initialized: Option<initialized::InitializedCall>,
    /// Actual Action ownership spans preparation, initialization, business and physical pool retirement.
    /// 真实 Action 所有权覆盖准备、初始化、业务及物理池退役。
    action_work: Option<crate::runtime::action_status::ActionWorkGuard>,
    /// Result becomes observable only after cleanup, or as an explicit retained failure.
    /// 结果仅在清理后可见，或作为明确保留失败可见。
    result: Option<Result<Value, VulcanPluginRuntimeError>>,
    /// Completed request cleanup and exclusive retirement or reusable borrow release permit host slot reclamation.
    /// 已完成请求清理，以及独占退役或复用借用释放，允许回收宿主槽位。
    retired: bool,
    /// A single asynchronous observer owns completion at a time.
    /// 同时仅一个异步观测者拥有完成处理。
    supervised: bool,
    /// Queued or executing blocking preparation must finish before cleanup recovery can inspect ownership.
    /// 排队或执行中的阻塞准备必须结束，清理恢复才能检查所有权。
    preparing: bool,
    /// Closing failures remain distinct from business failures in drain diagnostics.
    /// 排空诊断中的关闭失败与业务失败保持区分。
    cleanup_error: Option<String>,
}

impl CallState {
    /// Reuse the sole acknowledgement predicate after actual physical retirement.
    /// 实际物理退役后复用唯一确认判定。
    fn can_acknowledge_retirement(&self) -> bool {
        self.retired && !self.supervised && !self.preparing && self.result.is_some()
    }
}

/// Workspace preparation is explicit; lifecycle environment creation belongs to the registered worker.
/// 工作区准备为显式声明；生命周期环境创建属于已登记工作者。
#[derive(Serialize)]
enum CallWorkspace {
    /// A caller has already supplied its authorized workspace.
    /// 调用方已经提供其授权工作区。
    Provided(PathBuf),
    /// Resolve and prepare the original descriptor's writable environment inside the tracked blocking task.
    /// 在受跟踪阻塞任务内解析并准备原描述符的可写环境。
    LifecycleEnvironment,
    /// Resolve the frozen execution package inside the registered preparation task.
    /// 在已登记准备任务内解析冻结执行包。
    ExecutionPackage,
}

/// Admission intent owns the exact export and any Action cancellation receiver.
/// 入场意图拥有精确导出及可选 Action 取消接收器。
enum CallAdmission {
    /// Tool admission binds its declared export to one controller owner.
    /// 工具入场将声明导出绑定至一个控制器所有者。
    Controller {
        /// The original controller owns this tool's retained history.
        /// 原控制器拥有此工具的保留历史。
        owner_session_id: String,
        /// Exact manifest tool identity selects the optional private reuse contract.
        /// 精确清单工具身份选择可选私有复用契约。
        tool_id: String,
        /// The validated manifest selects the exact tool export.
        /// 已校验清单选择精确工具导出。
        export: String,
    },
    /// A bounded initialization target retains its original pool and every admitted operation.
    /// 有界初始化目标保留原池及每个已接纳操作。
    PrewarmTarget {
        /// Original exact target authorized by the automatic policy.
        /// 由自动策略授权的原精确目标。
        request: VulcanPluginPrewarmTargetRequest,
        /// Automatic trigger and acknowledged policy identity that admitted this target.
        /// 接纳此目标的自动触发及已确认策略身份。
        automatic: prewarm::automatic::AutomaticAdmission,
    },
    /// Lifecycle admission invokes one host-selected declaration.
    /// 生命周期入场调用一个宿主选择的声明。
    Lifecycle {
        /// The host-selected lifecycle event resolves to this declared export.
        /// 宿主选择的生命周期事件解析至此声明导出。
        export: String,
    },
    /// Action cancellation remains live until initialization completes and business admission starts.
    /// Action 取消保持实时，直到初始化完成且业务入场开始。
    Action {
        /// The validated Action declaration selects its sole business export.
        /// 已校验 Action 声明选择其唯一业务导出。
        export: String,
        /// The original cancellation receiver, diagnostic record and deadline remain owned together.
        /// 原取消接收器、诊断记录及截止时间继续共同具有归属。
        execution: crate::runtime::execution::ActionExecution,
    },
    /// A probe requires only the declared module's successful initialization.
    /// 探测仅要求声明模块成功初始化。
    Probe,
    /// Initialization of one unpublished target holds the original transaction's real readiness lease.
    /// 单个未发布目标的初始化持有原事务的实际就绪租约。
    CandidateProbe {
        /// The host constructed this authority from an inspected, still-prepared transaction.
        /// 宿主从已检查且仍处于准备阶段的事务构造此权威。
        authority: Arc<ActivationExecution>,
    },
    /// Original namespace authority selects one managed removal lifecycle callback.
    /// 原命名空间权威选择单个受管移除生命周期回调。
    RemovalLifecycle {
        /// Original guard is retained by this exact execution authority.
        /// 原护栏由此精确执行权威保留。
        authority: Arc<RemovalLifecycleExecution>,
        /// Export comes exclusively from the original manifest.
        /// 导出仅来自原清单。
        export: String,
    },
    /// One original-request lifecycle callback retains its exact old or target package without publishing routing.
    /// 单个原请求生命周期回调保留精确旧包或目标包，不发布路由。
    ActivationLifecycle {
        /// Exact operation-specific transaction authority remains alive through real cleanup.
        /// 精确操作专属事务权威保留至实际清理。
        authority: Arc<ActivationExecution>,
        /// Function is selected only from the inspected manifest's lifecycle declaration.
        /// 函数仅从已检查清单生命周期声明选择。
        export: String,
    },
}

impl CallAdmission {
    /// Return the host-selected purpose without consulting business payloads or export name guesses.
    /// 返回宿主选择的用途，不读取业务载荷或猜测导出名称。
    fn capacity_purpose(&self) -> VulcanPluginCapacityPurpose {
        match self {
            Self::Controller { .. } | Self::PrewarmTarget { .. } => {
                VulcanPluginCapacityPurpose::ToolCall
            }
            Self::Lifecycle { .. }
            | Self::ActivationLifecycle { .. }
            | Self::RemovalLifecycle { .. } => VulcanPluginCapacityPurpose::Lifecycle,
            Self::Action { .. } => VulcanPluginCapacityPurpose::UiAction,
            Self::Probe | Self::CandidateProbe { .. } => {
                VulcanPluginCapacityPurpose::ActivationProbe
            }
        }
    }

    /// Project retained ownership from this one authoritative admission intent.
    /// 从此唯一权威入场意图投影保留归属。
    fn ownership(&self) -> CallOwner {
        match self {
            Self::Controller {
                owner_session_id, ..
            } => CallOwner::Controller(owner_session_id.clone()),
            Self::PrewarmTarget { request, .. } => {
                CallOwner::Controller(request.owner_session_id.clone())
            }
            Self::Lifecycle { .. }
            | Self::ActivationLifecycle { .. }
            | Self::RemovalLifecycle { .. } => CallOwner::Lifecycle,
            Self::Action { .. } => CallOwner::Action,
            Self::Probe | Self::CandidateProbe { .. } => CallOwner::Probe,
        }
    }

    /// Return the declared business export; probes and prewarming intentionally have no business function.
    /// 返回声明业务导出；探测及预热有意不具有业务函数。
    fn export(&self) -> Option<&str> {
        match self {
            Self::Controller { export, .. }
            | Self::Lifecycle { export }
            | Self::ActivationLifecycle { export, .. }
            | Self::RemovalLifecycle { export, .. }
            | Self::Action { export, .. } => Some(export),
            Self::Probe | Self::CandidateProbe { .. } | Self::PrewarmTarget { .. } => None,
        }
    }

    /// Return whether initialization must be a distinct, tracked core operation.
    /// 返回初始化是否必须成为独立且受跟踪的核心操作。
    fn separates_initialization(&self) -> bool {
        matches!(
            self,
            Self::Action { .. } | Self::Probe | Self::CandidateProbe { .. }
        )
    }

    /// Derive one execution source from the original admitted intent.
    /// 从原接纳意图派生单个执行来源。
    fn source(&self) -> CallExecutionSource {
        match self {
            Self::CandidateProbe { authority } | Self::ActivationLifecycle { authority, .. } => {
                CallExecutionSource::Activation(Arc::clone(authority))
            }
            Self::RemovalLifecycle { authority, .. } => {
                CallExecutionSource::RemovalLifecycle(Arc::clone(authority))
            }
            _ => CallExecutionSource::Published,
        }
    }

    /// Borrow original-request authority only for explicit activation admission.
    /// 仅为显式激活入场借用原请求权威。
    fn activation(&self) -> Option<&Arc<ActivationExecution>> {
        match self {
            Self::CandidateProbe { authority } | Self::ActivationLifecycle { authority, .. } => {
                Some(authority)
            }
            _ => None,
        }
    }
}

/// Byte-checked inputs moved into one bounded preparation task, with explicit workspace preparation.
/// 移入单个有界准备任务的已校验字节输入，具有显式工作区准备。
struct Preparation {
    /// Workspace is host-owned and canonicalized by the bundle registrar.
    /// 工作区由宿主持有，由 bundle 注册器规范化。
    workspace: CallWorkspace,
    /// The exact route-specific envelope is retained without adding unrelated protocol fields.
    /// 保留精确路由信封，不添加无关协议字段。
    arguments: Value,
    /// Only tool calls select automatic business shutdown; internal calls never borrow it.
    /// 仅工具调用选择自动业务关闭；内部调用绝不借用它。
    finalizer: Option<ModuleFinalizer>,
    /// One typed intent selects business routing, observation and initialization semantics together.
    /// 单个类型化意图共同选择业务路由、观测及初始化语义。
    admission: CallAdmission,
    /// Host lifecycle authority is frozen independently from the plugin arguments.
    /// 宿主生命周期权威独立于插件参数冻结。
    scope: VulcanPluginBridgeLeaseScope,
    /// Explicit security partitions distinguish controller and each internal execution purpose.
    /// 显式安全分区区分控制器及每种内部执行用途。
    security_partition: String,
    /// Absolute admission deadline includes blocking preparation and core queue waits.
    /// 绝对入场截止时间包含阻塞准备及核心队列等待。
    deadline: Instant,
}

/// Queryable host and core evidence for one owned stateless invocation.
/// 单个有归属无状态调用的可查询宿主及核心证据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginFormalCallStatus {
    /// Explicit target receipt remains queryable before startup and through partial or completed initialization.
    /// 明确目标回执在启动前及部分或完整初始化期间保持可查询。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prewarm: Option<VulcanPluginPrewarmTargetStatus>,
    /// Stable host identity accepted by query, cancellation and forgetting.
    /// 查询、取消及遗忘接受的稳定宿主身份。
    pub invocation_id: String,
    /// Exact plugin and package routing generation captured on admission.
    /// 入场时捕获的精确插件及包路由代次。
    pub plugin_id: String,
    /// Original routing generation never follows a later catalog refresh.
    /// 原路由代次绝不跟随后续目录刷新。
    pub plugin_generation: u64,
    /// Business or prewarm operation status; initialization-only probes have no business operation.
    /// 业务或预热操作状态；仅初始化探测没有业务操作。
    pub operation: Option<OperationSnapshot>,
    /// Actual temporary-session initialization remains queryable while its original core observer is retained.
    /// 保留原核心观察者期间，可以查询实际临时会话初始化。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initialization: Option<OperationSnapshot>,
    /// True only after request cleanup and exclusive pool retirement or completed reusable borrow release,
    /// once supervision has recorded the result; a retired record can be acknowledged.
    /// 仅在请求清理完成，以及独占池退役或已完成复用借用释放，且监督已记录结果后为真；
    /// 已退役记录可被确认。
    pub retired: bool,
    /// An interrupted supervisor requires explicit drain recovery, while actual resources stay owned.
    /// 中断的监督者需要显式排空恢复，真实资源仍保持归属。
    pub requires_cleanup_recovery: bool,
    /// Successful version-one business projection or native prewarm instance result, including explicit JSON null.
    /// 成功的版本一业务投影或原生预热实例结果，包含显式 JSON 空值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// Known host or business failure; retained core evidence remains independently queryable.
    /// 已知宿主或业务失败；保留的核心证据仍可独立查询。
    pub error: Option<String>,
}

/// Dropping an observation requests cancellation without dropping actual core ownership.
/// 丢弃观测会请求取消，不丢弃真实核心所有权。
struct ObservationGuard {
    /// The exact record stays live independently of registry removal.
    /// 精确记录独立于注册表移除保持存活。
    call: Arc<FormalCall>,
    /// Completed observation disarms implicit cancellation.
    /// 完成观测后解除隐式取消。
    completed: bool,
}

/// Mark observer interruption without releasing core ownership or replaying work.
/// 标记观测者中断，不释放核心所有权或重放工作。
struct SupervisionGuard {
    /// Exact registered record survives task abortion and runtime shutdown.
    /// 精确注册记录在任务中断及运行时关闭后存活。
    call: Arc<FormalCall>,
    /// Normal completion already published its authoritative result.
    /// 正常完成已发布其权威结果。
    completed: bool,
}

impl Drop for SupervisionGuard {
    /// Publish interruption only when completion was not recorded; retained identities enable explicit recovery.
    /// 仅在未记录完成时发布中断；保留身份用于显式恢复。
    fn drop(&mut self) {
        if let Ok(mut state) = self.call.lock() {
            state.supervised = false;
            if !self.completed {
                state.result = Some(Err(VulcanPluginRuntimeError::new(
                    "Formal supervision was interrupted; inspect and drain the retained invocation.",
                )));
            }
        }
        // Release the evidence lock before waking; poison must also wake observers to expose the actual lock error.
        // 唤醒前释放证据锁；中毒也必须唤醒观测者以暴露真实锁错误。
        self.call.finished.notify_waiters();
    }
}

/// Completion follows the actual blocking closure, even if its Tokio join observer disappears.
/// 完成跟随实际阻塞闭包，即使其 Tokio 等待观测者消失。
struct PreparationGuard {
    /// The queued closure owns this record until it is dropped or really returns.
    /// 排队闭包拥有此记录至丢弃或真实返回。
    call: Arc<FormalCall>,
}

impl Drop for PreparationGuard {
    /// Release only preparation ownership; physical pools and operations remain independently tracked.
    /// 仅释放准备所有权；物理池及操作继续独立跟踪。
    fn drop(&mut self) {
        if let Ok(mut state) = self.call.lock() {
            state.preparing = false;
        }
    }
}

impl Drop for ObservationGuard {
    /// Request cancellation when an awaiting caller leaves; never release a VM or host slot here.
    /// 等待调用方离开时请求取消；绝不在此释放 VM 或宿主槽位。
    fn drop(&mut self) {
        if !self.completed {
            self.call.cancelled.store(true, Ordering::Release);
        }
        self.call.acknowledged.store(true, Ordering::Release);
    }
}

impl FormalCall {
    /// Borrow this original call's host identity; internal ownership keeps controller absent.
    /// 借用此原调用的宿主身份；内部所有权保持控制器为空。
    /// Return metadata references without cloning the descriptor or package owner.
    /// 返回元数据引用，不克隆描述符或包所有者。
    fn native_identity_owner(&self) -> crate::runtime::native_identity::NativeIdentityOwner<'_> {
        crate::runtime::native_identity::NativeIdentityOwner {
            plugin_generation: self.plugin.generation,
            controller_session_id: self.ownership.controller_id(),
            host_invocation_id: Some(&self.id),
            host_session_id: None,
        }
    }

    /// Acquire the short evidence lock; report poison instead of inventing a completion state.
    /// 获取短证据锁；报告中毒而不伪造完成状态。
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, CallState>, VulcanPluginRuntimeError> {
        self.state.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Formal invocation evidence lock is poisoned.")
        })
    }

    /// Project current host evidence and exact core state; return any observation failure unchanged.
    /// 投影当前宿主证据及精确核心状态；原样返回任何观测失败。
    fn status(&self) -> Result<VulcanPluginFormalCallStatus, VulcanPluginRuntimeError> {
        let state = self.lock()?;
        Ok(VulcanPluginFormalCallStatus {
            prewarm: state
                .prewarm
                .as_ref()
                .map(prewarm::TargetProgress::status)
                .transpose()?,
            invocation_id: self.id.clone(),
            plugin_id: self.plugin.id.clone(),
            plugin_generation: self.plugin.generation,
            operation: state
                .operation
                .as_ref()
                .map(OperationHandle::snapshot)
                .transpose()
                .map_err(core_error)?,
            initialization: state
                .initialized
                .as_ref()
                .and_then(|session| session.initialization_operation())
                .map(OperationHandle::snapshot)
                .transpose()
                .map_err(core_error)?,
            // Report retirement only once supervision has recorded its result, the same condition
            // acknowledgement requires, so an observer never sees a retired record it cannot forget.
            // 仅在监督记录结果后报告退役，与确认所需条件一致，使观测者不会看到无法遗忘的已退役记录。
            retired: state.retired && !state.supervised && state.result.is_some(),
            requires_cleanup_recovery: state.pending.is_none()
                && !state.supervised
                && !state.retired,
            value: state
                .result
                .as_ref()
                .and_then(|result| result.as_ref().ok().cloned()),
            error: state
                .result
                .as_ref()
                .and_then(|result| result.as_ref().err().map(ToString::to_string)),
        })
    }
}

impl ManagedFormalRuntime {
    /// Cancel all tracked host preparations and operations before requesting core shutdown.
    /// 请求核心关闭前取消全部受跟踪宿主准备及操作。
    /// Returns registry failure without treating unknown ownership as an empty runtime.
    /// 返回注册表失败，不把未知所有权当作空运行时。
    pub(super) fn cancel_calls(&self) -> Result<(), VulcanPluginRuntimeError> {
        for call in self.calls.lock().map_err(|_| calls_error())?.values() {
            call.cancelled.store(true, Ordering::Release);
        }
        Ok(())
    }

    /// Confirm every host-owned call really retired and its supervisor completed; retained results may remain.
    /// 确认每个宿主调用真实退役且监督者完成；保留结果可继续存在。
    pub(super) fn calls_drained(&self) -> Result<bool, VulcanPluginRuntimeError> {
        for call in self.calls.lock().map_err(|_| calls_error())?.values() {
            let state = call.lock()?;
            if !state.retired || state.supervised || state.preparing {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl VulcanPluginRuntimeManager {
    /// Check whether the exact plugin still owns an unretired formal call, including probe preparation.
    /// 检查精确插件是否仍持有未退役正式调用，包括探测准备。
    /// Returns true only for physical ownership; errors preserve cleanup diagnostics.
    /// 仅对物理所有权返回真；错误保留清理诊断。
    pub(in crate::runtime) fn has_unretired_formal_calls(
        &self,
        plugin_id: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        // The optional parent is the sole authority for every formal call.
        // 可选父级是每个正式调用的唯一权威。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(false);
        };
        // Retain the registry while checking the same physical-retirement checkpoints.
        // 检查同一物理退役检查点期间保留注册表。
        let calls = owner.calls.lock().map_err(|_| calls_error())?;
        for call in calls.values().filter(|call| call.plugin.id == plugin_id) {
            if !call.lock()?.retired {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Resume only idle cleanup after actual preparation ended; never repeat module preparation or submission.
    /// 仅在实际准备结束后恢复闲置清理；绝不重复模块准备或提交。
    /// `owner` and `call` are retained originals; return explicit metadata failure without inventing evidence.
    /// `owner` 及 `call` 为保留原件；明确返回元数据失败，不伪造证据。
    fn resume_formal_cleanup(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        call: &Arc<FormalCall>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Reject a missing executor before supervision ownership is transferred.
        // 在转移监督归属前拒绝缺失的执行器。
        let supervisor = self.supervisor_handle()?;
        let previous = {
            let mut state = call.lock()?;
            if state.supervised || state.preparing || state.retired {
                return Ok(());
            }
            let previous = state.result.clone().ok_or_else(|| {
                VulcanPluginRuntimeError::new(
                    "Formal cleanup has no completed supervision evidence.",
                )
            })?;
            state.supervised = true;
            previous
        };
        let manager = self.clone();
        let retry_owner = Arc::clone(owner);
        let retry_call = Arc::clone(call);
        let supervision = SupervisionGuard {
            call: Arc::clone(call),
            completed: false,
        };
        supervisor.spawn(async move {
            let mut supervision = supervision;
            let result = manager
                .finish_formal_call(&retry_owner, &retry_call, previous.map(|_| ()))
                .await;
            if let Ok(mut state) = retry_call.lock() {
                state.result = Some(result);
            }
            supervision.completed = true;
            // End the original supervision before acknowledgement-based reclamation, including cancelled observers.
            // 在按确认回收之前结束原监督，包含已取消观测者。
            drop(supervision);
            if let Err(error) = manager.reap_internal_calls(&retry_owner) {
                let _ = manager.record_cleanup_failure(&retry_call.plugin.id, error.to_string());
            }
        });
        Ok(())
    }

    /// Cancel and observe calls matching explicit plugin and/or controller ownership selectors.
    /// 取消并观测匹配明确插件及／或控制器所有者选择条件的调用。
    /// Return bounded drain evidence; expiration retains the real owner and never replays plugin code.
    /// 返回有界排空证据；到期保留真实所有者，绝不重放插件代码。
    pub(crate) async fn drain_formal_calls(
        &self,
        plugin_id: Option<&str>,
        owner_session_id: Option<&str>,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        Ok(self
            .observe_formal_calls(plugin_id, owner_session_id)
            .await?
            .report)
    }

    /// Observe calls selected by plugin_id and owner_session_id; return classified original cleanup without changing public reports.
    /// 观测由 plugin_id 及 owner_session_id 选中的调用；返回分类原清理，不改变公开报告。
    pub(in crate::runtime) async fn observe_formal_calls(
        &self,
        plugin_id: Option<&str>,
        owner_session_id: Option<&str>,
    ) -> Result<crate::runtime::drain::DrainObservation, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(crate::runtime::drain::DrainObservation::default());
        };
        let calls: Vec<_> = owner
            .calls
            .lock()
            .map_err(|_| calls_error())?
            .values()
            .filter(|call| {
                plugin_id.is_none_or(|id| call.plugin.id == id)
                    && owner_session_id.is_none_or(|id| call.ownership.controller_id() == Some(id))
            })
            .cloned()
            .collect();
        self.observe_selected_formal_calls(&owner, calls).await
    }

    /// Drain the exact retained calls using owner's existing finite shutdown budget; returns original cleanup evidence.
    /// 使用 owner 既有有限关闭预算排空精确保留 calls；返回原清理证据。
    /// Selection belongs to the caller, so activation cancellation cannot widen into ordinary plugin work.
    /// 选择属于调用方，因此候选取消不能扩展至普通插件工作。
    pub(super) async fn drain_selected_formal_calls(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        calls: Vec<Arc<FormalCall>>,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        Ok(self
            .observe_selected_formal_calls(owner, calls)
            .await?
            .report)
    }

    /// Observe owner's exact calls once; return pending deadlines separately from terminal cleanup failures.
    /// 单次观测 owner 的精确 calls；将待完成截止时间与终态清理失败分别返回。
    async fn observe_selected_formal_calls(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        calls: Vec<Arc<FormalCall>>,
    ) -> Result<crate::runtime::drain::DrainObservation, VulcanPluginRuntimeError> {
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(owner.settings.shutdown_timeout_ms))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Formal drain deadline is out of range.")
            })?;
        for call in &calls {
            call.cancelled.store(true, Ordering::Release);
            self.retire_unstarted_formal_call(call)?;
        }
        // Keep actual failure classification beside the unchanged public diagnostics.
        // 在未变公开诊断旁保留实际失败分类。
        let mut observation = crate::runtime::drain::DrainObservation::default();
        for call in calls {
            loop {
                self.resume_formal_cleanup(owner, &call)?;
                let (retired, supervised, cleanup_error) = {
                    let state = call.lock()?;
                    (state.retired, state.supervised, state.cleanup_error.clone())
                };
                if retired && !supervised {
                    observation.report.closed_formal_calls += 1;
                    if let Some(error) = cleanup_error {
                        observation.failed(format!(
                            "Formal invocation {} shutdown failed: {error}",
                            call.id
                        ));
                    }
                    break;
                }
                if Instant::now() >= deadline {
                    let diagnostic = format!(
                        "Formal invocation {} has not physically retired; its identity and capacity remain retained.",
                        call.id
                    );
                    self.record_cleanup_pending(&call.plugin.id, diagnostic.clone())?;
                    observation.pending(diagnostic);
                    break;
                }
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
            }
        }
        self.reap_internal_calls(owner)?;
        Ok(observation)
    }

    /// Submit one legacy stateless tool to the formal parent using its exact owned payload and timeout.
    /// 使用精确有归属载荷与超时，将单个旧无状态工具提交到正式父级。
    /// `tool`, owner and workspace are host authority; return the original business value or queryable failure.
    /// `tool`、所有者及工作区属于宿主权威；返回原业务值或可查询失败。
    pub(crate) async fn invoke_formal_stateless(
        &self,
        tool: &VulcanPluginToolDescriptor,
        owner_session_id: String,
        workspace_root: PathBuf,
        payload: Value,
        timeout_ms: u64,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // Freeze the existing tool budget before constructing either independent envelope.
        // 在构造两个独立信封之前冻结现有工具预算。
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(timeout_ms))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin execution deadline is out of range.")
            })?;
        let owner = self.required_formal_owner()?;
        self.ensure_current_tool_generation(tool)?;
        let plugin = self
            .snapshot()
            .plugins
            .get(&tool.plugin_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin disappeared before formal admission.")
            })?;
        if plugin.generation != tool.plugin_generation {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin generation changed before formal admission.",
            ));
        }
        let manifest = plugin
            .manifest
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin has no valid manifest."))?;
        let runtime = super::super::lease::required_lua_runtime(manifest)?;
        // Count before allocating the larger envelope, then reserve every frozen input together.
        // 分配更大信封之前先计数，再共同预留全部冻结输入。
        luaskills::runtime::embedded::json_size(&payload, owner.settings.parent.max_value_bytes)
            .map_err(core_error)?;
        let input = Preparation {
            workspace: CallWorkspace::Provided(workspace_root),
            arguments: self.formal_envelope(payload, timeout_ms)?,
            finalizer: Some(ModuleFinalizer {
                export: runtime.shutdown_function.clone(),
                arguments: self.formal_envelope(
                    plugin_payload("shutdown", Map::new())?,
                    owner.settings.shutdown_timeout_ms,
                )?,
                timeout_ms: owner.settings.shutdown_timeout_ms,
            }),
            admission: CallAdmission::Controller {
                tool_id: tool.id.clone(),
                export: runtime.invoke_function.clone(),
                owner_session_id: owner_session_id.clone(),
            },
            scope: VulcanPluginBridgeLeaseScope::Enabled,
            security_partition: format!("tool:{owner_session_id}"),
            deadline,
        };
        self.invoke_owned_formal_call(owner, plugin, input).await
    }

    /// List retained records belonging to `owner_session_id`, without creating a runtime or changing state.
    /// 列出属于 `owner_session_id` 的保留记录，不创建运行时或改变状态。
    /// Returns bounded live observations, including preparation and actual cleanup.
    /// 返回有界实时观测，包含准备及真实清理。
    pub fn formal_calls(
        &self,
        owner_session_id: &str,
    ) -> Result<Vec<VulcanPluginFormalCallStatus>, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(Vec::new());
        };
        let calls: Vec<_> = owner
            .calls
            .lock()
            .map_err(|_| calls_error())?
            .values()
            .filter(|call| call.ownership.controller_id() == Some(owner_session_id))
            .cloned()
            .collect();
        calls.iter().map(|call| call.status()).collect()
    }

    /// Request cancellation of the exact `id` owned by `owner_session_id`; return no completion claim.
    /// 请求取消由 `owner_session_id` 拥有的精确 `id`；不返回完成声明。
    pub fn cancel_formal_call(
        &self,
        id: &str,
        owner_session_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let owner = self.required_formal_owner()?;
        let call = owned_call(&owner, id, owner_session_id)?;
        call.cancelled.store(true, Ordering::Release);
        self.retire_unstarted_formal_call(&call)?;
        Ok(())
    }

    /// Read exact `id` owned by `owner_session_id`; return live core evidence without changing ownership.
    /// 读取由 `owner_session_id` 拥有的精确 `id`；返回实时核心证据，不改变所有权。
    pub fn formal_call_status(
        &self,
        id: &str,
        owner_session_id: &str,
    ) -> Result<VulcanPluginFormalCallStatus, VulcanPluginRuntimeError> {
        let owner = self.required_formal_owner()?;
        owned_call(&owner, id, owner_session_id)?.status()
    }

    /// Acknowledge retired `id` owned by `owner_session_id`; forget exact core evidence before host removal.
    /// 确认由 `owner_session_id` 拥有且已退役的 `id`；先遗忘精确核心证据，再移除宿主记录。
    /// Busy or failed forgetting retains both identities for explicit retry.
    /// 忙碌或遗忘失败时保留两种身份供显式重试。
    pub fn forget_formal_call(
        &self,
        id: &str,
        owner_session_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let owner = self.required_formal_owner()?;
        let mut calls = owner.calls.lock().map_err(|_| calls_error())?;
        let call = calls
            .get(id)
            .filter(|call| call.ownership.controller_id() == Some(owner_session_id))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Owned formal invocation was not found.")
            })?;
        {
            let mut state = call.lock()?;
            if !state.retired || state.supervised || state.result.is_none() {
                return Err(VulcanPluginRuntimeError::new(
                    "Formal invocation cleanup has not completed.",
                ));
            }
            if let Some(target) = &mut state.prewarm {
                while let Some(operation) = target.completed.last() {
                    owner
                        .host
                        .runtime()
                        .forget_operation(operation.id())
                        .map_err(core_error)?;
                    target.completed.pop();
                }
            }
            if let Some(operation) = &state.operation {
                owner
                    .host
                    .runtime()
                    .forget_operation(operation.id())
                    .map_err(core_error)?;
                state.operation = None;
            }
        }
        calls.remove(id);
        Ok(())
    }

    /// Return this manager's already attached parent, or an explicit missing-runtime error.
    /// 返回此管理器已附加父级，或明确缺失运行时错误。
    fn required_formal_owner(&self) -> Result<Arc<ManagedFormalRuntime>, VulcanPluginRuntimeError> {
        self.formal_runtime_owner()?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal plugin runtime is not attached."))
    }

    /// Own preparation, core observation and cleanup for `call`; publish errors without replaying Lua.
    /// 拥有 `call` 的准备、核心观测及清理；发布错误而不重放 Lua。
    /// Input payload and deadlines are frozen at admission; this detached supervisor returns no business value.
    /// 输入载荷及截止时间在入场时冻结；此分离监督者不返回业务值。
    async fn supervise_formal_call(
        &self,
        owner: Arc<ManagedFormalRuntime>,
        call: Arc<FormalCall>,
        mut input: Preparation,
    ) {
        // Keep the same manager and call alive through the actual blocking closure.
        // 在真实阻塞闭包期间保持同一管理器及调用存活。
        let manager = self.clone();
        // The worker retains the original formal parent even if its join observer is lost.
        // 即使等待观测者丢失，工作者仍保留原正式父级。
        let prepare_owner = Arc::clone(&owner);
        // Publish preparation against the exact original record.
        // 在精确原记录上发布准备状态。
        let prepare_call = Arc::clone(&call);
        if let Ok(mut state) = call.lock() {
            state.preparing = true;
        } else {
            return;
        }
        // This guard exists before queueing and follows physical preparation completion.
        // 此守卫在排队前存在，并跟随物理准备完成。
        let preparation_guard = PreparationGuard {
            call: Arc::clone(&call),
        };
        // Return remaining typed admission input without duplicating the retained business payload.
        // 返回剩余类型化入场输入，不复制保留的业务载荷。
        let prepared = tokio::task::spawn_blocking(move || {
            // Keep preparation marked live until this exact closure exits.
            // 直到此精确闭包退出前，保持准备处于存活状态。
            let _preparation_guard = preparation_guard;
            // Preserve the real preparation result beside its original unconsumed admission input.
            // 将真实准备结果与其原未消费入场输入共同保留。
            let outcome = manager.prepare_formal_call(&prepare_owner, &prepare_call, &mut input);
            (outcome, input)
        })
        .await;
        // Only normal preparation may proceed from initialization into a first business call.
        // 仅正常准备可以从初始化进入首次业务调用。
        let preparation = match prepared {
            Ok((Ok(()), input)) => {
                self.complete_initialized_admission(&owner, &call, input)
                    .await
            }
            Ok((Err(error), _)) => Err(error),
            Err(error) => Err(VulcanPluginRuntimeError::new(format!(
                "Formal preparation task failed: {error}"
            ))),
        };
        // Retirement always follows the original identities, including failed or cancelled initialization.
        // 退役始终跟随原身份，包含失败或取消的初始化。
        let result = self.finish_formal_call(&owner, &call, preparation).await;
        if let Ok(mut state) = call.lock() {
            state.result = Some(result);
        }
    }

    /// Freeze exact module inputs and submit once; all filesystem work precedes core execution admission.
    /// 冻结精确模块输入并提交一次；全部文件系统工作先于核心执行入场。
    /// `deadline` includes preparation time; errors never cause a second business submission.
    /// `deadline` 包含准备时间；错误绝不导致第二次业务提交。
    fn prepare_formal_call(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
        input: &mut Preparation,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if call.cancelled.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(
                "Formal invocation cancelled before preparation.",
            ));
        }
        if let CallAdmission::Action { execution, .. } = &input.admission {
            execution.record.check()?;
        }
        if Instant::now() >= input.deadline {
            return Err(VulcanPluginRuntimeError::new(
                "Formal invocation deadline expired before preparation.",
            ));
        }
        // Blocking workspace preparation belongs to the already-published invocation, never its waiter.
        // 阻塞工作区准备属于已发布调用，绝不属于其等待者。
        match &input.workspace {
            CallWorkspace::LifecycleEnvironment => {
                self.prepare_lifecycle_environment(owner, call, input)?
            }
            CallWorkspace::ExecutionPackage => {
                // Resolve exactly the descriptor pinned before publication and retain its full path budget.
                // 精确解析发布前固定的描述符，并保留其完整路径预算。
                let workspace = crate::runtime::environment::plugin_execution_package_root(
                    &self.state,
                    &call.plugin,
                )
                .map_err(VulcanPluginRuntimeError::new)?;
                input.workspace = CallWorkspace::Provided(workspace);
                self.reserve_prepared_input(owner, call, input)?;
            }
            CallWorkspace::Provided(_) => {}
        }
        // Every route now has one explicit workspace; no alternate directory is probed.
        // 每条路由现在都有一个明确工作区；不探测备用目录。
        let CallWorkspace::Provided(workspace) = &input.workspace else {
            return Err(VulcanPluginRuntimeError::new(
                "Formal preparation produced no workspace.",
            ));
        };
        // Reuse is selected only by the exact validated stateless tool entry.
        // 复用仅由精确已校验无状态工具入口选择。
        let reusable = match &input.admission {
            CallAdmission::Controller { tool_id, .. }
            | CallAdmission::PrewarmTarget {
                request: VulcanPluginPrewarmTargetRequest { tool_id, .. },
                ..
            } => {
                let manifest = call.plugin.manifest.as_ref().ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Formal tool manifest is missing.")
                })?;
                let runtime = super::super::lease::required_lua_runtime(manifest)?;
                let tool = runtime
                    .tools
                    .iter()
                    .find(|tool| &tool.id == tool_id)
                    .ok_or_else(|| {
                        VulcanPluginRuntimeError::new("Formal tool declaration is missing.")
                    })?;
                tool.execution
                    .reusable
                    .clone()
                    .map(|reuse| (tool_id.clone(), reuse))
            }
            _ => None,
        };
        if matches!(input.admission, CallAdmission::PrewarmTarget { .. }) && reusable.is_none() {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarming requires a declared reusable tool.",
            ));
        }
        let separate = input.admission.separates_initialization();
        let pool = if let Some((tool, reuse)) = reusable {
            self.prepare_reusable_call(owner, call, input, &tool, &reuse)?
        } else {
            // Retain exact purpose capacity through this real preparation and original core registration.
            // 在此真实准备及原核心注册期间保留精确用途容量。
            let capacity = {
                // Serialize the first registration and all later verification with host publication.
                // 将首次注册及全部后续核验与宿主发布排序。
                let publication = self.state.session_registration_gate.lock().map_err(|_| {
                    VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
                })?;
                self.validate_formal_submission(call, input.scope)?;
                owner.admit_capacity_member(
                    &call.plugin.id,
                    input.admission.capacity_purpose(),
                    &publication,
                )?
            };
            // Temporary sessions separate initialization from business while remaining private to this one call.
            // 临时会话将初始化与业务分离，同时仍为此单次调用私有。
            if separate && input.finalizer.is_some() {
                return Err(VulcanPluginRuntimeError::new(
                    "Action and probe sessions cannot select a business finalizer.",
                ));
            }
            // Register the original bundle before recording any actual initialization or business operation.
            // 在记录任何实际初始化或业务操作前登记原 bundle。
            let pool = owner
                .host
                .register_call_bundle_pool_in_capacity(
                    &capacity.capacity_id,
                    VulcanPluginBundlePoolRequest {
                        plugin_id: call.plugin.id.clone(),
                        plugin_generation: call.plugin.generation,
                        workspace_root: workspace.clone(),
                        security_partition: input.security_partition.clone(),
                        scope: input.scope,
                        policy: PluginPoolConfig {
                            kind: capacity.config.resources.kind,
                            min_resident_vms: 0,
                            max_resident_vms: 1,
                            max_running_calls: 1,
                            max_queued_calls: capacity.config.max_queued_calls,
                            reuse: if separate {
                                InstanceReuse::Session
                            } else {
                                InstanceReuse::SingleCall
                            },
                            serial: true,
                            backend: VULCAN_PLUGIN_EXECUTION_BACKEND,
                            idle_ttl_ms: None,
                            max_uses: None,
                        },
                        finalizer: input.finalizer.take(),
                    },
                    call.source.clone(),
                )
                .map_err(core_error)?;
            call.lock()?.pool = Some(pool.clone());
            pool
        };
        // No catalog publication may interleave this final generation check and core admission.
        // 任何目录发布都不能插入此次最终代次检查与核心入场之间。
        let _publication =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        self.validate_formal_submission(call, input.scope)?;
        self.validate_reusable_submission(owner, call)?;
        if let CallAdmission::Action { execution, .. } = &input.admission {
            execution.record.check()?;
        }
        // Initialization and later business share the same original absolute deadline.
        // 初始化及后续业务共用同一原绝对截止时间。
        let remaining = initialized::remaining(input.deadline)?;
        if matches!(input.admission, CallAdmission::PrewarmTarget { .. }) {
            // The target supervisor queries this exact borrowed domain before deciding whether initialization is needed.
            // 目标监督者先查询此精确借用域，再决定是否需要初始化。
            self.claim_prewarm_domain(owner, call)?;
        } else if separate {
            // Initialization receives its own core identity without invoking the Action export.
            // 初始化获得自身核心身份，不调用 Action 导出。
            let opening = owner
                .host
                .runtime()
                .open_session(&pool, remaining)
                .map_err(core_error)?;
            // Separate initialization keeps its own Core operation/session identity.
            // 独立初始化保持其自身 Core 操作／会话身份。
            crate::runtime::native_identity::observe_operation(
                owner.host.runtime().id(),
                &opening.operation,
                call.native_identity_owner(),
            );
            call.lock()?.initialized = Some(initialized::InitializedCall::new(
                opening.session_id,
                opening.operation,
            ));
        } else {
            // Core admission owns ordering for both exclusive and explicitly reusable tool domains.
            // 核心入场拥有独占及显式可复用工具域的顺序。
            let export = input.admission.export().ok_or_else(|| {
                VulcanPluginRuntimeError::new("Formal business admission has no declared export.")
            })?;
            // The returned handle is the sole actual business identity.
            // 返回句柄为唯一真实业务身份。
            let operation = owner
                .host
                .runtime()
                .submit(
                    EmbeddedCall {
                        pool_id: pool,
                        export: export.to_owned(),
                        arguments: std::mem::take(&mut input.arguments),
                        context: LuaInvocationContext::default(),
                    },
                    remaining,
                )
                .map_err(core_error)?;
            // Publish original identity while the admitted handle is still available.
            // 已接纳句柄仍可取得时发布原身份。
            crate::runtime::native_identity::observe_operation(
                owner.host.runtime().id(),
                &operation,
                call.native_identity_owner(),
            );
            call.lock()?.operation = Some(operation);
        }
        Ok(())
    }

    /// Add unique host correlation and `timeout_ms` to an already validated version-one payload.
    /// 向已校验版本一载荷加入唯一宿主关联及 `timeout_ms`。
    /// Return the exact bridge envelope; exhausted identities fail instead of wrapping.
    /// 返回精确桥接信封；身份耗尽时失败而不回绕。
    pub(super) fn formal_envelope(
        &self,
        mut payload: Value,
        timeout_ms: u64,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let request_id = self
            .state
            .next_request_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin request identity is exhausted."))?;
        let object = payload
            .as_object_mut()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin payload must be an object."))?;
        object.insert("request_id".to_string(), json!(request_id.to_string()));
        object.insert("host_timeout_ms".to_string(), json!(timeout_ms));
        Ok(json!({"payload_json":payload.to_string(),"host_timeout_ms":timeout_ms}))
    }

    /// Observe original core ownership, then retire the exact pool before releasing the host slot.
    /// 观测原核心所有权，然后在释放宿主槽位前退役精确池。
    /// `preparation` preserves a pre-submission failure; returns the independent business/closing projection.
    /// `preparation` 保留提交前失败；返回独立业务及关闭投影。
    async fn finish_formal_call(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
        preparation: Result<(), VulcanPluginRuntimeError>,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let operation = call.lock()?.operation.clone();
        let result = if let Some(operation) = operation {
            let mut cancellation_sent = false;
            let terminal = loop {
                if call.cancelled.load(Ordering::Acquire) && !cancellation_sent {
                    operation.cancel().map_err(core_error)?;
                    cancellation_sent = true;
                }
                let snapshot = operation.snapshot().map_err(core_error)?;
                if snapshot.phase.is_terminal() {
                    break snapshot;
                }
                // Await the same original operation directly, retaining finite cancellation checks without a blocking task.
                // 直接等待同一原操作，保留有限取消检查且不创建阻塞任务。
                let observed = observe_terminal(&operation).await?;
                if observed.phase.is_terminal() {
                    break observed;
                }
            };
            if let Some(finalization) = &terminal.finalization
                && let Some(Err(error)) = finalization
                    .outcome
                    .as_ref()
                    .map(|outcome| outcome.result())
            {
                call.lock()?.cleanup_error = Some(error.to_string());
                self.record_cleanup_failure(
                    &call.plugin.id,
                    format!("Formal invocation {} shutdown failed: {error}", call.id),
                )?;
            }
            project_result(&terminal)
        } else if call.lock()?.prewarm.is_some() {
            preparation
                .clone()
                .and_then(|()| self.prewarm_target_result(call))
        } else if matches!(call.ownership, CallOwner::Probe) {
            preparation.clone().map(|()| Value::Bool(true))
        } else {
            preparation.clone().and_then(|()| {
                Err(VulcanPluginRuntimeError::new(
                    "Formal submission produced no operation identity.",
                ))
            })
        };
        // Target completion preserves preparation failures even when the last successful initialization still has an operation.
        // 即使最后成功初始化仍有操作，目标完成也保留准备失败。
        let result = if call.lock()?.prewarm.is_some() {
            preparation
                .and(result)
                .and_then(|_| self.prewarm_target_result(call))
        } else {
            result
        };
        if let Some(work) = &call.lock()?.action_work {
            work.record().phase("cleanup");
        }
        self.close_initialized_call(owner, call).await?;
        owner.host.reap_workspace_requests().map_err(core_error)?;
        // Release only the request borrow; the reusable registry owns idle VM retirement.
        // 仅释放请求借用；可复用注册表拥有空闲 VM 退役。
        self.release_reusable_call(owner, call)?;
        let pool = call.lock()?.pool.clone();
        if let Some(pool) = pool {
            owner.host.runtime().close_pool(&pool).map_err(core_error)?;
            loop {
                match owner.host.runtime().forget_pool(&pool) {
                    Ok(()) => {
                        call.lock()?.pool = None;
                        break;
                    }
                    Err(error) if error.code == EmbeddedErrorCode::Busy => {
                        tokio::time::sleep(OBSERVATION_INTERVAL).await
                    }
                    Err(error) => return Err(core_error(error)),
                }
            }
        }
        if call
            .ownership
            .retains_published_slot(call.source.is_private())
        {
            self.release_session_slot_checked(&call.plugin.id)?;
        }
        // Failed preparation can retain a lease that never reached an actual pool.
        // 准备失败可能保留从未进入实际池的租约。
        if let CallExecutionSource::RemovalLifecycle(authority) = &call.source {
            authority.retire_prepared_environment()?;
        }
        // Physical Action capacity outlives every core session, request scope and pool checkpoint.
        // 物理 Action 容量比全部核心会话、请求作用域及池检查点存活更久。
        let mut state = call.lock()?;
        state.retired = true;
        state.action_work.take();
        result
    }
}

/// Preserve native prewarm results or convert business and finalization outcomes to the legacy result shape.
/// 保留原生预热结果，或将业务及关闭结果转换为旧结果形状。
/// `snapshot` must be terminal; preserve both failure messages without replay or fallback parsing.
/// `snapshot` 必须为终态；保留两阶段失败消息，不重放或回退解析。
fn project_result(snapshot: &OperationSnapshot) -> Result<Value, VulcanPluginRuntimeError> {
    // Borrow retained business frames synchronously; successful closing values carry no projected payload.
    // 同步借用保留的业务帧；成功关闭值不携带需要投影的载荷。
    let (business, closing) = if let Some(finalization) = &snapshot.finalization {
        (
            match &finalization.business {
                OperationOutcome::Succeeded { value } => Ok(value),
                OperationOutcome::Failed { error } => Err(error.clone()),
            },
            match finalization.outcome.as_ref().ok_or_else(|| {
                VulcanPluginRuntimeError::new("Terminal finalization has no outcome.")
            })? {
                OperationOutcome::Succeeded { .. } => Ok(()),
                OperationOutcome::Failed { error } => Err(error.clone()),
            },
        )
    } else {
        (
            match (&snapshot.value, &snapshot.error) {
                (Some(value), None) => Ok(value),
                (None, Some(error)) => Err(error.clone()),
                _ => {
                    return Err(VulcanPluginRuntimeError::new(
                        "Terminal operation has inconsistent result evidence.",
                    ));
                }
            },
            Ok(()),
        )
    };
    // Only the core's frozen explicit prewarm context selects its native instance result contract.
    // 仅核心冻结的明确预热上下文选择其原生实例结果契约。
    let business = business.map_err(core_error).and_then(|frame| {
        if matches!(
            &snapshot.context,
            OperationContext::Module(context) if context.prewarm
        ) {
            Ok(frame.clone())
        } else {
            super::super::lease::parse_plugin_result(frame)
        }
    });
    match (business, closing.map_err(core_error)) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(business), Err(closing)) => Err(VulcanPluginRuntimeError::new(format!(
            "{business}; {closing}"
        ))),
    }
}

/// Resolve exactly `id` under its declared controller owner; reject foreign or forgotten records.
/// 在声明控制器所有者下精确解析 `id`；拒绝外来或已遗忘记录。
fn owned_call(
    owner: &ManagedFormalRuntime,
    id: &str,
    owner_session_id: &str,
) -> Result<Arc<FormalCall>, VulcanPluginRuntimeError> {
    owner
        .calls
        .lock()
        .map_err(|_| calls_error())?
        .get(id)
        .filter(|call| call.ownership.controller_id() == Some(owner_session_id))
        .cloned()
        .ok_or_else(|| VulcanPluginRuntimeError::new("Owned formal invocation was not found."))
}

/// Preserve the structured core diagnostic as the existing host error type.
/// 将结构化核心诊断保留为既有宿主错误类型。
pub(super) fn core_error(
    error: luaskills::runtime::embedded::EmbeddedError,
) -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new(error.to_string())
}

/// Report unavailable host ownership without inventing an empty registry.
/// 报告宿主所有权不可用，不伪造空注册表。
fn calls_error() -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new("Formal invocation registry lock is poisoned.")
}
