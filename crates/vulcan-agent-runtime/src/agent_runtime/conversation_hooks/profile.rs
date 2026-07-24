use super::*;

/// Prepared asynchronous profile request dispatched independently from user-prompt precheck.
/// 独立于用户提示 precheck 派发的已准备异步画像请求。
#[derive(Clone)]
pub struct ConversationProfileRequest {
    /// Dispatcher snapshot used for this profile preparation.
    /// 当前画像准备使用的调度器快照。
    dispatcher: HookDispatcher,
    /// Session that owns the profile request.
    /// 拥有该画像请求的会话。
    session_id: SessionId,
    /// JSON payload shared with the owning prompt turn.
    /// 与所属提示词回合共享的 JSON 载荷。
    payload: Value,
    /// Logical turn id associated with this profile request.
    /// 与该画像请求关联的逻辑回合标识。
    turn_id: String,
}

impl ConversationProfileRequest {
    /// Builds one prepared asynchronous profile request.
    /// 构建一条已准备的异步画像请求。
    ///
    /// # Parameters
    /// 参数
    /// `dispatcher` is the immutable Hook dispatcher snapshot for this request.
    /// `dispatcher` 是该请求使用的不可变 Hook 调度器快照。
    /// `session_id` identifies the owning session.
    /// `session_id` 标识所属会话。
    /// `payload` carries the prompt-turn context consumed by profile handlers.
    /// `payload` 携带画像处理器消费的提示词回合上下文。
    /// `turn_id` correlates the outcome with the exact prompt turn.
    /// `turn_id` 将结果与精确提示词回合关联。
    ///
    /// # Returns
    /// 返回值
    /// A prepared request that has not dispatched handlers yet.
    /// 返回尚未派发处理器的已准备请求。
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

    /// Returns the logical turn id associated with this profile request.
    /// 返回与该画像请求关联的逻辑回合标识。
    ///
    /// # Returns
    /// 返回值
    /// The immutable prompt-turn correlation id.
    /// 返回不可变的提示词回合关联标识。
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    /// Runs profile preparation on a worker thread and returns a non-panicking outcome.
    /// 在工作线程运行画像准备并返回不会 panic 的结果。
    ///
    /// # Returns
    /// 返回值
    /// The completed profile outcome, including continued-handler failures.
    /// 返回已完成的画像结果，其中包含允许继续的处理器失败。
    pub async fn run(self) -> ConversationProfileOutcome {
        self.run_on_worker_thread().await
    }

    /// Builds an empty profile outcome for crate-local regression tests without dispatching handlers.
    /// 为 crate 内回归测试构建不派发处理器的空画像结果。
    ///
    /// # Returns
    /// 返回值
    /// An empty successful test outcome retaining the original turn correlation.
    /// 返回保留原始回合关联的空成功测试结果。
    #[cfg(test)]
    pub(crate) fn skip(self) -> ConversationProfileOutcome {
        ConversationProfileOutcome {
            session_id: self.session_id,
            turn_id: self.turn_id,
            report: Ok(HookDispatchReport::empty(HookPayload {
                value: self.payload,
            })),
        }
    }

