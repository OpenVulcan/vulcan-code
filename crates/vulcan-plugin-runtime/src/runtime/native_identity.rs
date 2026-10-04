//! Optional process-wide observation of original native identities, without retaining execution owners.
//! 可选的进程级原生身份观测，不保留执行所有者。

use super::VulcanPluginRuntimeError;
use luaskills::runtime::embedded::{
    EmbeddedErrorCode, OperationContext, OperationHandle, OperationSnapshot,
};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// Immutable whitelist copied from original pool registration or admitted operation authority.
/// 从原池注册或已接纳操作权威复制的不可变白名单。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum VulcanPluginNativeIdentityEvent {
    /// Exact pool identity survives later catalog publication through the consumer's bounded log window.
    /// 精确池身份通过消费者的有界日志窗口跨越后续目录发布。
    PoolRegistered {
        /// Original Core runtime namespace.
        /// 原 Core 运行时命名空间。
        runtime_id: String,
        /// Original Core-issued pool identity.
        /// 原 Core 签发池身份。
        pool_id: String,
        /// Plugin identity from the frozen execution authority.
        /// 来自冻结执行权威的插件身份。
        plugin_id: String,
        /// Original numeric host generation.
        /// 原宿主数字代次。
        plugin_generation: u64,
        /// Exact Core package-generation spelling.
        /// 精确 Core 包代次拼写。
        package_generation: String,
        /// Original immutable execution revision.
        /// 原不可变执行修订。
        execution_revision: String,
    },
    /// Original admitted operation authority plus its separate host owner identities.
    /// 原已接纳操作权威及其分别保存的宿主所有者身份。
    OperationBound {
        /// Original Core runtime namespace.
        /// 原 Core 运行时命名空间。
        runtime_id: String,
        /// Original Core-issued pool identity.
        /// 原 Core 签发池身份。
        pool_id: String,
        /// Original Core-issued operation identity.
        /// 原 Core 签发操作身份。
        operation_id: String,
        /// Plugin identity frozen in ModuleOperationContext.
        /// ModuleOperationContext 中冻结的插件身份。
        plugin_id: String,
        /// Original numeric host generation from the retained owner.
        /// 来自保留所有者的原宿主数字代次。
        plugin_generation: u64,
        /// Original Core package-generation spelling.
        /// 原 Core 包代次拼写。
        package_generation: String,
        /// Original immutable execution revision.
        /// 原不可变执行修订。
        execution_revision: String,
        /// Actual Core session; prewarm and ordinary reusable calls have no fixed session.
        /// 实际 Core 会话；预热及普通复用调用没有固定会话。
        core_session_id: Option<String>,
        /// Original controller owner; internal operations have none.
        /// 原控制器所有者；内部操作为空。
        controller_session_id: Option<String>,
        /// FormalCall identity only; never substituted by a controller or Core session.
        /// 仅 FormalCall 身份；绝不以控制器或 Core 会话替代。
        host_invocation_id: Option<String>,
        /// Pinned host session only; persistent Hook/UI ownership does not invent one.
        /// 仅固定宿主会话；持久 Hook/UI 所有权不虚构此值。
        host_session_id: Option<String>,
        /// Actual optional trusted Core request; never parsed from arguments or partitions.
        /// 实际可选可信 Core 请求；绝不从参数或分区解析。
        trusted_request_id: Option<String>,
        /// Explicit Core prewarm admission flag.
        /// 明确的 Core 预热入场标记。
        prewarm: bool,
        /// Actual independent closing instance; never attributed to the last business request.
        /// 实际独立关闭实例；绝不归给最后业务请求。
        finalization_instance_id: Option<String>,
    },
    /// A known operation could not expose its authority; the capture window is incomplete.
    /// 已知操作无法暴露权威；采集窗口不完整。
    ObservationUnavailable {
        /// Already-known original runtime identity.
        /// 已经确定的原运行时身份。
        runtime_id: String,
        /// Already-known original operation identity.
        /// 已经确定的原操作身份。
        operation_id: String,
        /// Safe Core error code; unbound context has no fabricated error.
        /// 安全 Core 错误码；未绑定上下文不伪造错误。
        error_code: Option<EmbeddedErrorCode>,
    },
}

/// Metadata-only callback; implementations must hand off without blocking or reentering runtime control.
/// 仅元数据回调；实现必须非阻塞交接，且不得重入运行时控制。
pub type VulcanPluginNativeIdentityCallback =
    Arc<dyn Fn(VulcanPluginNativeIdentityEvent) + Send + Sync + 'static>;

/// The service guard alone retains the callback; native pools and history never do.
/// 仅服务守卫保留回调；原生池与历史绝不保留。
struct Sink {
    /// Service-provided nonblocking metadata consumer.
    /// 服务提供的非阻塞元数据消费者。
    callback: VulcanPluginNativeIdentityCallback,
    /// Capture errors and sink panics are reported at the service capture boundary.
    /// 采集错误及接收器 panic 在服务采集边界报告。
    failures: AtomicU64,
}

