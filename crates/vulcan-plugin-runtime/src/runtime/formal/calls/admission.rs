//! Shared admission and acknowledgement for tools and host lifecycle callbacks.
//! 工具与宿主生命周期回调的共享入场及确认。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Check one frozen call against the current catalog under the caller's publication gate.
    /// 在调用方持有的发布门下，对照当前目录检查单个冻结调用。
    /// Scope is host-selected; returns explicit generation or cancellation failures before submission.
    /// 作用域由宿主选择；在提交前返回明确代次或取消失败。
    pub(super) fn validate_formal_submission(
        &self,
        call: &FormalCall,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Use one catalog snapshot for identity and lifecycle authority.
        // 使用单个目录快照同时检查身份及生命周期权威。
        let snapshot = self.snapshot();
        // The original descriptor cannot follow another generation.
        // 原描述符不能跟随另一个代次。
        let current = snapshot.plugins.get(&call.plugin.id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Plugin disappeared before submission.")
        })?;
        if current.generation != call.plugin.generation
            || current.runtime_state != scope.required_runtime_state()
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
        Ok(())
    }
}

impl VulcanPluginRuntimeManager {
    /// Replace one prepared input reservation under the shared call and pinned-session budget.
    /// 在共享调用及固定会话预算下替换单个已准备输入预留。
    /// The exact call and parent must still be registered; any error keeps its previous reservation.
    /// 精确调用及父级必须仍已登记；任何错误都保留其原预留。
    pub(super) fn reserve_prepared_input(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
        input: &Preparation,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Recount the exact prepared metadata against the parent's original bounds.
        // 根据父级原始边界重新计量精确准备后元数据。
        let bytes = input.input_bytes(&call.ownership, &owner.settings)?;
        // Hold the shared call registry while inspecting or changing retained ownership.
        // 检查或变更保留所有权期间持有共享调用注册表。
        let calls = owner.calls.lock().map_err(|_| calls_error())?;
        // Verify that preparation still belongs to the originally registered identity.
        // 验证准备仍属于最初登记的身份。
        let registered = calls.get(&call.id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal ownership disappeared during preparation.")
        })?;
        if !std::ptr::eq(registered.as_ref(), call) {
            return Err(VulcanPluginRuntimeError::new(
                "Formal preparation identity changed.",
            ));
        }
        // Replace the current record's reservation; do not count it twice or reset another call's budget.
        // 替换当前记录的预留；不重复计数，也不重置其他调用预算。
        let total = calls
            .values()
            .filter(|other| other.id != call.id)
            .map(|other| other.input_bytes.load(Ordering::Acquire))
            .chain(
                owner
                    .pinned
                    .lock()
                    .map_err(|_| calls_error())?
                    .values()
                    .map(|session| session.input_bytes),
            )
            .try_fold(bytes, usize::checked_add);
        if total.is_none_or(|total| total > owner.settings.parent.max_queued_bytes) {
            return Err(VulcanPluginRuntimeError::new(
                "Formal invocation input byte capacity is exhausted after environment preparation.",
            ));
        }
        call.input_bytes.store(bytes, Ordering::Release);
        Ok(())
    }
}

impl Preparation {
    /// Measure the exact retained input against parent value and queue budgets.
    /// 按父级值及队列预算计量精确保留输入。
    /// Ownership and settings are authoritative; returns checked serialized bytes without allocating JSON text.
    /// 归属及设置为权威；返回受检序列化字节数，不分配 JSON 文本。
    fn input_bytes(
        &self,
        ownership: &CallOwner,
        settings: &VulcanPluginRuntimeSettings,
    ) -> Result<usize, VulcanPluginRuntimeError> {
        luaskills::runtime::embedded::json_size(&self.arguments, settings.parent.max_value_bytes)
            .map_err(core_error)?;
        luaskills::runtime::embedded::json_size(
            &(
                &self.arguments,
                &self.finalizer,
                self.admission.export(),
                &self.workspace,
                &self.security_partition,
                self.scope.as_str(),
                ownership,
            ),
            settings.parent.max_queued_bytes,
        )
        .map_err(core_error)
    }
}

