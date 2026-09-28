//! Bounded host ownership for stateless calls submitted to the sole formal runtime.
//! 提交到唯一正式运行时的无状态调用有界宿主所有权。

use super::*;
use crate::{VulcanPluginDescriptor, VulcanPluginToolDescriptor};
use luaskills::LuaInvocationContext;
use luaskills::runtime::embedded::{
    EmbeddedCall, EmbeddedErrorCode, EmbeddedPluginConfig, ExecutionBackend, InstanceReuse,
    ModuleFinalizer, OperationHandle, OperationSnapshot, PluginPoolConfig, PoolKind,
};
use serde_json::{Map, Value, json};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// One control observation interval; polling never occupies a waiting OS thread.
/// 唯一控制观测间隔；轮询不会占用等待中的系统线程。
pub(super) const OBSERVATION_INTERVAL: Duration = Duration::from_millis(10);

/// One exact admitted host invocation; metadata never owns another VM scheduler.
/// 单个精确已接纳宿主调用；元数据绝不拥有另一个 VM 调度器。
pub(super) struct FormalCall {
    /// Stable host identity exists before any package preparation starts.
    /// 稳定宿主身份在任何包准备开始前存在。
    id: String,
    /// Frozen descriptor pins the original managed package generation.
    /// 冻结描述符固定原受管包代次。
    plugin: VulcanPluginDescriptor,
    /// Exact controller session owns both pending and completed evidence.
    /// 精确控制器会话拥有待处理及完成证据。
    owner_session_id: String,
    /// Conservative serialized input reservation persists until the result is acknowledged.
    /// 保守序列化输入预留保留至结果被确认。
    pub(super) input_bytes: usize,
    /// Cancellation remains live before and after core admission.
    /// 核心入场前后取消均保持实时。
    cancelled: AtomicBool,
    /// Short state lock protects identity publication and cleanup evidence only.
    /// 短状态锁仅保护身份发布及清理证据。
    state: Mutex<CallState>,
}

/// Mutable control evidence for one host call, independent of its observer's lifetime.
/// 单个宿主调用的可变控制证据，独立于观测者寿命。
#[derive(Default)]
struct CallState {
    /// Pool identity is published before submission and retained until physical forgetting succeeds.
    /// 池身份在提交前发布，并保留至物理遗忘成功。
    pool: Option<String>,
    /// Core-issued operation owns the actual execution and automatic finalization.
    /// 核心签发操作拥有实际执行及自动关闭。
    operation: Option<OperationHandle>,
    /// Result becomes observable only after cleanup, or as an explicit retained failure.
    /// 结果仅在清理后可见，或作为明确保留失败可见。
    result: Option<Result<Value, VulcanPluginRuntimeError>>,
    /// Physical retirement, not cancellation or a returned business value, releases the host slot.
    /// 物理退役释放宿主槽位，取消或已返回业务值不释放。
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

/// Frozen, byte-checked inputs moved into one bounded preparation task.
/// 移入单个有界准备任务的冻结且已校验字节输入。
struct Preparation {
    /// Workspace is host-owned and canonicalized by the bundle registrar.
    /// 工作区由宿主持有，由 bundle 注册器规范化。
    workspace: PathBuf,
    /// Exact business envelope includes its unique correlation and finite budget.
    /// 精确业务信封包含唯一关联及有限预算。
    arguments: Value,
    /// Shutdown has independent correlation and its own finite budget.
    /// 关闭拥有独立关联及自身有限预算。
    finalizer_arguments: Value,
    /// Absolute admission deadline includes blocking preparation and core queue waits.
    /// 绝对入场截止时间包含阻塞准备及核心队列等待。
    deadline: Instant,
}

/// Queryable host and core evidence for one owned stateless invocation.
/// 单个有归属无状态调用的可查询宿主及核心证据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginFormalCallStatus {
    /// Stable host identity accepted by query, cancellation and forgetting.
    /// 查询、取消及遗忘接受的稳定宿主身份。
    pub invocation_id: String,
    /// Exact plugin and package routing generation captured on admission.
    /// 入场时捕获的精确插件及包路由代次。
    pub plugin_id: String,
    /// Original routing generation never follows a later catalog refresh.
    /// 原路由代次绝不跟随后续目录刷新。
    pub plugin_generation: u64,
    /// Core status is absent only before operation publication or when preparation failed.
    /// 仅操作发布前或准备失败时缺少核心状态。
    pub operation: Option<OperationSnapshot>,
    /// True only after actual pool ownership has been forgotten successfully.
    /// 仅在真实池所有权成功遗忘后为真。
    pub retired: bool,
    /// An interrupted supervisor requires explicit drain recovery, while actual resources stay owned.
    /// 中断的监督者需要显式排空恢复，真实资源仍保持归属。
    pub requires_cleanup_recovery: bool,
    /// Successful version-one business projection, including explicit JSON null.
    /// 成功的版本一业务投影，包含显式 JSON 空值。
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
        if !self.completed
            && let Ok(mut state) = self.call.lock()
        {
            state.supervised = false;
            state.result = Some(Err(VulcanPluginRuntimeError::new(
                "Formal supervision was interrupted; inspect and drain the retained invocation.",
            )));
        }
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
    }
}

