//! Single manager ownership and control for the formal plugin parent runtime.
//! 正式插件父运行时的单个管理器所有权及控制。

use super::*;
use luaskills::runtime::embedded::{EmbeddedRuntime, EmbeddedRuntimeUsage, PoolUsage};
use serde::Serialize;

mod budgets;
pub use budgets::VulcanPluginFormalPluginStatus;
mod diagnostics;
pub use diagnostics::{
    VulcanPluginResourceDiagnostic, VulcanPluginResourceDiagnostics,
    VulcanPluginResourceErrorDiagnostic, VulcanPluginResourceKind,
    VulcanPluginResourceOperationDiagnostic,
};
mod capacities;
pub use capacities::{
    VulcanPluginCapacityControlAction, VulcanPluginCapacityControlRequest,
    VulcanPluginCapacityRegistrationRequest, VulcanPluginFormalCapacityCatalog,
    VulcanPluginFormalCapacityStatus,
};

mod calls;
pub use calls::reuse::VulcanPluginReusableDomainStatus;
pub use calls::{
    VulcanPluginAutomaticPrewarmOrigin, VulcanPluginAutomaticPrewarmPermit,
    VulcanPluginAutomaticPrewarmTrigger, VulcanPluginFormalCallStatus,
    VulcanPluginLegacyLifecycleCall, VulcanPluginPrewarmTargetRequest,
    VulcanPluginPrewarmTargetStatus,
};
mod pinned;
pub use pinned::VulcanPluginFormalSessionStatus;
pub(super) use pinned::deadline as operation_deadline;
mod instances;
#[cfg(test)]
pub(super) use instances::PersistentRuntime;
pub use instances::{
    VulcanPluginActivationPredecessorCompletion,
    VulcanPluginActivationPredecessorHandoffParticipant,
    VulcanPluginActivationPredecessorRegistration, VulcanPluginActivationTargetCompletion,
    VulcanPluginActivationTargetHandoffParticipant, VulcanPluginActivationTargetRegistration,
    VulcanPluginMcpPredecessorHandoffEvidence, VulcanPluginMcpTargetHandoffEvidence,
    VulcanPluginNativeMcpHandoffAdapter, VulcanPluginNativeMcpPredecessorHandoffAdapter,
};

/// One original attachment is either fully ready or owns the exact Core whose post-construction registration failed.
/// 单个原附加状态要么完全就绪，要么拥有构造后注册失败的精确 Core。
/// Both branches retain the same native identity; failed initialization never grants business ownership or means absence.
/// 两个分支均保留同一原生身份；失败初始化绝不授予业务所有权，也不表示缺失。
#[derive(Clone)]
pub(super) enum FormalRuntimeAttachment {
    /// Fully registered host ownership; its adapter contains the original shared Core.
    /// 完全注册的宿主所有权；其适配器包含原共享 Core。
    Ready(Arc<ManagedFormalRuntime>),
    /// Retain the original Core, validated construction settings and exact initialization error for explicit closing retries.
    /// 保留原 Core、已校验构造设置及精确初始化错误，供明确关闭重试。
    Failed {
        /// The same constructed Core remains reachable until real worker and retirement joins.
        /// 同一已构造 Core 在真实工作线程及退役等待退出之前持续可达。
        runtime: Arc<EmbeddedRuntime>,
        /// Original settings bound failure cleanup without substituting global defaults.
        /// 原设置约束失败清理，不以全局默认值替代。
        /// Shared immutable settings avoid copying the configured inventory on every shutdown observation.
        /// 共享不可变设置避免在每次关闭观测中复制配置清单。
        settings: Arc<VulcanPluginRuntimeSettings>,
        /// Exact original failure is returned to every ordinary owner query.
        /// 每次普通所有者查询均返回精确原失败。
        error: VulcanPluginRuntimeError,
    },
}

impl FormalRuntimeAttachment {
    /// Borrow this attachment's sole actual Core for shutdown control only.
    /// 仅为关闭控制借用此附加状态唯一实际 Core。
    /// No inputs beyond this owner; returns the same ready or failed Core without creating or replacing one.
    /// 除此所有者外无输入；返回同一就绪或失败 Core，不创建或替换 Core。
    pub(super) fn runtime(&self) -> &EmbeddedRuntime {
        match self {
            Self::Ready(owner) => owner.host.runtime(),
            Self::Failed { runtime, .. } => runtime.as_ref(),
        }
    }
}

