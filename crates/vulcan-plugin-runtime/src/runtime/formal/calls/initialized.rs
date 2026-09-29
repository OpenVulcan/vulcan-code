//! Temporary core sessions preserve Action admission and probe initialization without a second scheduler.
//! 临时核心会话保留 Action 入场及探测初始化，不建立第二个调度器。

use super::*;
use crate::runtime::action_status::ActionWorkGuard;
use crate::runtime::execution::ActionExecution;
use luaskills::RuntimeRequestContext;
use luaskills::runtime::embedded::EmbeddedSessionPhase;

/// One private core session and its independent initialization acknowledgement checkpoints.
/// 单个私有核心会话及其独立初始化确认检查点。
pub(super) struct InitializedCall {
    /// Original core session identity never changes during cleanup recovery.
    /// 原核心会话身份在清理恢复期间绝不改变。
    session_id: String,
    /// Initialization retains actual ownership until it is terminal and explicitly forgotten.
    /// 初始化保留实际所有权，直到终结且显式遗忘。
    initialization: Option<OperationHandle>,
    /// Forgetting the session is recorded before any later fallible cleanup step.
    /// 在任何后续可能失败的清理步骤之前记录会话遗忘。
    session_forgotten: bool,
}
impl InitializedCall {
    /// Borrow the original initialization observer while retained; absence means its exact cleanup checkpoint completed.
    /// 保留期间借用原初始化观察者；缺失表示其精确清理检查点已完成。
    pub(super) fn initialization_operation(&self) -> Option<&OperationHandle> {
        self.initialization.as_ref()
    }

    /// Keep the exact opening receipt; neither identity is synthesized by the host.
    /// 保留精确开启回执；两个身份均不由宿主合成。
    pub(super) fn new(session_id: String, initialization: OperationHandle) -> Self {
        Self {
            session_id,
            initialization: Some(initialization),
            session_forgotten: false,
        }
    }
}

/// Return the original deadline's remaining duration or a precise expiration error.
/// 返回原截止时间的剩余时长，或精确到期错误。
pub(super) fn remaining(deadline: Instant) -> Result<Duration, VulcanPluginRuntimeError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal invocation deadline expired during preparation.")
        })
}

impl VulcanPluginRuntimeManager {
    /// Invoke one transient Action with its declared export, original request workspace and cancellation owner.
    /// 使用声明导出、原请求工作区及取消所有者调用单个瞬时 Action。
    /// Return the version-one business value only after actual retirement, or its explicit error.
    /// 仅在实际退役后返回版本一业务值，或返回明确错误。
    pub(in crate::runtime) async fn invoke_formal_action(
        &self,
        plugin: VulcanPluginDescriptor,
        workspace: PathBuf,
        export: String,
        arguments: Value,
        execution: ActionExecution,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // All work uses the attached parent and original API-entry deadline.
        // 全部工作使用已附加父级及原接口入场截止时间。
        let owner = self.required_formal_owner()?;
        execution.record.phase("initializing");
        // This private session is never registered as the plugin's persistent UI runtime.
        // 此私有会话绝不登记为插件的持久 UI 运行时。
        let input = Preparation {
            workspace: CallWorkspace::Provided(workspace),
            arguments,
            finalizer: None,
            scope: VulcanPluginBridgeLeaseScope::Enabled,
            security_partition: format!("action:{}:{}", plugin.id, plugin.generation),
            deadline: execution.deadline.into_std(),
            admission: CallAdmission::Action { export, execution },
        };
        self.invoke_owned_formal_call(owner, plugin, input).await
    }

