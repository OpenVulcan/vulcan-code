//! Unpublished initialization uses the original formal parent, call registry and capacity admission.
//! 未发布初始化使用原正式父级、调用注册表及容量入场。

use super::*;
use crate::VulcanPluginActivationCandidate;

impl VulcanPluginRuntimeManager {
    /// Invoke candidate's declared enable callback once under the original lifecycle deadline.
    /// 在原生命周期期限内单次调用 candidate 声明的启用回调。
    /// Return durable observed completion; the host must coordinate predecessor drain and later route publication.
    /// 返回持久观测完成结果；宿主必须协调前驱排空及后续路由发布。
    pub async fn enable_activation_candidate(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Use the existing lifecycle budget rather than introducing another timeout authority.
        // 使用既有生命周期预算，不引入另一个超时权威。
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Candidate enable deadline is out of range.")
            })?;
        self.invoke_formal_candidate_operation(candidate, CandidateOperation::Enable, deadline)
            .await
    }

    /// Compensate candidate's observed enable through its declared disable callback, without replay.
    /// 通过声明的停用回调补偿 candidate 已观测启用，不重放。
    /// Return observed cleanup; any compensation attempt permanently excludes this request from publication.
    /// 返回观测清理结果；任何补偿尝试均永久禁止发布此请求。
    pub async fn rollback_activation_candidate_enable(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Compensation shares the existing bounded lifecycle observation budget.
        // 补偿共享既有有界生命周期观测预算。
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Candidate compensation deadline is out of range.")
            })?;
        self.invoke_formal_candidate_operation(
            candidate,
            CandidateOperation::EnableRollback,
            deadline,
        )
        .await
    }

    /// Initialize candidate's declared Lua module under the existing probe deadline without public publication.
    /// 在既有探测期限内初始化 candidate 声明的 Lua 模块，不进行公开发布。
    /// Success requires tracked initialization and cleanup; timeout may return earlier while actual ownership remains.
    /// 成功要求受跟踪初始化及清理完成；超时可能提前返回，而实际所有权继续保留。
    /// Package top-level effects are possible and are never replayed here.
    /// 插件包顶层副作用可能发生，此处绝不重放。
    /// This does not invoke install callbacks or validate external connections; unfinished work keeps commit and abort fenced.
    /// 此接口不调用安装回调或验证外部连接；未完成工作持续阻止提交及中止。
    pub async fn probe_activation_candidate(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // One original deadline includes authority checks, dependency preparation and actual initialization.
        // 单个原始截止时间包含权威检查、依赖准备及实际初始化。
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(VULCAN_PLUGIN_PROBE_TIMEOUT_MS))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Candidate probe deadline is out of range.")
            })?;
        self.invoke_formal_candidate_operation(candidate, CandidateOperation::Probe, deadline)
            .await
    }

    /// Invoke candidate's declared install callback with the original lifecycle envelope and bounded observation.
    /// 使用原生命周期信封及有界观测调用 candidate 声明的安装回调。
    /// Returns the actual callback result after cleanup; absent declarations succeed without executing Lua.
    /// 清理后返回实际回调结果；缺失声明直接成功，不执行 Lua。
    /// A declared callback records one durable attempt before preparation; repeated requests are rejected even after retirement.
    /// 声明回调在准备前记录单次持久尝试；即使已经退役也拒绝重复请求。
    /// Cancellation or uncertain persistence requires querying the original installation receipt, never automatic replay.
    /// 取消或持久化不确定要求查询原安装回执，绝不自动重放。
    pub async fn install_activation_candidate(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Lifecycle preparation and execution share the existing lifecycle budget from API entry.
        // 从 API 入口起，生命周期准备与执行共享既有生命周期预算。
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Candidate installation deadline is out of range.")
            })?;
        self.invoke_formal_candidate_operation(candidate, CandidateOperation::Install, deadline)
            .await
    }

    /// Run candidate's exact operation under the original deadline; return its tracked result without replay.
    /// 在原 deadline 下运行 candidate 精确 operation；返回受跟踪结果，不重放。
    pub(super) async fn invoke_formal_candidate_operation(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
        operation: CandidateOperation,
        deadline: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Only an inspected transaction can create the operation's exact unpublished authority.
        // 只有已检查事务可以创建操作的精确未发布权威。
        let authority = ActivationExecution::new(&self.state, candidate, operation)?;
        // Preserve the existing no-callback contract for content-only packages.
        // 为纯内容插件包保留既有无回调契约。
        let manifest = authority.plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Activation candidate has no valid manifest.")
        })?;
        let Some(lua_runtime) = &manifest.lua_runtime else {
            return Ok(());
        };
        // Frozen lifecycle declarations are the sole source of exports and events.
        // 固定生命周期声明是导出及事件的唯一来源。
        let (event, declaration) = match operation {
            CandidateOperation::Probe => (None, None),
            CandidateOperation::Install => {
                (Some("install"), lua_runtime.lifecycle.install.as_ref())
            }
            CandidateOperation::Enable => (Some("enable"), lua_runtime.lifecycle.enable.as_ref()),
            CandidateOperation::EnableRollback => (
                Some("enable_rollback"),
                lua_runtime.lifecycle.disable.as_ref(),
            ),
        };
        if event.is_some() && declaration.is_none() {
            return Ok(());
        }
        // Own the declaration before moving the private authority into the formal call.
        // 将私有权威移动至正式调用前，取得声明的所有权。
        let export = declaration.cloned();
        // Scope is derived from the typed operation and remains fixed through native cleanup.
        // 作用域从类型化操作派生，并在原生清理期间持续固定。
        let scope = operation.scope();
        self.validate_plugin_requirements_in_scope(&authority.plugin, scope)?;
        // This is the existing parent; candidate work cannot allocate a replacement scheduler.
        // 此对象为既有父级；候选工作不能分配替代调度器。
        let owner = self.required_formal_owner()?;
        // The target remains independent from all public catalog observations.
        // 目标持续独立于全部公开目录观测。
        let plugin = authority.plugin.clone();
        // Timeout diagnostics retain the exact original durable request.
        // 超时诊断保留精确原持久请求。
        let request_id = authority.request_id;
        // The declaration decides the original lifecycle envelope; preparation adds its trusted environment path.
        // 声明决定原生命周期信封；准备过程加入可信环境路径。
        let (admission, arguments) = match export {
            Some(export) => (
                CallAdmission::ActivationLifecycle { authority, export },
                json!({"event":event.expect("declared lifecycle event"),"plugin_id":plugin.id,"request_id":request_id.to_string()}),
            ),
            None => (CallAdmission::CandidateProbe { authority }, json!({})),
        };
        // All operations retain original byte accounting, deadlines and actual resource supervision.
        // 全部操作均保留原字节计量、截止时间及实际资源监督。
        let input = Preparation {
            workspace: CallWorkspace::LifecycleEnvironment,
            arguments,
            finalizer: None,
            scope,
            security_partition: format!("candidate:{}:{request_id}:{}", plugin.id, scope.as_str()),
            deadline,
            admission,
        };
        // The same request admits one durable install attempt, before any preparation or native side effect.
        // 在任何准备或原生副作用之前，相同请求仅接纳一次持久安装尝试。
        let installation = if operation == CandidateOperation::Install {
            Some(
                candidate
                    .begin_installation_attempt()
                    .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?,
            )
        } else {
            None
        };
        // Probe initialization is effectful too; each non-install operation has one original durable intent.
        // 探测初始化同样可产生副作用；每个非安装操作具有单个原持久意图。
        let effect = match operation {
            CandidateOperation::Install => None,
            CandidateOperation::Probe => Some(crate::VulcanPluginActivationEffect::Probe),
            CandidateOperation::Enable => Some(crate::VulcanPluginActivationEffect::Enable),
            CandidateOperation::EnableRollback => {
                Some(crate::VulcanPluginActivationEffect::EnableRollback)
            }
        }
        .map(|operation| candidate.begin_effect_attempt(operation))
        .transpose()
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Retain the observation separately so both runtime errors and deadline expiry are checkpointed.
        // 单独保留观测，使运行时错误及期限到达均形成检查点。
        let result = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            self.invoke_owned_formal_call(owner, plugin, input),
        )
        .await
        .map_err(|_| VulcanPluginRuntimeError::new(format!(
            "Activation candidate {request_id} timed out; inspect its original calls for actual retirement."
        )))
        .and_then(|result| result.map(|_| ()));
        if let Some(installation) = installation {
            // A failed receipt write stays uncertain, even if the callback already returned successfully.
            // 即使回调已成功返回，回执写入失败仍保持不确定。
            installation.finish(result.is_ok()).map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Activation candidate {request_id} installation receipt is uncertain: {error}; inspect the original request."
                ))
            })?;
        }
        if let Some(effect) = effect {
            // A lost acknowledgement cannot be reported as a safely repeatable callback.
            // 丢失确认不能报告为可安全重复的回调。
            effect.finish(result.is_ok()).map_err(|error| VulcanPluginRuntimeError::new(format!(
                "Activation candidate {request_id} effect receipt is uncertain: {error}; inspect the original request."
            )))?;
        }
        result
    }

    /// Query retained candidate calls for request_id in this manager's original parent without starting or replaying work.
    /// 在此管理器原父级中查询 request_id 的保留候选调用，不启动或重放工作。
    /// Empty results mean no retained calls now, not proof that initialization never ran or the journal completed.
    /// 空结果表示当前没有保留调用，不证明初始化从未运行或日志已完成。
    pub fn activation_candidate_calls(
        &self,
        request_id: Uuid,
    ) -> Result<Vec<VulcanPluginFormalCallStatus>, VulcanPluginRuntimeError> {
        // An unattached parent has no retained work and must not be created by inspection.
        // 未附加父级没有保留工作，检查不得创建它。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(Vec::new());
        };
        owner
            .calls
            .lock()
            .map_err(|_| calls_error())?
            .values()
            .filter(|call| {
                call.activation.as_ref().is_some_and(|authority| {
                    authority.request_id == request_id && authority.operation.is_candidate()
                })
            })
            .map(|call| call.status())
            .collect()
    }

    /// Cancel and drain currently admitted candidate calls for request_id, leaving ordinary plugin calls untouched.
    /// 取消并排空 request_id 当前已接纳候选调用，保持普通插件调用不变。
    /// Returns exact original-call cleanup evidence; unfinished work continues to fence storage mutation.
    /// 返回精确原调用清理证据；未完成工作继续阻止存储变更。
    pub async fn drain_activation_candidate_calls(
        &self,
        request_id: Uuid,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        tokio::runtime::Handle::try_current().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Candidate drain requires a Tokio runtime: {error}"
            ))
        })?;
        // The drain supervisor survives an interrupted external waiter.
        // 排空监督者在外部等待者中断后继续存活。
        let manager = self.clone();
        tokio::spawn(async move {
            // Query original ownership only; absence never creates a replacement parent.
            // 仅查询原所有权；缺失绝不创建替代父级。
            let Some(owner) = manager.formal_runtime_owner()? else {
                return Ok(VulcanPluginDrainReport::default());
            };
            // Exact request filtering prevents cancelling old-version work in the same plugin namespace.
            // 精确请求过滤防止取消相同插件命名空间中的旧版本工作。
            let calls = owner
                .calls
                .lock()
                .map_err(|_| calls_error())?
                .values()
                .filter(|call| {
                    call.activation.as_ref().is_some_and(|authority| {
                        authority.request_id == request_id && authority.operation.is_candidate()
                    })
                })
                .cloned()
                .collect();
            manager.drain_selected_formal_calls(&owner, calls).await
        })
        .await
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Candidate drain supervisor failed: {error}"))
        })?
    }
}