/// Manager-owned formal runtime and the exact settings frozen at construction.
/// 管理器持有的正式运行时及构造时冻结的精确设置。
pub(super) struct ManagedFormalRuntime {
    /// Native adapter owns the core parent and all runtime-local capability registrations.
    /// 原生适配器拥有核心父级及全部运行时内能力注册。
    pub host: VulcanPluginEmbeddedHost,
    /// Construction baseline freezes parent/plugin limits and timeouts; live purpose policy belongs to capacities.
    /// 构造基线冻结父级／插件上限及超时；活动用途策略属于 capacities。
    pub settings: VulcanPluginRuntimeSettings,
    /// Exact core capacity identities and preparing-member claims survive module generation changes.
    /// 精确核心容量身份及准备中成员声明在模块代次变化后保持存活。
    capacities: Mutex<capacities::CapacityRegistry>,
    /// Bounded host ownership covers preparation through physical cleanup and unacknowledged results.
    /// 有界宿主所有权覆盖准备、物理清理及未确认结果。
    calls: Mutex<BTreeMap<String, Arc<calls::FormalCall>>>,
    /// Bounded reusable domain metadata; physical VM accounting remains exclusively in LuaSkills.
    /// 有界可复用域元数据；物理 VM 记账仍完全属于 LuaSkills。
    reusable: Mutex<calls::reuse::Registry>,
    /// One weakly owned observer collects independent idle-instance finalizations.
    /// 单个弱持有观测者收集独立空闲实例关闭结果。
    reuse_observer: AtomicBool,
    /// Host session identities exist before preparation and survive interrupted observers.
    /// 宿主会话身份在准备前存在，并在观测者中断后存活。
    pinned: Mutex<BTreeMap<String, Arc<pinned::PinnedSession>>>,
    /// Persistent Hook ownership includes preparation and physical retirement.
    /// 持久 Hook 所有权包含准备及物理退役。
    hooks: Mutex<BTreeMap<String, Arc<instances::PersistentRuntime>>>,
    /// Persistent UI instances share ownership mechanics but never Hook state or finalizers.
    /// 持久 UI 实例共享所有权机制，但绝不共享 Hook 状态或关闭器。
    ui_actions: Mutex<BTreeMap<String, Arc<instances::PersistentRuntime>>>,
}

/// Read-only observation of this manager's formal runtime, separate from legacy lease accounting.
/// 此管理器正式运行时的只读观测，与旧租约记账分开。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginFormalRuntimeStatus {
    /// Core-issued identity shared by every clone of this manager.
    /// 此管理器全部克隆共享的核心签发身份。
    pub runtime_id: String,
    /// Exact validated settings, including the effective parent limits.
    /// 精确已校验设置，包含生效父级上限。
    pub settings: VulcanPluginRuntimeSettings,
    /// Current retained administrative receipt is captured atomically with its effective settings.
    /// 当前保留管理回执与其生效设置原子捕获。
    pub policy_update: Option<VulcanPluginCapacityPolicyRecord>,
    /// Current queue and operation accounting from the sole core owner.
    /// 来自唯一核心所有者的当前队列及操作记账。
    pub operations: EmbeddedRuntimeUsage,
    /// Actual VM accounting, including creation and unfinished retirement.
    /// 真实 VM 记账，包含创建中及尚未完成退役。
    pub resources: PoolUsage,
}