    /// Validate the original bundle in its enabling scope without invoking a plugin business export.
    /// 在启用中作用域校验原 bundle，不调用插件业务导出。
    /// The descriptor and deadline are frozen by the public probe entry; success follows actual retirement.
    /// 描述符及截止时间由公开探测入口冻结；成功在实际退役后返回。
    pub(in crate::runtime) async fn invoke_formal_probe(
        &self,
        plugin: VulcanPluginDescriptor,
        deadline: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Missing formal ownership is an explicit failure, never a legacy-engine fallback.
        // 缺少正式所有权为明确失败，绝不回退旧引擎。
        let owner = self.required_formal_owner()?;
        // The real module loader validates all declared exports during core initialization.
        // 真实模块加载器在核心初始化期间校验全部声明导出。
        let input = Preparation {
            workspace: CallWorkspace::ExecutionPackage,
            arguments: json!({}),
            finalizer: None,
            scope: VulcanPluginBridgeLeaseScope::EnablingProbe,
            security_partition: format!("probe:{}:{}", plugin.id, plugin.generation),
            deadline,
            admission: CallAdmission::Probe,
        };
        self.invoke_owned_formal_call(owner, plugin, input)
            .await
            .map(|_| ())
    }

    /// Complete any separate initialization and admit Action business exactly once.
    /// 完成可选独立初始化，并且仅一次接纳 Action 业务。
    /// Input owns the original receiver and arguments; errors proceed to the same physical cleanup.
    /// 输入拥有原接收器及参数；错误进入同一物理清理。
    pub(super) async fn complete_initialized_admission(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
        input: Preparation,
    ) -> Result<(), VulcanPluginRuntimeError> {
        match input.admission {
            CallAdmission::Controller { .. }
            | CallAdmission::Prewarm { .. }
            | CallAdmission::Lifecycle { .. } => Ok(()),
            CallAdmission::PrewarmTarget { .. } => {
                self.converge_prewarm_target(owner, call, input.deadline, input.scope)
                    .await
            }
            CallAdmission::Probe | CallAdmission::CandidateProbe { .. } => {
                self.wait_call_initialization(call).await
            }
            CallAdmission::Action {
                export,
                mut execution,
            } => {
                execution.record.phase("initialization_wait");
                execution
                    .wait(self.wait_call_initialization(call))
                    .await??;
                execution.remaining_ms()?;
                // Authority and cancellation admission must be checked again after real initialization.
                // 真实初始化之后必须再次检查权威及取消入场。
                let _publication = self.state.session_registration_gate.lock().map_err(|_| {
                    VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
                })?;
                self.validate_formal_submission(call, input.scope)?;
                execution.record.start()?;
                // Keep the exact temporary session while submitting its sole business operation.
                // 提交唯一业务操作期间保留精确临时会话。
                let mut state = call.lock()?;
                // Initialization can never manufacture a replacement session if the original is missing.
                // 原会话缺失时，初始化绝不能制造替代会话。
                let session = state.initialized.as_ref().ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Prepared Action has no core session.")
                })?;
                // Only the original host-authorized workspace can accompany this business operation.
                // 仅原宿主授权工作区可以伴随此业务操作。
                let CallWorkspace::Provided(workspace) = input.workspace else {
                    return Err(VulcanPluginRuntimeError::new(
                        "Prepared Action has no authorized workspace.",
                    ));
                };
                // The correlated operation owns callback timing in addition to the full-call physical guard.
                // 关联操作拥有回调计时，并且另有整个调用的物理守卫。
                let work = ActionWorkGuard::new(Arc::clone(&execution.record), None)?;
                // Allocate one non-reusable trusted request correlation from the existing authority.
                // 从既有权威分配一个不可复用的可信请求关联。
                let request = self
                    .state
                    .next_request_id
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                        value.checked_add(1)
                    })
                    .map_err(|_| {
                        VulcanPluginRuntimeError::new("Plugin request identity is exhausted.")
                    })?;
                // Bind the exact declared export and original Action envelope to the actual core operation.
                // 将精确声明导出及原 Action 信封绑定至实际核心操作。
                let operation = owner
                    .host
                    .submit_action(
                        &session.session_id,
                        EmbeddedCall {
                            pool_id: state.pool.clone().ok_or_else(|| {
                                VulcanPluginRuntimeError::new("Prepared Action has no pool.")
                            })?,
                            export,
                            arguments: input.arguments,
                            context: LuaInvocationContext {
                                request_context: Some(RuntimeRequestContext {
                                    request_id: Some(request.to_string()),
                                    ..RuntimeRequestContext::default()
                                }),
                                ..LuaInvocationContext::default()
                            },
                        },
                        &workspace,
                        input.deadline,
                        work,
                    )
                    .map_err(core_error)?;
                state.operation = Some(operation);
                Ok(())
            }
        }
    }

    /// Observe the original initialization, requesting cancellation only on the same registered call.
    /// 观测原初始化，仅针对同一已登记调用请求取消。
    /// Returns real initialization success or error; an in-progress native callback remains physically owned.
    /// 返回真实初始化成功或错误；进行中的原生回调继续具有物理归属。
    async fn wait_call_initialization(
        &self,
        call: &FormalCall,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Clone only the core observer, not a VM or another initialization attempt.
        // 仅克隆核心观测者，不克隆 VM 或另一次初始化尝试。
        let operation = call
            .lock()?
            .initialized
            .as_ref()
            .and_then(|session| session.initialization.clone())
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Formal initialization has no operation identity.")
            })?;
        loop {
            if call.cancelled.load(Ordering::Acquire) {
                operation.cancel().map_err(core_error)?;
            }
            // Terminal evidence originates from the actual core operation, not elapsed wall-clock time.
            // 终态证据来自真实核心操作，不来自已流逝墙钟时间。
            let snapshot = operation.snapshot().map_err(core_error)?;
            if snapshot.phase.is_terminal() {
                return match (&snapshot.value, &snapshot.error) {
                    (Some(_), None) => Ok(()),
                    (None, Some(error)) => Err(core_error(error.clone())),
                    _ => Err(VulcanPluginRuntimeError::new(
                        "Terminal initialization has inconsistent result evidence.",
                    )),
                };
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }

    /// Close and forget the original temporary session and initialization using independent checkpoints.
    /// 使用独立检查点关闭并遗忘原临时会话及初始化。
    /// No temporary state requires no session cleanup; every admitted identity stays owned on error.
    /// 没有临时状态时无需会话清理；错误时保留每个已接纳身份的归属。
    pub(super) async fn close_initialized_call(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Snapshot the actual session identity without holding the host evidence lock across a wait.
        // 对真实会话身份取快照，不跨等待持有宿主证据锁。
        let original = call
            .lock()?
            .initialized
            .as_ref()
            .map(|session| (session.session_id.clone(), session.session_forgotten));
        if let Some((session_id, false)) = original {
            owner
                .host
                .runtime()
                .close_session(&session_id)
                .map_err(core_error)?;
            loop {
                // The core owns native work and resource retirement; only Closed permits forgetting.
                // 核心拥有原生工作及资源退役；仅 Closed 允许遗忘。
                let session = owner
                    .host
                    .runtime()
                    .session(&session_id)
                    .map_err(core_error)?;
                if session.phase == EmbeddedSessionPhase::Closed {
                    if session.finalization_operation.is_some() {
                        return Err(VulcanPluginRuntimeError::new(
                            "Temporary Action or probe unexpectedly has a business finalizer.",
                        ));
                    }
                    break;
                }
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
            }
            owner
                .host
                .runtime()
                .forget_session(&session_id)
                .map_err(core_error)?;
            call.lock()?
                .initialized
                .as_mut()
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new(
                        "Temporary session ownership disappeared during retirement.",
                    )
                })?
                .session_forgotten = true;
        }
        // A successful session checkpoint must survive any later operation-forgetting failure.
        // 成功的会话检查点必须跨后续操作遗忘失败存活。
        let initialization = call
            .lock()?
            .initialized
            .as_ref()
            .and_then(|session| session.initialization.clone());
        if let Some(operation) = initialization {
            owner
                .host
                .runtime()
                .forget_operation(operation.id())
                .map_err(core_error)?;
            call.lock()?
                .initialized
                .as_mut()
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new(
                        "Initialization ownership disappeared during retirement.",
                    )
                })?
                .initialization = None;
        }
        Ok(())
    }
}
