//! Production persistent ownership spans preparation, fixed Lua state and physical retirement.
//! 生产持久所有权覆盖准备、固定 Lua 状态及物理退役。

use super::calls::{OBSERVATION_INTERVAL, core_error};
use super::pinned::{deadline, project_business, project_closing, remaining};
use super::*;
use crate::{VulcanPluginDescriptor, VulcanPluginHookDescriptor};
use luaskills::runtime::embedded::{
    EmbeddedCall, EmbeddedErrorCode, EmbeddedSessionPhase, InstanceReuse, ModuleFinalizer,
    OperationHandle, OperationSnapshot, PluginPoolConfig,
};
use luaskills::{LuaInvocationContext, RuntimeRequestContext};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Instant;

mod admission;
mod control;
#[cfg(test)]
mod tests;

/// One plugin generation and purpose own one ordered persistent instance, independently of tool session limits.
/// 一个插件代次及用途拥有一个有序持久实例，独立于工具会话上限。
pub(in crate::runtime) struct PersistentRuntime {
    /// The declared purpose selects isolated admission and optional finalization.
    /// 已声明用途选择独立入场及可选关闭。
    kind: VulcanPluginPersistentRuntimeKind,
    /// UI cancellation admission is serialized until actual operation completion.
    /// UI 取消入场保持串行，直至实际操作完成。
    pub(in crate::runtime) operation_gate: Arc<tokio::sync::Mutex<()>>,
    /// Immutable descriptor retains the original executable package.
    /// 不可变描述符保留原可执行包。
    plugin: VulcanPluginDescriptor,
    /// Closing intent never proves physical termination.
    /// 关闭意图绝不证明物理终止。
    closing: AtomicBool,
    /// Short evidence lock never spans Lua execution or asynchronous waiting.
    /// 短证据锁绝不跨越 Lua 执行或异步等待。
    state: Mutex<PersistentState>,
}

/// Exact checkpoints permit recovery without replaying initialization.
/// 精确检查点允许恢复，且不会重放初始化。
#[derive(Default)]
struct PersistentState {
    /// The initiating action owns preparation until real initialization finishes.
    /// 发起 Action 持有准备，直至实际初始化完成。
    preparation_action: Option<crate::runtime::action_status::ActionWorkGuard>,
    /// Actual blocking preparation owns this flag until its closure ends.
    /// 实际阻塞准备持有此标志至闭包结束。
    preparing: bool,
    /// A missing monitor requires explicit restart on the retained record.
    /// 缺失监控要求在保留记录上显式重启。
    monitored: bool,
    /// Preparation completion differs from core initialization completion.
    /// 准备完成不同于核心初始化完成。
    prepared: Option<Result<(), VulcanPluginRuntimeError>>,
    /// Original package workspace also scopes independent finalization.
    /// 原包工作区同时限定独立关闭。
    workspace: Option<PathBuf>,
    /// Registered pool identity is recorded immediately after registration.
    /// 注册池身份在注册后立即记录。
    pool: Option<String>,
    /// Fixed session remains owned until physical closure.
    /// 固定会话保持所属关系至物理关闭。
    session: Option<String>,
    /// Initialization evidence survives a lost preparation waiter.
    /// 初始化证据在准备等待者离开后存活。
    initialization: Option<OperationHandle>,
    /// Business evidence awaits both actual completion and observer acknowledgement.
    /// 业务证据等待真实完成及观测者确认。
    operations: BTreeMap<String, PersistentOperation>,
    /// Independent shutdown outcome is copied before core evidence is forgotten.
    /// 独立关闭结果在核心证据遗忘前复制。
    finalization: Option<OperationSnapshot>,
    /// Successful session forgetting must not be repeated.
    /// 不得重复已成功的会话遗忘。
    session_forgotten: bool,
    /// Initialization evidence was explicitly acknowledged.
    /// 初始化证据已明确确认。
    initialization_forgotten: bool,
    /// Finalization evidence was explicitly acknowledged.
    /// 关闭证据已明确确认。
    finalization_forgotten: bool,
    /// Preparation, operations, session and pool have physically retired.
    /// 准备、操作、会话及池已物理退役。
    retired: bool,
    /// Failed cleanup remains visible until an explicit retry.
    /// 清理失败保持可见至显式重试。
    cleanup_error: Option<String>,
}

/// Acknowledgement is independent from cancellation and actual termination.
/// 确认独立于取消及真实终止。
struct PersistentOperation {
    /// Exact operation submitted once to the original session.
    /// 仅向原会话提交一次的精确操作。
    handle: OperationHandle,
    /// Observer acknowledgement permits later evidence reclamation.
    /// 观测者确认允许后续证据回收。
    acknowledged: Arc<AtomicBool>,
}

/// Interrupted observation requests cancellation while the registry retains physical ownership.
/// 中断观测请求取消，而注册表保留物理所有权。
struct PersistentObservation {
    /// Actual operation never comes from a mutable request payload.
    /// 真实操作绝不来自可变请求载荷。
    handle: OperationHandle,
    /// Shared acknowledgement survives executor cancellation.
    /// 共享确认在执行器取消后存活。
    acknowledged: Arc<AtomicBool>,
    /// Delivered terminal results disarm implicit cancellation.
    /// 已交付终态结果解除隐式取消。
    delivered: bool,
}

impl Drop for PersistentObservation {
    /// Cancel abandoned work and acknowledge observation without releasing running work.
    /// 取消放弃的工作并确认观测，不释放运行中工作。
    fn drop(&mut self) {
        if !self.delivered {
            let _ = self.handle.cancel();
        }
        self.acknowledged.store(true, Ordering::Release);
    }
}

/// Preparation ownership exists before a queued blocking closure starts.
/// 排队阻塞闭包开始前，准备所有权已存在。
struct PersistentPreparation(Arc<PersistentRuntime>);

impl Drop for PersistentPreparation {
    /// Publish interruption only when the actual closure did not publish its result.
    /// 仅在实际闭包未发布自身结果时发布中断。
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock()
            && state.preparing
        {
            state.prepared = Some(Err(instance_error(
                "persistent preparation was interrupted.",
            )));
            state.preparing = false;
            self.0.closing.store(true, Ordering::Release);
        }
    }
}

/// Dropping supervision never releases core resources.
/// 丢弃监督绝不释放核心资源。
struct PersistentMonitor(Arc<PersistentRuntime>);

impl Drop for PersistentMonitor {
    /// Preserve interrupted cleanup for the next explicit drain.
    /// 为下一次显式排空保留中断清理。
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.monitored = false;
        }
    }
}

impl PersistentRuntime {
    /// Return the exact state guard or an explicit poison error.
    /// 返回精确状态守卫或明确中毒错误。
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, PersistentState>, VulcanPluginRuntimeError> {
        self.state
            .lock()
            .map_err(|_| instance_error("persistent evidence lock is poisoned."))
    }
}

/// Return an explicit host error without substituting another execution path.
/// 返回明确宿主错误，不替换为另一执行路径。
fn instance_error(message: &str) -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new(message)
}

impl ManagedFormalRuntime {
    /// Select the exact kind's registry; return a borrowed mutex without locking it.
    /// 选择精确 kind 的注册表；返回借用互斥锁，不加锁。
    fn instances(
        &self,
        kind: VulcanPluginPersistentRuntimeKind,
    ) -> &Mutex<BTreeMap<String, Arc<PersistentRuntime>>> {
        match kind {
            VulcanPluginPersistentRuntimeKind::Hook => &self.hooks,
            VulcanPluginPersistentRuntimeKind::UiAction => &self.ui_actions,
        }
    }
}