impl VulcanPluginRuntimeManager {
    /// Attach exactly one formal runtime using `settings` and the already attached plugin engine.
    /// 使用 `settings` 及已附加插件引擎，恰好附加一个正式运行时。
    /// Returns the original error while retaining any constructed Core for real closing; only complete registration grants a ready owner.
    /// 返回原错误，并保留已构造 Core 供真实关闭；仅完整注册授予就绪所有者。
    /// Repeated attachment never replaces either a ready or failed original owner.
    /// 重复附加绝不替换就绪或失败原所有者。
    pub fn initialize_formal_runtime(
        &self,
        settings: VulcanPluginRuntimeSettings,
    ) -> Result<(), VulcanPluginRuntimeError> {
        settings.validate().map_err(VulcanPluginRuntimeError::new)?;
        // Hold only the attachment lock; construction registers metadata and starts bounded core workers, never Lua.
        // 仅持有附加锁；构造注册元数据并启动有界核心工作线程，绝不运行 Lua。
        let mut slot = self
            .state
            .formal_runtime
            .lock()
            .map_err(|_| formal_lock_error())?;
        if slot.is_some() {
            return Err(VulcanPluginRuntimeError::new(
                "Formal plugin runtime is already attached.",
            ));
        }
        // Preserve disk transaction evidence before creating any native owner; attachment never rewrites it.
        // 创建任何原生所有者前保留磁盘事务证据；附加绝不回写它。
        let policy =
            super::settings::policy::PolicyStore::load(self.plugin_state_root(), &settings)
                .map_err(VulcanPluginRuntimeError::new)?;
        // The adapter validates native registration capacity before starting its core owner.
        // 适配器在启动核心所有者前校验原生注册容量。
        let runtime = VulcanPluginEmbeddedHost::prepare_runtime(
            self,
            settings.parent.clone(),
            settings.max_capability_call_ms,
        )
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Every fallible step after Core construction must reach this one ownership commit before releasing the attachment gate.
        // Core 构造后每个可失败步骤必须在释放附加门之前到达此唯一所有权提交。
        let initialized: Result<
            (VulcanPluginEmbeddedHost, capacities::CapacityRegistry),
            VulcanPluginRuntimeError,
        > = (|| {
            // The adapter receives the same Arc retained by the original initializer, never another Core.
            // 适配器接收原初始化器保留的同一 Arc，绝不接收另一 Core。
            let host = VulcanPluginEmbeddedHost::attach_runtime(
                self,
                Arc::clone(&runtime),
                &settings.parent,
                settings.max_capability_call_ms,
            )
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
            // Explicit guarantees exist before any shared production work can consume the parent budget.
            // 在任何公共生产工作消费父级预算前，显式保证已经存在。
            let capacities = capacities::register_configured_capacities(&host, &settings, policy)?;
            Ok((host, capacities))
        })();
        match initialized {
            Ok((host, capacities)) => {
                *slot = Some(FormalRuntimeAttachment::Ready(Arc::new(
                    ManagedFormalRuntime {
                        host,
                        settings,
                        capacities: Mutex::new(capacities),
                        calls: Mutex::new(BTreeMap::new()),
                        reusable: Mutex::new(calls::reuse::Registry::default()),
                        reuse_observer: AtomicBool::new(false),
                        pinned: Mutex::new(BTreeMap::new()),
                        hooks: Mutex::new(BTreeMap::new()),
                        ui_actions: Mutex::new(BTreeMap::new()),
                    },
                )));
                Ok(())
            }
            Err(error) => {
                // Retain the already constructed Core before returning its original failure; Drop is not a join substitute.
                // 返回原失败之前保留已构造 Core；Drop 不能替代等待退出。
                *slot = Some(FormalRuntimeAttachment::Failed {
                    runtime,
                    settings: Arc::new(settings),
                    error: error.clone(),
                });
                Err(error)
            }
        }
    }

