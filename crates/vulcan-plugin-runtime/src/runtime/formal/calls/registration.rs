//! Shared bounded registration and exactly-once scheduling of original formal inputs.
//! 原正式输入的共享有界注册及恰好一次调度。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Retire call only while its original input is still unstarted; active workers keep physical ownership.
    /// 仅在原输入尚未启动时退役 call；活动工作者保持物理归属。
    /// Return accounting failures without discarding the pending request or claiming completed cleanup.
    /// 返回记账失败，不丢弃待处理请求或宣称清理完成。
    pub(in crate::runtime::formal) fn retire_unstarted_formal_call(
        &self,
        call: &Arc<FormalCall>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Preserve publication-before-evidence lock ordering used by admission.
        // 保持入场使用的先发布、后证据锁顺序。
        let publication =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        // Serialize ownership transfer and evidence changes under the original call lock.
        // 在原调用锁下排序归属转移及证据变化。
        let mut state = call.lock()?;
        if state.pending.is_none() || state.supervised {
            return Ok(());
        }
        if !matches!(call.ownership, CallOwner::Probe) {
            self.release_session_slot_with_publication(&call.plugin.id, &publication)?;
        }
        state.pending.take();
        state.action_work.take();
        state.retired = true;
        state.result = Some(Err(VulcanPluginRuntimeError::new(
            "Formal invocation cancelled before startup.",
        )));
        Ok(())
    }

    /// Cancel and retire unstarted records after owner admission closes; return exact cleanup failures.
    /// owner 入场关闭后取消并退役未启动记录；返回精确清理失败。
    /// Started records retain their supervisor and all native operation identities.
    /// 已启动记录保留监督者及全部原生操作身份。
    pub(in crate::runtime::formal) fn retire_pending_formal_calls(
        &self,
        owner: &ManagedFormalRuntime,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Drop the registry lock before acquiring publication or individual evidence locks.
        // 获取发布或各证据锁前释放注册表锁。
        let calls: Vec<_> = owner
            .calls
            .lock()
            .map_err(|_| calls_error())?
            .values()
            .cloned()
            .collect();
        for call in calls {
            call.cancelled.store(true, Ordering::Release);
            self.retire_unstarted_formal_call(&call)?;
        }
        Ok(())
    }

    /// Register input and its exact plugin under all existing quotas without executing or scheduling Lua.
    /// 在全部既有配额下登记 input 及其精确插件，不执行或调度 Lua。
    /// Return retained call ownership; callers explicitly start or cancel the original pending input.
    /// 返回保留调用归属；调用方明确启动或取消原待处理输入。
    pub(super) fn register_owned_formal_call(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        plugin: VulcanPluginDescriptor,
        input: Preparation,
    ) -> Result<Arc<FormalCall>, VulcanPluginRuntimeError> {
        self.reap_internal_calls(owner)?;
        // Retained ownership is derived once from the same intent used for initialization and business.
        // 保留归属从用于初始化及业务的同一意图一次性派生。
        let ownership = input.admission.ownership();
        // Hold Action capacity before any worker can be queued, even if its observer disappears.
        // 在任何工作者排队前持有 Action 容量，即使其观测者消失。
        let action_work = match &input.admission {
            CallAdmission::Action { execution, .. } => {
                execution.record.check()?;
                Some(crate::runtime::action_status::ActionWorkGuard::new(
                    Arc::clone(&execution.record),
                    None,
                )?)
            }
            _ => None,
        };
        // Count all retained source-selection metadata and both envelopes before publishing ownership.
        // 发布所有权前计入全部保留的源码选择元数据及两个信封。
        let input_bytes = input.input_bytes(&ownership, &owner.settings)?;
        // Target metadata is bounded before retaining its request or starting a worker.
        // 保留请求或启动工作者之前约束目标元数据。
        let target = prewarm::TargetProgress::for_admission(&input.admission);
        // The same publication gate orders lifecycle transitions, quota reservation and exact submission.
        // 同一发布门排序生命周期转换、配额预留及精确提交。
        let call = {
            // Serialize this authority check with catalog publication.
            // 将此次权威检查与目录发布排序。
            let publication = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
            // Explicit candidate authority never consults another catalog incarnation for its target.
            // 显式候选权威绝不查询其他目录实例来取得目标。
            self.validate_formal_authority(
                &plugin,
                input.scope,
                input.admission.candidate().map(Arc::as_ref),
            )?;
            if Instant::now() >= input.deadline {
                return Err(VulcanPluginRuntimeError::new(
                    "Formal invocation deadline expired before admission.",
                ));
            }
            // Read execution policy from the frozen admitted plugin manifest.
            // 从冻结的已接纳插件清单读取执行策略。
            let manifest = plugin
                .manifest
                .as_ref()
                .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin has no valid manifest."))?;
            // The manifest remains the sole source of the plugin's active-session limit.
            // 清单仍是插件活动会话上限的唯一来源。
            let maximum =
                super::super::super::lease::required_lua_runtime(manifest)?.max_active_sessions;
            // Hold the shared call registry while inspecting or changing retained ownership.
            // 检查或变更保留所有权期间持有共享调用注册表。
            let mut calls = owner.calls.lock().map_err(|_| calls_error())?;
            // Automatic retries observe the same original intent before any new quota or deadline is retained.
            // 自动重试在保留任何新配额或截止时间前观测同一原意图。
            if let Some(original) = prewarm::automatic::original_call(&calls, &input.admission)? {
                return Ok(original);
            }
            let lifecycle_permission =
                prewarm::automatic::validate_admission(owner, &input.admission, &publication)?;
            // Keep revocation excluded until the original inert receipt is visible in the registry.
            // 原未启动回执在注册表可见前，持续排除权限撤销。
            let _lifecycle_admission = lifecycle_permission
                .as_ref()
                .map(VulcanPluginAutomaticPrewarmPermit::admit)
                .transpose()?;
            // Include retained calls, pinned sessions and cached routing in the same checked byte sum.
            // 在同一受检字节总和中包含保留调用、固定会话及缓存路由。
            let total_bytes = calls
                .values()
                .map(|call| call.input_bytes.load(Ordering::Acquire))
                .chain(
                    owner
                        .pinned
                        .lock()
                        .map_err(|_| calls_error())?
                        .values()
                        .map(|session| session.input_bytes),
                )
                .chain(std::iter::once(
                    owner
                        .reusable
                        .lock()
                        .map_err(|_| calls_error())?
                        .retained_input_bytes()?,
                ))
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
            if !matches!(ownership, CallOwner::Probe) {
                self.reserve_session_slot_locked(&plugin.id, maximum, &publication)?;
            }
            // Keep the exact invocation record independently of its observer.
            // 独立于观测者保留精确调用记录。
            let call = Arc::new(FormalCall {
                id: Uuid::new_v4().to_string(),
                plugin,
                candidate: input.admission.candidate().cloned(),
                ownership,
                input_bytes: AtomicUsize::new(input_bytes),
                cancelled: AtomicBool::new(false),
                acknowledged: AtomicBool::new(false),
                state: Mutex::new(CallState {
                    action_work,
                    pending: Some(input),
                    prewarm: target,
                    supervised: false,
                    ..CallState::default()
                }),
            });
            calls.insert(call.id.clone(), Arc::clone(&call));
            call
        };

        Ok(call)
    }

    /// Start the pending input once; already started or completed calls never schedule replacement work.
    /// 仅启动待处理输入一次；已启动或完成的调用绝不调度替代工作。
    /// Owner and call are retained originals; return executor or lock failures without consuming pending input.
    /// owner 及 call 为保留原件；执行器或锁失败时返回错误，不消费待处理输入。
    pub(super) fn start_registered_formal_call(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        call: &Arc<FormalCall>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Already transferred or retired input needs no executor merely to observe its existing receipt.
        // 仅观测既有回执时，已转移或退役输入不需要执行器。
        if call.lock()?.pending.is_none() {
            return Ok(());
        }
        tokio::runtime::Handle::try_current().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Formal invocation requires a Tokio runtime: {error}"
            ))
        })?;
        self.ensure_reuse_observer(owner);
        // Taking the original input and publishing supervision form one short atomic ownership transfer.
        // 取走原输入与发布监督状态构成单个短原子归属转移。
        let input = {
            // Serialize ownership transfer and evidence changes under the original call lock.
            // 在原调用锁下排序归属转移及证据变化。
            let mut state = call.lock()?;
            let Some(input) = state.pending.take() else {
                return Ok(());
            };
            state.supervised = true;
            if let Some(target) = &mut state.prewarm {
                target.started = true;
            }
            input
        };
        // This guard records interruption without releasing actual work.
        // 此守卫记录中断，不释放实际工作。
        let supervision = SupervisionGuard {
            call: Arc::clone(call),
            completed: false,
        };
        // Retain the original manager identity for the operation being observed.
        // 为被观测操作保留原管理器身份。
        let manager = self.clone();
        // The supervisor retains the same formal parent throughout cleanup.
        // 监督者在整个清理期间保留同一正式父级。
        let supervisor_owner = Arc::clone(owner);
        // The supervisor observes the original invocation rather than recreating it.
        // 监督者观测原调用，不重新创建调用。
        let supervisor_call = Arc::clone(call);
        tokio::spawn(async move {
            // This guard records interruption without releasing actual work.
            // 此守卫记录中断，不释放实际工作。
            let mut supervision = supervision;
            manager
                .supervise_formal_call(Arc::clone(&supervisor_owner), supervisor_call, input)
                .await;
            supervision.completed = true;
            // If the waiter was dropped, completion itself reclaims only physically retired internal calls.
            // 等待方已被丢弃时，完成处理自身仅回收物理退役的内部调用。
            if let Err(error) = manager.reap_internal_calls(&supervisor_owner) {
                // A diagnostic-lock failure cannot turn incomplete cleanup into successful retirement.
                // 诊断锁失败不能把未完成清理变成成功退役。
                let _ =
                    manager.record_cleanup_failure(&supervision.call.plugin.id, error.to_string());
            }
        });

        Ok(())
    }
}
