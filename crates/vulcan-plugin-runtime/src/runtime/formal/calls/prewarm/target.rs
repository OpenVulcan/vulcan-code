//! Bounded convergence observes scheduler readiness while preserving every actual initialization.
//! 有界收敛观测调度器就绪状态，同时保留每次真实初始化。

use super::*;

/// One explicit target owns request metadata and handles, never independent VM counters.
/// 单个明确目标拥有请求元数据及句柄，绝不拥有独立 VM 计数。
pub(in crate::runtime::formal::calls) struct TargetProgress {
    /// Immutable caller selection survives preparation and physical cleanup.
    /// 不可变调用方选择在准备及物理清理后保留。
    pub(in crate::runtime::formal::calls) request: VulcanPluginPrewarmTargetRequest,
    /// Original trigger provenance is retained with the same input budget as its target.
    /// 原触发来源与其目标保留在同一输入预算中。
    pub(in crate::runtime::formal::calls) automatic: automatic::AutomaticAdmission,
    /// Transfer evidence distinguishes cancellation before startup from actual supervision.
    /// 转移证据区分启动前取消及真实监督。
    pub(in crate::runtime::formal::calls) started: bool,
    /// Earlier terminal operations remain inspectable until explicit acknowledgement.
    /// 先前终态操作在显式确认前保持可检查。
    pub(in crate::runtime::formal::calls) completed: Vec<OperationHandle>,
    /// Actual submissions, bounded by the original target independently of current idle count.
    /// 真实提交次数，由原目标约束，独立于当前空闲计数。
    attempts: usize,
    /// Captured authoritative observation is historical completion evidence.
    /// 捕获的权威观测为历史完成证据。
    satisfied: Option<EmbeddedReusablePoolSnapshot>,
}

impl TargetProgress {
    /// Build inert progress only for a target admission; other call intents return none.
    /// 仅为目标入场构建未启动进度；其他调用意图返回空值。
    pub(in crate::runtime::formal::calls) fn for_admission(
        admission: &CallAdmission,
    ) -> Option<Self> {
        match admission {
            CallAdmission::PrewarmTarget { request, automatic } => Some(Self {
                request: request.clone(),
                automatic: automatic.clone(),
                started: false,
                completed: Vec::new(),
                attempts: 0,
                satisfied: None,
            }),
            _ => None,
        }
    }

    /// Reserve request copies and the maximum retained handle array within limit, rejecting overflow.
    /// 在 limit 内预留请求副本及最大保留句柄数组，拒绝溢出。
    /// Return zero for ordinary calls; all reservations remain charged until receipt acknowledgement.
    /// 普通调用返回零；全部预留在回执确认前持续计费。
    pub(in crate::runtime::formal::calls) fn retained_bytes(
        admission: &CallAdmission,
        limit: usize,
    ) -> Result<usize, VulcanPluginRuntimeError> {
        let CallAdmission::PrewarmTarget { request, automatic } = admission else {
            return Ok(0);
        };
        // Pending preparation and retained history both own the original request before startup.
        // 启动前，待处理准备及保留历史均拥有原请求。
        let request_bytes = luaskills::runtime::embedded::json_size(&(request, automatic), limit)
            .map_err(core_error)?;
        request
            .target_idle
            .checked_mul(std::mem::size_of::<OperationHandle>())
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Self>()))
            .and_then(|bytes| bytes.checked_add(automatic.permit.retained_bytes()))
            .and_then(|bytes| {
                request_bytes
                    .checked_mul(2)
                    .and_then(|requests| bytes.checked_add(requests))
            })
            .filter(|bytes| *bytes <= limit)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(
                    "Prewarm target metadata exceeds the retained input budget.",
                )
            })
    }

    /// Project the original request and confirmed history; return native observation errors unchanged.
    /// 投影原请求及已确认历史；原样返回原生观测错误。
    pub(in crate::runtime::formal::calls) fn status(
        &self,
    ) -> Result<VulcanPluginPrewarmTargetStatus, VulcanPluginRuntimeError> {
        Ok(VulcanPluginPrewarmTargetStatus {
            request: self.request.clone(),
            automatic: self.automatic.origin.clone(),
            started: self.started,
            completed: self
                .completed
                .iter()
                .map(OperationHandle::snapshot)
                .collect::<Result<_, _>>()
                .map_err(core_error)?,
            satisfied: self.satisfied.clone(),
        })
    }
}