impl VulcanPluginRuntimeManager {
    /// Prepare the original lifecycle environment inside an already-owned blocking task.
    /// 在已具有归属的阻塞任务内准备原生命周期环境。
    /// Update the exact input reservation before pool publication; errors retain prior ownership for cleanup.
    /// 发布池之前更新精确输入预留；错误保留原所有权以供清理。
    pub(super) fn prepare_lifecycle_environment(
        &self,
        owner: &ManagedFormalRuntime,
        call: &FormalCall,
        input: &mut Preparation,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Reject stale lifecycle authority before starting potentially blocking filesystem work.
        // 开始可能阻塞的文件系统工作前拒绝陈旧生命周期权威。
        {
            // Serialize this authority check with catalog publication.
            // 将此次权威检查与目录发布排序。
            let publication = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
            // Read one immutable catalog observation for this decision.
            // 为此次决策读取单个不可变目录观测。
            let snapshot = self.snapshot();
            // Use the exact currently published plugin descriptor.
            // 使用当前发布的精确插件描述符。
            let current = snapshot.plugins.get(&call.plugin.id).ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin disappeared before lifecycle preparation.")
            })?;
            if current.generation != call.plugin.generation
                || current.runtime_state != input.scope.required_runtime_state()
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Plugin generation or lifecycle changed before environment preparation.",
                ));
            }
            drop(publication);
        }
        // This uses the same validated copy, dependency install and atomic environment publication as before.
        // 此处使用原有已校验复制、依赖安装及原子环境发布。
        let workspace = crate::runtime::environment::ensure_plugin_execution_environment(
            &self.state,
            &call.plugin,
        )
        .map_err(VulcanPluginRuntimeError::new)?;
        // The host event envelope owns this field; plugin input cannot replace the resolved directory.
        // 宿主事件信封拥有此字段；插件输入不能替换已解析目录。
        let arguments = input.arguments.as_object_mut().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Lifecycle arguments must be a host-owned object.")
        })?;
        arguments.insert("environment_root".into(), json!(&workspace));
        input.workspace = CallWorkspace::Provided(workspace);
        self.reserve_prepared_input(owner, call, input)
    }
}

impl VulcanPluginRuntimeManager {
    /// Run an exact lifecycle callback under its frozen descriptor, workspace and deadline.
    /// 在冻结描述符、工作区及截止时间下运行精确生命周期回调。
    /// The caller supplies the declared export and event envelope; return its version-one result.
    /// 调用方提供声明导出及事件信封；返回其版本一结果。
    pub(in crate::runtime) async fn invoke_formal_lifecycle(
        &self,
        plugin: VulcanPluginDescriptor,
        event: crate::VulcanPluginLifecycleEvent,
        export: String,
        arguments: Value,
        deadline: Instant,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // Parent identity is required; lifecycle execution cannot silently choose a legacy engine.
        // 父级身份为必需；生命周期执行不能静默选择旧引擎。
        let owner = self.required_formal_owner()?;
        // Scope derives exclusively from the trusted event, not a plugin-provided string.
        // 作用域仅由可信事件派生，不读取插件提供的字符串。
        let scope = VulcanPluginBridgeLeaseScope::from(event);
        // A lifecycle callback has fresh state and no tool or Hook business finalizer.
        // 生命周期回调拥有全新状态，没有工具或 Hook 业务关闭器。
        let input = Preparation {
            workspace: CallWorkspace::LifecycleEnvironment,
            arguments,
            finalizer: None,
            admission: CallAdmission::Lifecycle { export },
            scope,
            security_partition: format!("lifecycle:{}:{}", scope.as_str(), plugin.generation),
            deadline,
        };
        self.invoke_owned_formal_call(owner, plugin, input).await
    }