impl FormalCall {
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
            invocation_id: self.id.clone(),
            plugin_id: self.plugin.id.clone(),
            plugin_generation: self.plugin.generation,
            operation: state
                .operation
                .as_ref()
                .map(OperationHandle::snapshot)
                .transpose()
                .map_err(core_error)?,
            retired: state.retired,
            requires_cleanup_recovery: !state.supervised && !state.retired,
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
    /// Resume only idle cleanup after actual preparation ended; never repeat module preparation or submission.
    /// 仅在实际准备结束后恢复闲置清理；绝不重复模块准备或提交。
    /// `owner` and `call` are retained originals; return explicit metadata failure without inventing evidence.
    /// `owner` 及 `call` 为保留原件；明确返回元数据失败，不伪造证据。
    fn resume_formal_cleanup(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        call: &Arc<FormalCall>,
    ) -> Result<(), VulcanPluginRuntimeError> {
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
        tokio::spawn(async move {
            let mut supervision = supervision;
            let result = manager
                .finish_formal_call(&retry_owner, &retry_call, previous.map(|_| ()))
                .await;
            if let Ok(mut state) = retry_call.lock() {
                state.supervised = false;
                state.result = Some(result);
            }
            supervision.completed = true;
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
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(VulcanPluginDrainReport::default());
        };
        let calls: Vec<_> = owner
            .calls
            .lock()
            .map_err(|_| calls_error())?
            .values()
            .filter(|call| {
                plugin_id.is_none_or(|id| call.plugin.id == id)
                    && owner_session_id.is_none_or(|id| call.owner_session_id == id)
            })
            .cloned()
            .collect();
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(owner.settings.shutdown_timeout_ms))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Formal drain deadline is out of range.")
            })?;
        for call in &calls {
            call.cancelled.store(true, Ordering::Release);
        }
        let mut report = VulcanPluginDrainReport::default();
        for call in calls {
            loop {
                self.resume_formal_cleanup(&owner, &call)?;
                let (retired, supervised, cleanup_error) = {
                    let state = call.lock()?;
                    (state.retired, state.supervised, state.cleanup_error.clone())
                };
                if retired && !supervised {
                    report.closed_formal_calls += 1;
                    if let Some(error) = cleanup_error {
                        report.failures.push(format!(
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
                    self.record_cleanup_failure(&call.plugin.id, diagnostic.clone())?;
                    report.failures.push(diagnostic);
                    break;
                }
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
            }
        }
        Ok(report)
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
        let owner = self.formal_runtime_owner()?.ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal plugin runtime is not attached.")
        })?;
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(timeout_ms))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin execution deadline is out of range.")
            })?;
        // Reuse the core's streaming JSON counter before allocating an envelope or retaining input.
        // 在分配信封或保留输入前复用核心流式 JSON 计数器。
        luaskills::runtime::embedded::json_size(&payload, owner.settings.parent.max_value_bytes)
            .map_err(core_error)?;
        let arguments = self.formal_envelope(payload, timeout_ms)?;
        let finalizer_arguments = self.formal_envelope(
            plugin_payload("shutdown", Map::new())?,
            owner.settings.shutdown_timeout_ms,
        )?;
        luaskills::runtime::embedded::json_size(&arguments, owner.settings.parent.max_value_bytes)
            .map_err(core_error)?;
        let input_bytes = luaskills::runtime::embedded::json_size(
            &(
                &arguments,
                &finalizer_arguments,
                &owner_session_id,
                &workspace_root,
            ),
            owner.settings.parent.max_queued_bytes,
        )
        .map_err(core_error)?;
        // Publication cannot race host registration; every later preparation is already visible to drain.
        // 发布不能与宿主登记竞争；之后的每次准备都已对排空可见。
        let call = {
            let _publication = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
            self.ensure_current_tool_generation(tool)?;
            let plugin = self
                .snapshot()
                .plugins
                .get(&tool.plugin_id)
                .cloned()
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Plugin disappeared before formal admission.")
                })?;
            let manifest = plugin
                .manifest
                .as_ref()
                .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin has no valid manifest."))?;
            let maximum = super::super::lease::required_lua_runtime(manifest)?.max_active_sessions;
            let mut calls = owner.calls.lock().map_err(|_| calls_error())?;
            let total_bytes = calls
                .values()
                .map(|call| call.input_bytes)
                .chain(
                    owner
                        .pinned
                        .lock()
                        .map_err(|_| calls_error())?
                        .values()
                        .map(|session| session.input_bytes),
                )
                .try_fold(input_bytes, usize::checked_add);
            if total_bytes.is_none_or(|bytes| bytes > owner.settings.parent.max_queued_bytes) {
                return Err(VulcanPluginRuntimeError::new(
                    "Formal invocation input byte capacity is exhausted.",
                ));
            }
            if calls.len() >= owner.settings.parent.max_operations {
                return Err(VulcanPluginRuntimeError::new(
                    "Formal invocation retention capacity is exhausted; inspect and acknowledge retained calls.",
                ));
            }
            if owner.host.runtime().usage().map_err(core_error)?.closing {
                return Err(VulcanPluginRuntimeError::new(
                    "Formal plugin runtime is closing.",
                ));
            }
            self.reserve_session_slot_locked(&tool.plugin_id, maximum, &_publication)?;
            let call = Arc::new(FormalCall {
                id: Uuid::new_v4().to_string(),
                plugin,
                owner_session_id,
                input_bytes,
                cancelled: AtomicBool::new(false),
                state: Mutex::new(CallState {
                    supervised: true,
                    ..CallState::default()
                }),
            });
            calls.insert(call.id.clone(), Arc::clone(&call));
            call
        };
        let mut observation = ObservationGuard {
            call: Arc::clone(&call),
            completed: false,
        };
        let manager = self.clone();
        let supervisor_owner = Arc::clone(&owner);
        let supervisor_call = Arc::clone(&call);
        let supervision = SupervisionGuard {
            call: Arc::clone(&call),
            completed: false,
        };
        // This task uses bounded core status polling; only finite package preparation enters Tokio's blocking pool.
        // 此任务使用有界核心状态轮询；仅有限包准备进入 Tokio 阻塞池。
        tokio::spawn(async move {
            let mut supervision = supervision;
            manager
                .supervise_formal_call(
                    supervisor_owner,
                    supervisor_call,
                    Preparation {
                        workspace: workspace_root,
                        arguments,
                        finalizer_arguments,
                        deadline,
                    },
                )
                .await;
            supervision.completed = true;
        });
        loop {
            let observed = call.lock()?.result.clone();
            if let Some(result) = observed {
                observation.completed = true;
                if result.is_ok() {
                    self.forget_formal_call(&call.id, &call.owner_session_id)?;
                }
                return result.map_err(|error| {
                    VulcanPluginRuntimeError::new(format!(
                        "{error} [formal invocation {}]",
                        call.id
                    ))
                });
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
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
            .filter(|call| call.owner_session_id == owner_session_id)
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
            .filter(|call| call.owner_session_id == owner_session_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Owned formal invocation was not found.")
            })?;
        {
            let state = call.lock()?;
            if !state.retired || state.supervised || state.result.is_none() {
                return Err(VulcanPluginRuntimeError::new(
                    "Formal invocation cleanup has not completed.",
                ));
            }
            if let Some(operation) = &state.operation {
                owner
                    .host
                    .runtime()
                    .forget_operation(operation.id())
                    .map_err(core_error)?;
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
        input: Preparation,
    ) {
        let manager = self.clone();
        let prepare_owner = Arc::clone(&owner);
        let prepare_call = Arc::clone(&call);
        if let Ok(mut state) = call.lock() {
            state.preparing = true;
        } else {
            return;
        }
        let preparation_guard = PreparationGuard {
            call: Arc::clone(&call),
        };
        let preparation = tokio::task::spawn_blocking(move || {
            let _preparation_guard = preparation_guard;
            manager.prepare_formal_call(&prepare_owner, &prepare_call, input)
        })
        .await
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Formal preparation task failed: {error}"))
        })
        .and_then(|result| result);
        let result = self.finish_formal_call(&owner, &call, preparation).await;
        // Errors in evidence or retirement keep the record and capacity instead of pretending cleanup completed.
        // 证据或退役错误保留记录及容量，不伪称清理完成。
        if let Ok(mut state) = call.lock() {
            state.supervised = false;
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
        input: Preparation,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if call.cancelled.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(
                "Formal invocation cancelled before preparation.",
            ));
        }
        let manifest = call
            .plugin
            .manifest
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin has no valid manifest."))?;
        let runtime = super::super::lease::required_lua_runtime(manifest)?;
        {
            // Plugin aggregate creation is atomic under the same host publication gate, never a guessed fallback.
            // 插件聚合在同一宿主发布门下原子创建，绝不是猜测性回退。
            let _publication = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
            match owner.host.runtime().plugin(&call.plugin.id) {
                Ok(_) => {}
                Err(error) if error.code == EmbeddedErrorCode::NotFound => {
                    let parent = &owner.settings.parent;
                    owner
                        .host
                        .runtime()
                        .register_plugin(
                            call.plugin.id.clone(),
                            EmbeddedPluginConfig {
                                max_registered_pools: parent.max_registered_pools,
                                max_sessions: parent.max_sessions,
                                max_resident_vms: parent.max_resident_vms,
                                max_running_calls: parent.max_running_calls,
                                max_queued_calls: parent.max_queued_calls,
                                max_queued_bytes: parent.max_queued_bytes,
                                max_operations: parent.max_operations,
                            },
                        )
                        .map_err(core_error)?;
                }
                Err(error) => return Err(core_error(error)),
            }
        }
        let shutdown_ms = owner.settings.shutdown_timeout_ms;
        let pool = owner
            .host
            .register_bundle_pool(VulcanPluginBundlePoolRequest {
                plugin_id: call.plugin.id.clone(),
                plugin_generation: call.plugin.generation,
                workspace_root: input.workspace,
                security_partition: format!("tool:{}", call.owner_session_id),
                scope: VulcanPluginBridgeLeaseScope::Enabled,
                policy: PluginPoolConfig {
                    kind: PoolKind::Shared,
                    min_resident_vms: 0,
                    max_resident_vms: 1,
                    max_running_calls: 1,
                    max_queued_calls: owner.settings.parent.max_queued_calls,
                    reuse: InstanceReuse::SingleCall,
                    serial: true,
                    backend: ExecutionBackend::InProcess,
                    idle_ttl_ms: None,
                    max_uses: None,
                },
                finalizer: Some(ModuleFinalizer {
                    export: runtime.shutdown_function.clone(),
                    arguments: input.finalizer_arguments,
                    timeout_ms: shutdown_ms,
                }),
            })
            .map_err(core_error)?;
        call.lock()?.pool = Some(pool.clone());
        // A lifecycle transition cannot interleave the final current-generation check and submission.
        // 生命周期转换不能插入最后的当前代次检查与提交之间。
        let _publication =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        let snapshot = self.snapshot();
        let current = snapshot.plugins.get(&call.plugin.id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Plugin disappeared before submission.")
        })?;
        if current.generation != call.plugin.generation
            || current.runtime_state != crate::VulcanPluginRuntimeState::Enabled
        {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin generation changed before formal submission.",
            ));
        }
        if call.cancelled.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(
                "Formal invocation cancelled before submission.",
            ));
        }
        let remaining = input
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(
                    "Formal invocation deadline expired during preparation.",
                )
            })?;
        let operation = owner
            .host
            .runtime()
            .submit(
                EmbeddedCall {
                    pool_id: pool,
                    export: runtime.invoke_function.clone(),
                    arguments: input.arguments,
                    context: LuaInvocationContext::default(),
                },
                remaining,
            )
            .map_err(core_error)?;
        call.lock()?.operation = Some(operation);
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
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
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
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
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
        } else {
            preparation.and_then(|()| {
                Err(VulcanPluginRuntimeError::new(
                    "Formal submission produced no operation identity.",
                ))
            })
        };
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
        self.release_session_slot_checked(&call.plugin.id)?;
        call.lock()?.retired = true;
        result
    }
}

/// Convert the original core business value and independent finalization outcome to the legacy result shape.
/// 将原核心业务值及独立关闭结果转换为旧结果形状。
/// `snapshot` must be terminal; preserve both failure messages without replay or fallback parsing.
/// `snapshot` 必须为终态；保留两阶段失败消息，不重放或回退解析。
fn project_result(snapshot: &OperationSnapshot) -> Result<Value, VulcanPluginRuntimeError> {
    let (business, closing) = if let Some(finalization) = &snapshot.finalization {
        (
            finalization.business.result(),
            finalization
                .outcome
                .as_ref()
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Terminal finalization has no outcome.")
                })?
                .result()
                .map(|_| ()),
        )
    } else {
        (
            match (&snapshot.value, &snapshot.error) {
                (Some(value), None) => Ok(value.clone()),
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
    let business = business
        .map_err(core_error)
        .and_then(|frame| super::super::lease::parse_plugin_result(&json!({"result":frame})));
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
        .filter(|call| call.owner_session_id == owner_session_id)
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