    /// Runs the profile dispatcher inside a private Tokio runtime.
    /// 在私有 Tokio 运行时内运行画像调度器。
    ///
    /// # Returns
    /// 返回值
    /// The correlated outcome after dispatch completion or explicit infrastructure failure.
    /// 返回派发完成或基础设施显式失败后的关联结果。
    async fn run_on_worker_thread(self) -> ConversationProfileOutcome {
        // session_id and turn_id remain available after the request moves into the worker thread.
        // session_id 与 turn_id 在请求移入工作线程后仍保持可用。
        let session_id = self.session_id;
        let turn_id = self.turn_id.clone();
        // context identifies the independent profile hook point instead of the user precheck point.
        // context 标识独立画像 Hook 点位，而不是用户 precheck 点位。
        let context = HookContext {
            run_id: format!("{}:{}", HOOK_CONVERSATION_PROFILE_PREPARE, session_id),
            point: HOOK_CONVERSATION_PROFILE_PREPARE.to_string(),
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
        // request carries the immutable prompt-turn payload into the dedicated profile dispatcher.
        // request 将不可变提示词回合载荷传入专用画像调度器。
        let request = HookDispatchRequest::new(
            context,
            HookPayload {
                value: self.payload,
            },
        );
        // sender and receiver return the worker result without blocking the caller's async executor.
        // sender 与 receiver 在不阻塞调用方异步执行器的情况下返回工作线程结果。
        let (sender, receiver) = oneshot::channel();
        // spawn_result retains thread-creation failures as explicit profile diagnostics.
        // spawn_result 将线程创建失败保留为显式画像诊断。
        let spawn_result = std::thread::Builder::new()
            .name(format!("vulcan-profile-{session_id}"))
            .spawn(move || {
                let result = run_profile_dispatch_on_private_runtime(self.dispatcher, request);
                let _ = sender.send(result);
            });
        if let Err(error) = spawn_result {
            return ConversationProfileOutcome {
                session_id,
                turn_id,
                report: Err(format!(
                    "profile preparation hook thread failed to start: {error}"
                )),
            };
        }
        // report converts worker disconnection into the same explicit non-panicking outcome.
        // report 将工作线程断连转换为同一种显式且不会 panic 的结果。
        let report = receiver.await.unwrap_or_else(|_| {
            Err(String::from(
                "profile preparation hook dispatch thread disconnected",
            ))
        });
        ConversationProfileOutcome {
            session_id,
            turn_id,
            report,
        }
    }
}

/// Runs one profile dispatch inside a private current-thread Tokio runtime.
/// 在私有单线程 Tokio 运行时中执行一次画像派发。
///
/// # Parameters
/// 参数
/// `dispatcher` owns the registered profile handlers for this immutable dispatch.
/// `dispatcher` 持有本次不可变派发使用的已注册画像处理器。
/// `request` carries the exact profile hook context and payload.
/// `request` 携带精确的画像 Hook 上下文与载荷。
///
/// # Returns
/// 返回值
/// The Hook report or an explicit timeout, runtime, or panic diagnostic.
/// 返回 Hook 报告，或显式的超时、运行时及 panic 诊断。
fn run_profile_dispatch_on_private_runtime(
    dispatcher: HookDispatcher,
    request: HookDispatchRequest,
) -> Result<HookDispatchReport, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        // runtime isolates blocking profile networking from the UI executor.
        // runtime 将可能阻塞的画像网络访问与 UI 执行器隔离。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("profile preparation hook runtime failed: {error}"))?;
        runtime.block_on(async move {
            tokio::time::timeout(
                Duration::from_millis(PROFILE_DISPATCH_TIMEOUT_MS),
                dispatcher.dispatch(request),
            )
            .await
            .map_err(|_| String::from("profile preparation hook dispatch timed out"))?
            .map_err(|error| error.to_string())
        })
    }))
    .unwrap_or_else(|_| {
        Err(String::from(
            "profile preparation hook dispatch thread panicked",
        ))
    })
}

/// Result produced by one independent profile preparation dispatch.
/// 单次独立画像准备派发产生的结果。
#[derive(Debug, Clone)]
pub struct ConversationProfileOutcome {
    /// Session that owns the completed profile preparation.
    /// 拥有已完成画像准备的会话。
    pub session_id: SessionId,
    /// Hook dispatch report or a non-fatal infrastructure diagnostic.
    /// Hook 派发报告或非致命基础设施诊断。
    pub report: Result<HookDispatchReport, String>,
    /// Logical turn id associated with this profile preparation.
    /// 与该画像准备关联的逻辑回合标识。
    pub turn_id: String,
}