    /// Admit bounded frozen input using the sole runtime and preserve actual work independently of waiting.
    /// 使用唯一运行时接纳有界冻结输入，并独立于等待保留实际工作。
    /// Parameters contain exact parent, descriptor, ownership and preparation; returns the observed result.
    /// 参数包含精确父级、描述符、归属和准备输入；返回已观测结果。
    pub(super) async fn invoke_owned_formal_call(
        &self,
        owner: Arc<ManagedFormalRuntime>,
        plugin: VulcanPluginDescriptor,
        input: Preparation,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        tokio::runtime::Handle::try_current().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Formal invocation requires a Tokio runtime: {error}"
            ))
        })?;
        self.reap_internal_calls(&owner)?;
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
        // The same publication gate orders lifecycle transitions, quota reservation and exact submission.
        // 同一发布门排序生命周期转换、配额预留及精确提交。
        let call = {
            // Serialize this authority check with catalog publication.
            // 将此次权威检查与目录发布排序。
            let publication = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
            // Read one immutable catalog observation for this decision.
            // 为此次决策读取单个不可变目录观测。
            let snapshot = self.snapshot();
            // Use the exact currently published plugin descriptor.
            // 使用当前发布的精确插件描述符。
            let current = snapshot.plugins.get(&plugin.id).ok_or_else(|| {
                VulcanPluginRuntimeError::new("Plugin disappeared before formal admission.")
            })?;
            if current.generation != plugin.generation
                || current.runtime_state != input.scope.required_runtime_state()
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Plugin generation or lifecycle changed before formal admission.",
                ));
            }
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
            // Include retained calls and pinned sessions in the same checked byte sum.
            // 在同一受检字节总和中包含保留调用及固定会话。
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
                ownership,
                input_bytes: AtomicUsize::new(input_bytes),
                cancelled: AtomicBool::new(false),
                acknowledged: AtomicBool::new(false),
                state: Mutex::new(CallState {
                    action_work,
                    supervised: true,
                    ..CallState::default()
                }),
            });
            calls.insert(call.id.clone(), Arc::clone(&call));
            call
        };
        // Both guards exist before spawning so discarding an unpolled task preserves recovery evidence.
        // 两个守卫在派发前存在，丢弃未轮询任务时仍保留恢复证据。
        let mut observation = ObservationGuard {
            call: Arc::clone(&call),
            completed: false,
        };
        // This guard records interruption without releasing actual work.
        // 此守卫记录中断，不释放实际工作。
        let supervision = SupervisionGuard {
            call: Arc::clone(&call),
            completed: false,
        };
        // Retain the original manager identity for the operation being observed.
        // 为被观测操作保留原管理器身份。
        let manager = self.clone();
        // The supervisor retains the same formal parent throughout cleanup.
        // 监督者在整个清理期间保留同一正式父级。
        let supervisor_owner = Arc::clone(&owner);
        // The supervisor observes the original invocation rather than recreating it.
        // 监督者观测原调用，不重新创建调用。
        let supervisor_call = Arc::clone(&call);
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
        loop {
            // Clone completed evidence under the short lock before acknowledgement.
            // 确认之前在短锁下克隆完成证据。
            let observed = call.lock()?.result.clone();
            if let Some(result) = observed {
                observation.completed = true;
                call.acknowledged.store(true, Ordering::Release);
                match &call.ownership {
                    CallOwner::Controller(id) if result.is_ok() => {
                        self.forget_formal_call(&call.id, id)?;
                    }
                    CallOwner::Lifecycle | CallOwner::Action | CallOwner::Probe => {
                        self.reap_internal_calls(&owner)?;
                    }
                    CallOwner::Controller(_) => {}
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

    /// Reclaim acknowledged internal-call evidence only after physical retirement and completed supervision.
    /// 仅在物理退役及监督完成后回收已确认的内部调用证据。
    /// The exact parent supplies operation identities; any failure retains the current record for recovery.
    /// 精确父级提供操作身份；任何失败均保留当前记录以供恢复。
    pub(super) fn reap_internal_calls(
        &self,
        owner: &ManagedFormalRuntime,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Hold the shared call registry while inspecting or changing retained ownership.
        // 检查或变更保留所有权期间持有共享调用注册表。
        let mut calls = owner.calls.lock().map_err(|_| calls_error())?;
        // Remove each successful checkpoint before the next fallible acknowledgement.
        // 下次可能失败的确认前先移除每个已成功检查点。
        let ids: Vec<_> = calls.keys().cloned().collect();
        for id in ids {
            // Keep the exact invocation record independently of its observer.
            // 独立于观测者保留精确调用记录。
            let call = calls
                .get(&id)
                .expect("ID was collected under the same registry lock");
            if matches!(call.ownership, CallOwner::Controller(_))
                || !call.acknowledged.load(Ordering::Acquire)
            {
                continue;
            }
            // Inspect the original physical-retirement and supervision checkpoints.
            // 检查原物理退役及监督检查点。
            let state = call.lock()?;
            if !state.retired || state.supervised || state.preparing || state.result.is_none() {
                continue;
            }
            if let Some(operation) = &state.operation {
                owner
                    .host
                    .runtime()
                    .forget_operation(operation.id())
                    .map_err(core_error)?;
            }
            drop(state);
            calls.remove(&id);
        }
        Ok(())
    }
}
