use super::*;

/// Prepared completed-turn hook request that can run after the next queued prompt starts.
/// 可在下一条排队提示词启动后执行的已准备完成回合 hook 请求。
#[derive(Clone)]
pub struct ConversationCompletionRequest {
    /// Dispatcher snapshot used for this completed-turn hook.
    /// 当前完成回合 hook 使用的调度器快照。
    dispatcher: HookDispatcher,
    /// Session that owns the completed turn.
    /// 拥有已完成回合的会话。
    session_id: SessionId,
    /// JSON payload sent to the completed-turn hook point.
    /// 发送给完成回合 hook 点位的 JSON 载荷。
    pub(super) payload: Value,
    /// Logical turn id associated with this completed turn.
    /// 与该完成回合关联的逻辑回合标识。
    turn_id: String,
}

impl ConversationCompletionRequest {
    /// Builds one prepared completed-turn hook request.
    /// 构建一条已准备的完成回合 hook 请求。
    pub(super) fn new(
        dispatcher: HookDispatcher,
        session_id: SessionId,
        payload: Value,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            dispatcher,
            session_id,
            payload,
            turn_id: turn_id.into(),
        }
    }

    /// Restricts VMM hook execution using the latest host-owned service readiness.
    /// 使用宿主最新持有的服务就绪事实限制 VMM Hook 执行。
    ///
    /// # Parameters
    /// 参数
    /// `vmm_runtime_ready` is true only when service configuration and runtime remain ready.
    /// `vmm_runtime_ready` 仅在服务配置与运行态仍保持就绪时为 true。
    ///
    /// # Returns
    /// 返回值
    /// This method returns no value and can only revoke, never grant, admission-time readiness.
    /// 本方法不返回值，并且只能撤销、绝不能授予准入时的就绪事实。
    pub fn restrict_vmm_runtime_readiness(&mut self, vmm_runtime_ready: bool) {
        let admitted_ready = self
            .payload
            .get("vmm_runtime_ready")
            .and_then(Value::as_bool)
            == Some(true);
        if let Some(payload) = self.payload.as_object_mut() {
            payload.insert(
                String::from("vmm_runtime_ready"),
                Value::Bool(admitted_ready && vmm_runtime_ready),
            );
        }
    }

    /// Returns the session that owns the completed turn.
    /// 返回拥有该完成回合的会话。
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Returns the logical turn id associated with this completed-turn request.
    /// 返回与这条完成回合请求关联的逻辑回合标识。
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    /// Runs the completed-turn hook dispatch on a worker thread.
    /// 在工作线程执行完成回合 hook 调度。
    pub async fn run(self) -> ConversationCompletionOutcome {
        self.run_on_worker_thread().await
    }

    /// Runs the completed-turn hook dispatch in one private worker thread.
    /// 在单个私有工作线程中执行完成回合 hook 调度。
    ///
    /// # Returns
    /// # 返回值
    /// Returns the completed dispatch report, or a stage-specific non-fatal diagnostic.
    /// 返回完成的调度报告，或带有明确阶段信息的非致命诊断。
    async fn run_on_worker_thread(self) -> ConversationCompletionOutcome {
        let session_id = self.session_id;
        let turn_id = self.turn_id.clone();
        // context preserves the exact completed-turn correlation before ownership moves to the worker.
        // context 在所有权移交工作线程前保留精确的完成回合关联信息。
        let context = HookContext {
            run_id: format!("{}:{}", HOOK_CONVERSATION_TURN_COMPLETED, session_id),
            point: HOOK_CONVERSATION_TURN_COMPLETED.to_string(),
            scope: HookScope::Session,
            actor: HookActor::new("vulcan-code-runtime"),
            workspace_id: None,
            project_id: None,
            session_id: Some(session_id.to_string()),
            conversation_id: Some(session_id.to_string()),
            user_id: None,
            capabilities: Default::default(),
            metadata: BTreeMap::new(),
        };
        // request is the immutable dispatch input consumed by the worker-owned runtime.
        // request 是由工作线程私有运行时消费的不可变调度输入。
        let request = HookDispatchRequest::new(
            context,
            HookPayload {
                value: self.payload,
            },
        );
        let (sender, receiver) = oneshot::channel();
        let spawn_result = std::thread::Builder::new()
            .name(format!("vulcan-completion-{session_id}"))
            .spawn(move || {
                let result = run_completion_dispatch_on_private_runtime(self.dispatcher, request);
                let _ = sender.send(result);
            });
        if let Err(error) = spawn_result {
            return ConversationCompletionOutcome {
                session_id,
                turn_id,
                report: Err(format!(
                    "completed-turn hook thread failed to start: {error}"
                )),
                vmm_writeback_effects: Vec::new(),
            };
        }
        let report = receiver.await.unwrap_or_else(|_| {
            Err(String::from(
                "completed-turn hook dispatch thread disconnected",
            ))
        });
        let vmm_writeback_effects = report
            .as_ref()
            .map(vmm_writeback_effects_from_report)
            .unwrap_or_default();
        ConversationCompletionOutcome {
            session_id,
            turn_id,
            report,
            vmm_writeback_effects,
        }
    }
}