    /// Return the actual attached capability registry for trusted host registration and queued dispatch.
    /// 返回真实附加的能力注册表，供可信宿主注册及分发排队调用。
    /// Uses only this manager; absence is an error and never creates a parent or another registry.
    /// 仅使用此管理器；未附加时返回错误，绝不创建父级或另一个注册表。
    /// The returned handle retains its original identity after shutdown; core closure still rejects new registration.
    /// 返回句柄在关闭后保留原身份；核心关闭仍拒绝新注册。
    /// Callers explicitly refresh the catalog after changing registrations and retain responsibility for handler authority.
    /// 调用方在变更注册后显式刷新目录，并继续负责处理器授权。
    pub fn formal_capabilities(
        &self,
    ) -> Result<
        Arc<luaskills::runtime::embedded::capabilities::CapabilityRegistry>,
        VulcanPluginRuntimeError,
    > {
        // Keep the exact attached parent alive until its existing registry handle has been cloned.
        // 保持精确附加父级存活，直到其已有注册表句柄完成克隆。
        let owner = self.formal_runtime_owner()?.ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal plugin runtime is not attached.")
        })?;
        Ok(owner.host.runtime().capabilities())
    }

    /// Return this manager's shared formal owner, or `None` for a metadata-only manager.
    /// 返回此管理器共享的正式所有者；纯元数据管理器返回 `None`。
    /// The attachment lock is released before callers enter core control or execution.
    /// 调用方进入核心控制或执行前释放附加锁。
    /// A failed attachment returns its exact initialization error, never `None` or a partially ready owner.
    /// 失败附加返回精确初始化错误，绝不返回 `None` 或部分就绪所有者。
    pub(super) fn formal_runtime_owner(
        &self,
    ) -> Result<Option<Arc<ManagedFormalRuntime>>, VulcanPluginRuntimeError> {
        match self.formal_runtime_attachment()? {
            Some(FormalRuntimeAttachment::Ready(owner)) => Ok(Some(owner)),
            Some(FormalRuntimeAttachment::Failed { error, .. }) => Err(error),
            None => Ok(None),
        }
    }

    /// Clone the one original ready-or-failed attachment for trusted shutdown control.
    /// 为可信关闭控制克隆唯一原就绪或失败附加状态。
    /// No arguments; returns explicit pre-Core absence or the retained original branch, releasing the lock before any drain work.
    /// 无参数；返回明确 Core 前缺失或保留原分支，在任何排空工作前释放锁。
    fn formal_runtime_attachment(
        &self,
    ) -> Result<Option<FormalRuntimeAttachment>, VulcanPluginRuntimeError> {
        Ok(self
            .state
            .formal_runtime
            .lock()
            .map_err(|_| formal_lock_error())?
            .clone())
    }

    /// Observe effective settings and current accounting without creating a runtime or changing configuration.
    /// 观测生效设置及当前记账，不创建运行时或修改配置。
    /// Returns `None` before explicit attachment; core observation failures remain explicit errors.
    /// 显式附加前返回 `None`；核心观测失败保持显式错误。
    pub fn formal_runtime_status(
        &self,
    ) -> Result<Option<VulcanPluginFormalRuntimeStatus>, VulcanPluginRuntimeError> {
        // Absence is a real attachment state, never a reason to create an implicit runtime.
        // 缺失是真实附加状态，绝不据此隐式创建运行时。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        // The cloned owner keeps this exact runtime alive throughout both observations.
        // 克隆所有者在两次观测期间保持此精确运行时存活。
        let runtime = owner.host.runtime();
        // Persisted intent and current settings must never be sampled from different policy transactions.
        // 持久意图及当前设置绝不能采样自不同策略事务。
        let (settings, policy_update) = owner.policy_status()?;
        Ok(Some(VulcanPluginFormalRuntimeStatus {
            runtime_id: runtime.id().to_string(),
            settings,
            policy_update,
            operations: runtime
                .usage()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?,
            resources: runtime
                .resources()
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?,
        }))
    }

    /// Permanently close formal admission and request cancellation without claiming that resources have drained.
    /// 永久关闭正式入场并请求取消，不宣称资源已经排空。
    /// No runtime means no work; attached owners remain available for subsequent observations.
    /// 没有运行时即没有工作；已附加所有者继续可供后续观测。
    pub fn request_formal_runtime_close(&self) -> Result<(), VulcanPluginRuntimeError> {
        match self.formal_runtime_attachment()? {
            Some(FormalRuntimeAttachment::Failed { runtime, .. }) => {
                // Failed native registration never published business registries; close its same original Core directly.
                // 失败原生注册从未发布业务注册表；直接关闭同一原 Core。
                runtime
                    .request_close()
                    .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
            }
            Some(FormalRuntimeAttachment::Ready(owner)) => {
                {
                    // Serialize permanent closure with registration and same-owner transfer; no callback or drain wait runs here.
                    // 将永久关闭与注册及同所有者转交串行化；此处不执行回调或排空等待。
                    let _registration =
                        self.state.session_registration_gate.lock().map_err(|_| {
                            VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
                        })?;
                    self.close_reusable_domains(None, None, true)?;
                    owner.cancel_calls()?;
                    owner.cancel_pinned()?;
                    owner.cancel_instances()?;
                    owner
                        .host
                        .runtime()
                        .request_close()
                        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
                }
                // Pending retirement acquires the same registration gate itself, after permanent closure has been recorded.
                // 永久关闭记录之后，待处理退役自行取得相同注册门。
                self.retire_pending_formal_calls(&owner)?;
            }
            None => {}
        }
        Ok(())
    }

    /// Permanently close this manager's attached parent and observe complete joins within its actual shutdown budget.
    /// 永久关闭此管理器附加父级，并在其真实关闭预算内观测完整线程等待退出。
    /// No arguments; this synchronous host-only boundary must run outside service locks and native callbacks.
    /// 无参数；此同步宿主边界必须在服务锁及原生回调之外运行。
    /// Returns success only for explicit parent absence or real execution/retirement joins; timeout and errors retain the original owner.
    /// 仅在明确父级缺失或真实执行／退役线程等待退出时返回成功；超时及错误保留原所有者。
    /// Closure is irreversible; the finite observation budget neither restores admission nor frees unfinished resources.
    /// 关闭不可逆；有限观测预算既不恢复准入，也不释放尚未完成的资源。
    pub fn close_formal_runtime_blocking(&self) -> Result<(), VulcanPluginRuntimeError> {
        // Read the same retained parent's effective settings, never global defaults or a later configuration source.
        // 读取同一保留父级的生效设置，绝不读取全局默认值或后来的配置来源。
        let Some(attachment) = self.formal_runtime_attachment()? else {
            return Ok(());
        };
        // A failed parent keeps its original setting; a ready parent's actual policy observation remains authoritative.
        // 失败父级保持原设置；就绪父级真实策略观测仍为权威。
        let shutdown_timeout_ms = match &attachment {
            FormalRuntimeAttachment::Failed { settings, .. } => settings.shutdown_timeout_ms,
            FormalRuntimeAttachment::Ready(owner) => owner.policy_status()?.0.shutdown_timeout_ms,
        };
        // Freeze one attempt deadline before requesting permanent closure; retries keep this same owner.
        // 请求永久关闭前冻结单次尝试截止点；重试保持同一所有者。
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(shutdown_timeout_ms))
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal shutdown deadline overflowed."))?;
        self.request_formal_runtime_close()?;
        loop {
            if self.poll_formal_runtime_closed()? {
                return Ok(());
            }
            // Saturation makes an elapsed budget explicit instead of extending it after a slow poll.
            // 饱和减法明确表达预算耗尽，而不在缓慢轮询之后延长预算。
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Formal runtime `{}` did not completely close within {} ms; its original owner remains retained.",
                    attachment.runtime().id(),
                    shutdown_timeout_ms,
                )));
            }
            // Reuse the sole formal observation cadence and never sleep past this attempt's deadline.
            // 复用唯一正式观测节奏，且绝不睡眠超过本次尝试截止点。
            std::thread::sleep(calls::OBSERVATION_INTERVAL.min(remaining));
        }
    }

    /// Check actual core worker and VM termination after a close request.
    /// 在关闭请求后检查核心工作线程与 VM 的真实终止。
    /// Returns true only without a runtime, or after host ownership drains and the core confirms complete shutdown.
    /// 仅在无运行时，或宿主所有权排空且核心确认完整关闭后返回真值。
    pub fn poll_formal_runtime_closed(&self) -> Result<bool, VulcanPluginRuntimeError> {
        match self.formal_runtime_attachment()? {
            None => Ok(true),
            Some(FormalRuntimeAttachment::Failed { runtime, .. }) => {
                // Core owns partial native registrations; only its execution and retirement joins prove failure cleanup complete.
                // Core 拥有部分原生注册；仅其执行及退役等待退出证明失败清理完成。
                runtime
                    .poll_closed()
                    .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
            }
            Some(FormalRuntimeAttachment::Ready(owner)) => {
                self.maintain_reusable_domains(&owner)?;
                if !owner
                    .reusable
                    .lock()
                    .map_err(|_| formal_lock_error())?
                    .is_empty()
                {
                    return Ok(false);
                }
                if !owner.calls_drained()?
                    || !owner.pinned_drained()?
                    || !owner.instances_drained()?
                {
                    return Ok(false);
                }
                owner
                    .host
                    .runtime()
                    .poll_closed()
                    .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
            }
        }
    }
}