impl VulcanPluginRuntimeManager {
    /// Converge the original borrowed domain before deadline under scope, without replaying any operation.
    /// 在 deadline 前按 scope 收敛原借用域，不重放任何操作。
    /// Return only after a confirmed idle observation satisfies the target or an explicit failure occurs.
    /// 仅在已确认空闲观测满足目标或发生明确失败后返回。
    pub(in crate::runtime::formal::calls) async fn converge_prewarm_target(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
        deadline: Instant,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> Result<(), VulcanPluginRuntimeError> {
        loop {
            if self.advance_prewarm_target(owner, call, deadline, scope)? {
                return Ok(());
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }

    /// Advance at most one initialization using owner, call, deadline and scope; true means target observed.
    /// 使用 owner、call、deadline 及 scope 至多推进一次初始化；真值表示已观测目标。
    /// All operations remain in the same call; false requests another asynchronous observation.
    /// 全部操作保留在同一调用中；假值要求再次异步观测。
    fn advance_prewarm_target(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
        deadline: Instant,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        if call.cancelled.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm target was cancelled.",
            ));
        }
        if automatic::is_closed(call)? {
            return Err(VulcanPluginRuntimeError::new(
                "Automatic prewarm lifecycle is closed.",
            ));
        }
        initialized::remaining(deadline)?;
        // Preserve the last handle until it is terminal and its outcome has been checked.
        // 在终结并检查结果前保留最后句柄。
        let operation = call.lock()?.operation.clone();
        if let Some(operation) = operation {
            // Inspect authoritative native evidence without reconstructing state from host counters.
            // 检查权威原生证据，不从宿主计数器重建状态。
            let snapshot = operation.snapshot().map_err(core_error)?;
            if !snapshot.phase.is_terminal() {
                return Ok(false);
            }
            project_result(&snapshot)?;
        }
        // Publication cannot change authority between validation and submission or target completion.
        // 发布不能在校验与提交或目标完成之间改变权威。
        let _publication =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        self.validate_formal_submission(call, scope)?;
        self.validate_reusable_submission(owner, call)?;
        // A policy change fences every later initialization while original native work retains its receipt.
        // 策略变化隔离每次后续初始化，原原生工作仍保留其回执。
        let permission = automatic::validate_call(owner, call, &_publication)?;
        // Keep the short lifecycle gate until native enqueue returns; callbacks run outside this boundary.
        // 保留短生命周期门直至原生入队返回；回调在此边界之外运行。
        let _admission = permission
            .as_ref()
            .map(VulcanPluginAutomaticPrewarmPermit::admit)
            .transpose()?;
        // Serialize ownership transfer and evidence changes under the original call lock.
        // 在原调用锁下排序归属转移及证据变化。
        let mut state = call.lock()?;
        // Keep the original borrowed pool identity without selecting another domain.
        // 保留原借用池身份，不选择另一域。
        let pool = state
            .pool
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Prewarm target has no original pool."))?
            .clone();
        // Inspect authoritative native evidence without reconstructing state from host counters.
        // 检查权威原生证据，不从宿主计数器重建状态。
        let snapshot = owner
            .host
            .runtime()
            .reusable_pool_status(&pool)
            .map_err(core_error)?;
        initialized::remaining(deadline)?;
        if call.cancelled.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm target was cancelled.",
            ));
        }
        if snapshot.closing || snapshot.admission_blocked {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm target pool admission is closed or blocked.",
            ));
        }
        // Read bounded target ownership independently of physical allocation counts.
        // 独立于物理分配计数读取有界目标归属。
        let progress = state.prewarm.as_mut().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal invocation has no prewarm target.")
        })?;
        if snapshot.ready >= progress.request.target_idle {
            progress.satisfied = Some(snapshot);
            return Ok(true);
        }
        if progress.attempts >= progress.request.target_idle {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm target was not reached within its bounded initialization attempts.",
            ));
        }
        if snapshot.physical.resident >= snapshot.max_resident_vms {
            return Ok(false);
        }
        // Reserve exactly one history slot before moving ownership or admitting any new side effects.
        // 移动归属或接纳任何新副作用前，精确预留一个历史槽。
        progress.completed.try_reserve_exact(1).map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Prewarm history reservation failed: {error}"))
        })?;
        if let Some(previous) = state.operation.take() {
            state
                .prewarm
                .as_mut()
                .expect("validated target")
                .completed
                .push(previous);
        }
        // Native admission owns the real queue and aggregate resource limits.
        // 原生入场拥有真实队列及聚合资源上限。
        let operation = owner
            .host
            .runtime()
            .prewarm_instance(
                EmbeddedPrewarm {
                    pool_id: pool,
                    context: LuaInvocationContext::default(),
                },
                initialized::remaining(deadline)?,
            )
            .map_err(core_error)?;
        // Capture each actual attempt before the completed history can be popped or forgotten.
        // 完成历史可以弹出或遗忘之前，捕获每次实际尝试。
        crate::runtime::native_identity::observe_operation(
            owner.host.runtime().id(),
            &operation,
            call.native_identity_owner(),
        );
        state.operation = Some(operation);
        state.prewarm.as_mut().expect("validated target").attempts += 1;
        Ok(false)
    }

    /// Return the recorded completion observation for call; never infer success from a last operation.
    /// 返回 call 记录的完成观测；绝不从最后操作推断成功。
    pub(in crate::runtime::formal::calls) fn prewarm_target_result(
        &self,
        call: &FormalCall,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // Serialize ownership transfer and evidence changes under the original call lock.
        // 在原调用锁下排序归属转移及证据变化。
        let state = call.lock()?;
        // Read bounded target ownership independently of physical allocation counts.
        // 独立于物理分配计数读取有界目标归属。
        let progress = state.prewarm.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal invocation has no prewarm target.")
        })?;
        // Success requires the recorded scheduler observation, not merely a successful initializer.
        // 成功要求已记录调度器观测，而非仅成功初始化器。
        let satisfied = progress.satisfied.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Prewarm target has no confirmed completion observation.")
        })?;
        Ok(json!({
            "poolId": satisfied.pool_id,
            "targetIdle": progress.request.target_idle,
            "observedReady": satisfied.ready,
            "initializations": progress.attempts,
        }))
    }
}