/// Runs one completed-turn dispatch inside a private Tokio runtime on the existing worker thread.
/// 在既有工作线程内使用私有 Tokio 运行时执行一次完成回合调度。
///
/// # Parameters
/// # 参数
/// - `dispatcher`: Dispatcher snapshot that owns the registered completed-turn handlers.
/// - `dispatcher`：拥有已注册完成回合处理器的调度器快照。
/// - `request`: Fully correlated completed-turn dispatch request.
/// - `request`：包含完整关联信息的完成回合调度请求。
///
/// # Returns
/// # 返回值
/// Returns the hook report on success, or a diagnostic that identifies runtime creation,
/// timeout, dispatcher, or panic failure stages.
/// 成功时返回 Hook 报告；失败时返回可区分运行时创建、超时、调度器或 panic 阶段的诊断。
fn run_completion_dispatch_on_private_runtime(
    dispatcher: HookDispatcher,
    request: HookDispatchRequest,
) -> Result<HookDispatchReport, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("completed-turn hook runtime failed: {error}"))?;
        runtime.block_on(async move {
            tokio::time::timeout(
                Duration::from_millis(COMPLETION_DISPATCH_TIMEOUT_MS),
                dispatcher.dispatch(request),
            )
            .await
            .map_err(|_| String::from("completed-turn hook dispatch timed out"))?
            .map_err(|error| format!("completed-turn hook dispatch failed: {error}"))
        })
    }))
    .unwrap_or_else(|_| Err(String::from("completed-turn hook dispatch thread panicked")))
}

/// Result produced by one asynchronous completed-turn hook dispatch.
/// 单次异步完成回合 hook 调度产生的结果。
#[derive(Debug, Clone)]
pub struct ConversationCompletionOutcome {
    /// Session that owns the completed turn.
    /// 拥有已完成回合的会话。
    pub session_id: SessionId,
    /// Logical turn id associated with the completed turn.
    /// 与已完成回合关联的逻辑回合标识。
    pub turn_id: String,
    /// Hook dispatch report or a non-fatal error message.
    /// hook 调度报告或非致命错误消息。
    pub report: Result<HookDispatchReport, String>,
    /// VMM writeback effects extracted from the completed-turn hook report.
    /// 从完成回合 hook 报告中提取的 VMM 写回效果。
    pub vmm_writeback_effects: Vec<VmmWritebackHookEffect>,
}

/// Extracts VMM writeback effects from one hook dispatch report.
/// 从一次 hook 调度报告中提取 VMM 写回效果。
pub(super) fn vmm_writeback_effects_from_report(
    report: &HookDispatchReport,
) -> Vec<VmmWritebackHookEffect> {
    report
        .metadata
        .get(HOOK_METADATA_VMM_WRITEBACK_EFFECTS)
        .into_iter()
        .flat_map(|values| values.iter())
        .flat_map(|value| {
            if let Some(items) = value.as_array() {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect::<Vec<_>>()
            } else {
                serde_json::from_value(value.clone())
                    .ok()
                    .into_iter()
                    .collect()
            }
        })
        .collect()
}