/// Process-wide weak slot lets every future manager and parent share the same service subscription.
/// 进程级弱槽使后续每个管理器及父级共享相同服务订阅。
static OBSERVER: OnceLock<Mutex<Option<Weak<Sink>>>> = OnceLock::new();

/// Unique service lifetime guard; dropping it removes only this exact subscription.
/// 唯一服务寿命守卫；丢弃时仅移除此精确订阅。
pub struct VulcanPluginNativeIdentitySubscription {
    /// The sole strong service owner; callback snapshots may temporarily retain it during emission.
    /// 唯一强服务所有者；发送期间回调快照可临时保留它。
    sink: Arc<Sink>,
}

impl VulcanPluginNativeIdentitySubscription {
    /// Detach this exact observer while retaining its guard for tail observation; return no business result.
    /// 撤销此精确观察器，并保留守卫用于尾部观测；不返回业务结果。
    /// Snapshot upgrades are serialized with detachment, so pending_observations reaches zero after all earlier emissions finish.
    /// 快照升级与撤销串行，因此全部较早发送结束后 pending_observations 到达零。
    pub fn detach(&self) {
        // Recover an observational lock error without allowing shutdown to alter business ownership.
        // 恢复观测锁错误，不允许关闭改变业务所有权。
        let mut slot = OBSERVER
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|error| {
                self.sink.failures.fetch_add(1, Ordering::AcqRel);
                error.into_inner()
            });
        if slot
            .as_ref()
            .is_some_and(|original| original.ptr_eq(&Arc::downgrade(&self.sink)))
        {
            *slot = None;
        }
    }

    /// Return outstanding original observer snapshots; after detach, zero proves identity emissions have exited.
    /// 返回尚未退出的原观察器快照数；撤销后零值证明身份发送已经退出。
    pub fn pending_observations(&self) -> usize {
        Arc::strong_count(&self.sink).saturating_sub(1)
    }

    /// Return cumulative observation errors and caught sink panics for this capture window.
    /// 返回此采集窗口累计观测错误及已捕获接收器 panic。
    /// Read at the capture boundary; zero does not prove queue delivery or file persistence.
    /// 在采集边界读取；零值不能证明队列交付或文件持久化。
    pub fn observation_failures(&self) -> u64 {
        self.sink.failures.load(Ordering::Acquire)
    }
}

impl Drop for VulcanPluginNativeIdentitySubscription {
    /// Clear only the matching weak slot; an already executing callback snapshot may finish afterward.
    /// 仅清除匹配的弱槽；已经执行的回调快照仍可在之后结束。
    fn drop(&mut self) {
        self.detach();
    }
}

/// Install `callback` once before native admission; reject replacement of a live service owner.
/// 在原生入场前一次性安装 `callback`；拒绝替换活动服务所有者。
/// Return the unique lifetime guard or an explicit configuration error; this does not install the Core logger.
/// 返回唯一寿命守卫或明确配置错误；此接口不安装 Core 日志器。
pub fn install_native_identity_observer(
    callback: VulcanPluginNativeIdentityCallback,
) -> Result<VulcanPluginNativeIdentitySubscription, VulcanPluginRuntimeError> {
    // Installation errors remain visible before any formal work is admitted.
    // 任何正式工作入场之前，安装错误保持可见。
    let mut slot = OBSERVER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| VulcanPluginRuntimeError::new("Native identity observer lock is poisoned."))?;
    if slot.as_ref().and_then(Weak::upgrade).is_some() {
        return Err(VulcanPluginRuntimeError::new(
            "Native identity observer is already installed.",
        ));
    }
    // Only this guard owns a strong subscription; the global slot cannot retain a service or manager.
    // 仅此守卫拥有强订阅；全局槽不能保留服务或管理器。
    let sink = Arc::new(Sink {
        callback,
        failures: AtomicU64::new(0),
    });
    *slot = Some(Arc::downgrade(&sink));
    Ok(VulcanPluginNativeIdentitySubscription { sink })
}

/// Borrowed immutable host ownership; all optional identities preserve their real separate meanings.
/// 借用的不可变宿主所有权；全部可选身份保持真实独立含义。
#[derive(Clone, Copy)]
pub(super) struct NativeIdentityOwner<'a> {
    /// Original numeric descriptor or reusable-domain generation.
    /// 原数字描述符或复用域代次。
    pub plugin_generation: u64,
    /// Actual retained controller ownership.
    /// 实际保留控制器所有权。
    pub controller_session_id: Option<&'a str>,
    /// Actual FormalCall identity, when present.
    /// 存在时的实际 FormalCall 身份。
    pub host_invocation_id: Option<&'a str>,
    /// Actual PinnedSession identity, when present.
    /// 存在时的实际 PinnedSession 身份。
    pub host_session_id: Option<&'a str>,
}