/// Construct the stable attachment-lock error without hiding poisoned ownership.
/// 构造稳定附加锁错误，不隐藏已中毒的所有权。
fn formal_lock_error() -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new("Formal plugin runtime attachment lock is poisoned.")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod failed_initialization_tests {
    use super::*;
    use crate::runtime::tests::test_manager;
    use std::time::{Duration, Instant};

    /// Build valid settings whose legitimate capacity owner exceeds the actual Core JSON identity budget.
    /// 构建有效设置，使合法容量所有者超出真实 Core JSON 身份预算。
    /// No inputs; returns the original settings that pass host validation and fail real native plugin registration.
    /// 无输入；返回通过宿主校验、却在真实原生插件注册失败的原设置。
    fn native_registration_failure_settings() -> VulcanPluginRuntimeSettings {
        // A small explicit fixture budget bounds memory while preserving the production validation path.
        // 较小明确夹具预算约束内存，同时保留生产校验路径。
        let mut settings = VulcanPluginRuntimeSettings::default();
        settings.parent.max_value_bytes = 1024;
        // Every character is valid; length derives from the actual original Core setting instead of another limit.
        // 每个字符均合法；长度派生自真实原 Core 设置，而非另一个上限。
        let plugin_id = "a".repeat(settings.parent.max_value_bytes + 1);
        // Resolve the same production inherited purpose budget before publishing its explicit configured commitment.
        // 发布明确配置承诺之前，解析同一生产继承用途预算。
        let budget = settings
            .capacity_budget(&plugin_id, VulcanPluginCapacityPurpose::Hook)
            .0;
        settings.capacity_budgets.insert(
            plugin_id,
            [(VulcanPluginCapacityPurpose::Hook, budget)].into(),
        );
        settings
            .validate()
            .expect("valid host settings before native failure");
        settings
    }

    /// Retain the exact failed Core, startup error and settings while business rejects partial registration and closing retries really join it.
    /// 保留精确失败 Core、启动错误及设置，同时业务拒绝部分注册，关闭重试真实等待其退出。
    /// No inputs or result; the actual overly long valid identity reaches Core registration and its same owner is tested through public closing.
    /// 无输入及返回值；实际过长合法身份进入 Core 注册，并通过公开关闭测试同一所有者。
    #[test]
    fn post_core_capacity_registration_failure_retains_original_parent_and_error() {
        // Private package and engine roots follow the established real native fixture.
        // 私有插件包及引擎根遵循既有真实原生夹具。
        let root = tempfile::tempdir().expect("failed original native root");
        // This manager has a real engine but no preexisting parent to hide the failure.
        // 此管理器具有真实引擎，但没有可隐藏失败的既有父级。
        let manager = test_manager(root.path(), root.path().join("bundled"));
        // The settings themselves remain valid; failure is specifically post-Core native registration.
        // 设置自身保持有效；失败明确发生在 Core 后原生注册。
        let settings = native_registration_failure_settings();
        // Capture the exact original public initialization error before any cleanup attempt.
        // 任何清理尝试之前捕获精确原公开初始化错误。
        let original_error = manager
            .initialize_formal_runtime(settings.clone())
            .expect_err("real native registration failure");
        assert!(
            original_error
                .message
                .contains("JSON value exceeds its configured byte limit")
        );
        // The raw owner is read only from the actual single attachment slot.
        // 原始所有者仅从实际单个附加槽读取。
        let runtime = match manager
            .formal_runtime_attachment()
            .expect("retained attachment")
            .expect("failed original slot")
        {
            FormalRuntimeAttachment::Failed {
                runtime,
                settings: retained,
                error,
            } => {
                assert_eq!(retained.as_ref(), &settings);
                assert_eq!(error.message, original_error.message);
                runtime
            }
            FormalRuntimeAttachment::Ready(_) => {
                panic!("partial registration must not grant ready ownership")
            }
        };
        // The already constructed parent is real and still open; absence cannot manufacture a true close.
        // 已构造父级真实存在且仍开放；缺失不能制造关闭真值。
        let original_id = runtime.id().to_owned();
        assert!(
            !manager
                .poll_formal_runtime_closed()
                .expect("original remains open")
        );
        assert_eq!(
            manager
                .formal_runtime_owner()
                .err()
                .expect("reject partial owner")
                .message,
            original_error.message
        );
        assert_eq!(
            manager
                .formal_runtime_status()
                .err()
                .expect("reject partial status")
                .message,
            original_error.message
        );
        assert_eq!(
            manager
                .formal_capabilities()
                .err()
                .expect("reject partial registry")
                .message,
            original_error.message
        );
        assert_eq!(
            manager
                .formal_plugin_status("valid-owner")
                .err()
                .expect("reject partial business")
                .message,
            original_error.message
        );
        assert!(manager.initialize_formal_runtime(settings.clone()).is_err());
        manager
            .request_formal_runtime_close()
            .expect("close actual failed Core");
        assert!(runtime.usage().expect("actual closing state").closing);
        manager
            .close_formal_runtime_blocking()
            .expect("real failed execution and retirement joins");
        assert!(runtime.poll_closed().expect("joined same original Core"));
        manager
            .close_formal_runtime_blocking()
            .expect("same owner closing retry");
        match manager
            .formal_runtime_attachment()
            .expect("original after retry")
            .expect("retained failed owner")
        {
            FormalRuntimeAttachment::Failed {
                runtime: retained,
                settings: retained_settings,
                error,
            } => {
                assert!(Arc::ptr_eq(&runtime, &retained));
                assert_eq!(retained.id(), original_id);
                assert_eq!(retained_settings.as_ref(), &settings);
                assert_eq!(error.message, original_error.message);
            }
            FormalRuntimeAttachment::Ready(_) => {
                panic!("closure cannot convert failed ownership to ready")
            }
        }
        assert_eq!(
            manager
                .formal_runtime_status()
                .err()
                .expect("startup error survives closing")
                .message,
            original_error.message
        );
    }

    /// Dropping the actual failed manager requests closure of its retained same Core, which still requires explicit joins.
    /// 丢弃实际失败管理器会请求关闭其保留同一 Core，而该 Core 仍需明确等待退出。
    /// No inputs or result; a real retained observer verifies Drop requests rather than pretending native success.
    /// 无输入及返回值；真实保留观测者验证 Drop 请求，而非假装原生成功。
    #[test]
    fn failed_attachment_manager_drop_requests_same_core_close() {
        // Isolated real roots remain alive until observer-confirmed physical closure.
        // 隔离真实根在观测者确认物理关闭之前保持存活。
        let root = tempfile::tempdir().expect("failed manager drop root");
        // The original construction uses the same actual engine fixture.
        // 原构造使用同一实际引擎夹具。
        let manager = test_manager(root.path(), root.path().join("bundled"));
        // The exact original settings also bound this observer's wait.
        // 精确原设置同时约束此观测者等待。
        let settings = native_registration_failure_settings();
        manager
            .initialize_formal_runtime(settings.clone())
            .expect_err("real post-Core failure");
        // Retain only the actual same Core, never another manager or execution runtime.
        // 仅保留实际同一 Core，绝不保留另一管理器或执行运行时。
        let runtime = match manager
            .formal_runtime_attachment()
            .expect("attachment")
            .expect("failed original")
        {
            FormalRuntimeAttachment::Failed { runtime, .. } => runtime,
            FormalRuntimeAttachment::Ready(_) => {
                panic!("expected failed original may have changed")
            }
        };
        drop(manager);
        assert!(
            runtime
                .usage()
                .expect("same Core closing admission")
                .closing
        );
        // Actual finite polling proves termination; last-manager release itself proves only a close request.
        // 实际有限轮询证明终止；最后管理器释放自身仅证明关闭请求。
        let deadline = Instant::now() + Duration::from_millis(settings.shutdown_timeout_ms);
        while !runtime.poll_closed().expect("same failed Core joins") {
            // Reuse the sole formal cadence rather than busy polling or copying another timing constant.
            // 复用唯一正式节奏，不忙轮询，也不复制另一时间常量。
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "failed original Core did not join");
            std::thread::sleep(calls::OBSERVATION_INTERVAL.min(remaining));
        }
    }
}
