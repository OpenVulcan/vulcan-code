use super::*;

/// Prepared asynchronous precheck request that can run outside the UI message handler.
/// 可在 UI 消息处理器之外运行的已准备异步 precheck 请求。
#[derive(Clone)]
pub struct ConversationPrecheckRequest {
    /// Dispatcher snapshot used for this precheck run.
    /// 当前 precheck 运行使用的调度器快照。
    dispatcher: HookDispatcher,
    /// Session that owns the precheck request.
    /// 拥有该 precheck 请求的会话。
    session_id: SessionId,
    /// JSON payload sent to the hook point.
    /// 发送给 hook 点位的 JSON 载荷。
    payload: Value,
    /// Logical turn id associated with this precheck request.
    /// 与该 precheck 请求关联的逻辑回合标识。
    turn_id: String,
}

impl ConversationPrecheckRequest {
    /// Builds one prepared asynchronous precheck request.
    /// 构建一条已准备的异步 precheck 请求。
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

    /// Returns the logical turn id associated with this precheck request.
    /// 返回与该 precheck 请求关联的逻辑回合标识。
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    /// Runs the precheck hook dispatch on a worker thread and returns a non-panicking report result.
    /// 在工作线程执行 precheck hook 调度，并返回不会 panic 的报告结果。
    pub async fn run(self) -> ConversationPrecheckOutcome {
        self.run_on_worker_thread().await
    }

    /// Completes this prepared precheck without dispatching handlers.
    /// 在不调度处理器的情况下完成这条已准备的 precheck。
    pub fn skip(self) -> ConversationPrecheckOutcome {
        ConversationPrecheckOutcome {
            session_id: self.session_id,
            turn_id: self.turn_id,
            report: Ok(HookDispatchReport::empty(HookPayload {
                value: self.payload,
            })),
        }
    }

    /// Runs the async hook dispatcher inside a private Tokio runtime so the UI executor never blocks.
    /// 在私有 Tokio 运行时中执行异步 hook 调度，避免阻塞 UI 执行器。
    async fn run_on_worker_thread(self) -> ConversationPrecheckOutcome {
        let session_id = self.session_id;
        let turn_id = self.turn_id.clone();
        let context = HookContext {
            run_id: format!("{}:{}", HOOK_CONVERSATION_TURN_PRECHECK, session_id),
            point: HOOK_CONVERSATION_TURN_PRECHECK.to_string(),
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
        let request = HookDispatchRequest::new(
            context,
            HookPayload {
                value: self.payload,
            },
        );
        let report = run_hook_dispatch_on_worker_thread(
            "precheck hook",
            format!("vulcan-precheck-{session_id}"),
            self.dispatcher,
            request,
        )
        .await;
        ConversationPrecheckOutcome {
            session_id,
            turn_id,
            report,
        }
    }
}

/// Result produced by one asynchronous conversation precheck dispatch.
/// 单次异步会话 precheck 调度产生的结果。
#[derive(Debug, Clone)]
pub struct ConversationPrecheckOutcome {
    /// Session that owns the completed precheck.
    /// 拥有已完成 precheck 的会话。
    pub session_id: SessionId,
    /// Hook dispatch report or a non-fatal error message.
    /// hook 调度报告或非致命错误消息。
    pub report: Result<HookDispatchReport, String>,
    /// Logical turn id associated with this precheck.
    /// 与该 precheck 关联的逻辑回合标识。
    pub turn_id: String,
}