/// Snapshot the weak subscription without keeping its lock during metadata construction or callback execution.
/// 取得弱订阅快照，构造元数据及执行回调期间不保留其锁。
/// Return none when disabled; poison is recovered with a counted error on the original live sink.
/// 禁用时返回空；锁污染恢复时在原活动接收器计入错误。
fn sink() -> Option<Arc<Sink>> {
    // A disabled observer never allocates even its global mutex.
    // 禁用观察器连全局互斥锁也不分配。
    let slot = OBSERVER.get()?;
    match slot.lock() {
        Ok(slot) => slot.as_ref().and_then(Weak::upgrade),
        Err(error) => {
            // The retained weak identity is authoritative even when the observational lock is poisoned.
            // 即使观测锁被污染，保留弱身份仍然是权威。
            let sink = error.into_inner().as_ref().and_then(Weak::upgrade);
            if let Some(sink) = &sink {
                sink.failures.fetch_add(1, Ordering::AcqRel);
            }
            sink
        }
    }
}

impl Sink {
    /// Deliver one safe `event`; count panic without changing any business result or recursively calling the sink.
    /// 交付单个安全 `event`；计入 panic，不改变业务结果且不递归调用接收器。
    fn emit(&self, event: VulcanPluginNativeIdentityEvent) {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.callback)(event)))
            .is_err()
        {
            self.failures.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// Copy whitelist metadata from borrowed `snapshot` and `owner`; return no execution handles or business JSON.
    /// 从借用 `snapshot` 及 `owner` 复制白名单元数据；不返回执行句柄或业务 JSON。
    fn operation(
        &self,
        runtime_id: &str,
        snapshot: &OperationSnapshot,
        owner: NativeIdentityOwner<'_>,
    ) {
        // Unbound work is explicit missing module authority, never a guessed current plugin.
        // 未绑定工作表示明确缺失模块权威，绝不猜测当前插件。
        let OperationContext::Module(context) = &snapshot.context else {
            self.unavailable(runtime_id, &snapshot.operation_id, None);
            return;
        };
        // Only frozen caller fields are copied; context workspace, partition, effects and value stay private.
        // 仅复制冻结调用方字段；上下文工作区、分区、副作用及值保持私有。
        let caller = &context.caller;
        self.emit(VulcanPluginNativeIdentityEvent::OperationBound {
            runtime_id: caller.runtime_id.clone(),
            pool_id: context.pool_id.clone(),
            operation_id: caller.operation_id.clone(),
            plugin_id: caller.plugin_id.clone(),
            plugin_generation: owner.plugin_generation,
            package_generation: caller.package_generation.clone(),
            execution_revision: caller.execution_revision.clone(),
            core_session_id: caller.session_id.clone(),
            controller_session_id: owner.controller_session_id.map(str::to_owned),
            host_invocation_id: owner.host_invocation_id.map(str::to_owned),
            host_session_id: owner.host_session_id.map(str::to_owned),
            trusted_request_id: caller.request_id.clone(),
            prewarm: context.prewarm,
            finalization_instance_id: context.finalization_instance_id.clone(),
        });
    }

    /// Record unavailable authority for known keys and optional safe `error_code`; count incomplete capture.
    /// 为已知键及可选安全 `error_code` 记录不可取得的权威；计入不完整采集。
    fn unavailable(
        &self,
        runtime_id: &str,
        operation_id: &str,
        error_code: Option<EmbeddedErrorCode>,
    ) {
        self.failures.fetch_add(1, Ordering::AcqRel);
        self.emit(VulcanPluginNativeIdentityEvent::ObservationUnavailable {
            runtime_id: runtime_id.to_owned(),
            operation_id: operation_id.to_owned(),
            error_code,
        });
    }
}

/// Lazily construct `event` only for an installed observer; return without allocation when disabled.
/// 仅为已安装观察器惰性构造 `event`；禁用时不分配直接返回。
pub(super) fn observe_pool(event: impl FnOnce() -> VulcanPluginNativeIdentityEvent) {
    if let Some(sink) = sink() {
        sink.emit(event());
    }
}

/// Read the minimum existing public Core API for admitted `operation`, only when observation is enabled.
/// 仅启用观测时，为已接纳 `operation` 读取最小既有 Core 公共接口。
/// Core exposes only snapshot(); copy safe metadata then immediately drop its business payload and temporary handle observation.
/// Core 仅暴露 snapshot()；复制安全元数据后立即丢弃业务载荷及临时句柄观测。
pub(super) fn observe_operation(
    runtime_id: &str,
    operation: &OperationHandle,
    owner: NativeIdentityOwner<'_>,
) {
    if let Some(sink) = sink() {
        match operation.snapshot() {
            Ok(snapshot) => sink.operation(runtime_id, &snapshot, owner),
            Err(error) => sink.unavailable(runtime_id, operation.id(), Some(error.code)),
        }
    }
}

/// Borrow the caller's existing `snapshot`; observation never clones the full value or effects snapshot.
/// 借用调用方既有 `snapshot`；观测绝不克隆完整值或副作用快照。
pub(super) fn observe_snapshot(
    runtime_id: &str,
    snapshot: &OperationSnapshot,
    owner: NativeIdentityOwner<'_>,
) {
    if let Some(sink) = sink() {
        sink.operation(runtime_id, snapshot, owner);
    }
}

#[cfg(test)]
mod tests;
