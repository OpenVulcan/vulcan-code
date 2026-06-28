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

    /// Runs the completed-turn hook dispatch synchronously for legacy callers.
    /// 为旧调用方同步执行完成回合 hook 调度。
    pub(super) fn run_blocking(self) -> Option<HookDispatchReport> {
        ConversationHookRuntime::dispatch_completion_blocking(
            self.dispatcher,
            self.session_id,
            self.payload,
        )
    }

    /// Runs the completed-turn hook dispatch in a private worker thread.
    /// 在私有工作线程中执行完成回合 hook 调度。
    async fn run_on_worker_thread(self) -> ConversationCompletionOutcome {
        let session_id = self.session_id;
        let turn_id = self.turn_id.clone();
        let (sender, receiver) = oneshot::channel();
        let spawn_result = std::thread::Builder::new()
            .name(format!("vulcan-completion-{session_id}"))
            .spawn(move || {
                let result = self.run_blocking();
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
        let report = receiver
            .await
            .map_err(|_| String::from("completed-turn hook dispatch thread disconnected"))
            .and_then(|report| {
                report
                    .ok_or_else(|| String::from("completed-turn hook dispatch produced no report"))
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
