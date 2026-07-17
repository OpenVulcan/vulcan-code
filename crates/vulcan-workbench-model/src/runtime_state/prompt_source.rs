use super::*;

/// Stable source kind attached to one pending session prompt queue entry.
/// 附着在单条待发送会话提示词队列项上的稳定来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingSessionPromptSource {
    /// User-authored follow-up that should wait for the current run to finish naturally.
    /// 应等待当前运行自然结束后再发送的用户跟进消息。
    UserFollowup,
    /// User-authored follow-up that should interrupt the current run before being dispatched.
    /// 应先打断当前运行再发送的用户跟进消息。
    UserInterrupt,
    /// Host-created session mailbox input that returns completed runtime work to the parent model.
    /// 宿主创建的会话 mailbox 输入，用于将已完成运行时工作回流给父级模型。
    SessionMailbox,
    /// Host-created hidden continuation that advances an active session goal while the thread is idle.
    /// 宿主创建的隐藏续跑输入，用于在线程空闲时推进活跃会话目标。
    GoalContinuation,
}

impl PendingSessionPromptSource {
    /// Returns whether this prompt source begins a new user-authored parent turn.
    /// 返回该提示词来源是否会开启新的用户编写父会话轮次。
    ///
    /// # Returns
    /// 返回值
    /// `true` for user prompts that should reset parent-round UI affordances.
    /// 对应应重置父级轮次界面的用户提示词时返回 `true`。
    pub fn starts_user_parent_turn(self) -> bool {
        matches!(self, Self::UserFollowup)
    }

    /// Returns whether this source should be persisted as canonical user-authored conversation text.
    /// 返回该来源是否应作为用户编写的规范会话文本持久化。
    pub fn persists_canonical_user_message(self) -> bool {
        matches!(self, Self::UserFollowup | Self::UserInterrupt)
    }

    /// Returns whether this source should append a visible user transcript record.
    /// 返回该来源是否应追加可见用户转录记录。
    pub fn appends_visible_user_prompt_record(self) -> bool {
        matches!(self, Self::UserFollowup | Self::UserInterrupt)
    }

    /// Returns whether this source should run user-prompt precheck hooks.
    /// 返回该来源是否应该运行用户提示词 precheck hook。
    pub fn runs_user_prompt_precheck(self) -> bool {
        matches!(self, Self::UserFollowup)
    }

    /// Returns whether this source should close the logical user-authored turn.
    /// 返回该来源是否应该闭合用户编写的逻辑回合。
    pub fn runs_completed_turn_hooks(self) -> bool {
        self.starts_user_parent_turn()
    }

    /// Returns whether this source is process input folded into the active user turn.
    /// 返回该来源是否属于折叠进当前用户轮次过程的输入。
    pub fn folds_into_active_turn_process(self) -> bool {
        matches!(self, Self::UserInterrupt)
    }

    /// Returns whether this source allows destructive user-prompt editing.
    /// 返回该来源是否允许破坏性用户提示词编辑。
    pub fn allows_user_prompt_edit(self) -> bool {
        self.starts_user_parent_turn()
    }
}

/// Returns the internal queue index that should dispatch next.
/// 返回下一条应被分发的内部队列索引。
pub fn pending_prompt_dispatch_index(
    queue: &[PendingSessionPromptEntry],
    _priority_override: bool,
) -> usize {
    let _ = queue;
    0
}

/// Runtime facts required by the shared pending-prompt dispatch gate.
/// 共享待发送提示词分发门禁所需的运行态事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingPromptDispatchContext {
    /// Whether the current provider uses the Anthropic runtime family.
    /// 当前 Provider 是否使用 Anthropic 运行时族。
    pub uses_anthropic_runtime: bool,
    /// Whether delegated subagents are still running under the same parent session.
    /// 同一父会话下是否仍有委派子代理正在运行。
    pub has_running_subagents: bool,
    /// Whether the parent session still owns an active root model stream.
    /// 父会话是否仍拥有活跃的根模型流。
    pub has_active_run: bool,
    /// Current visible session run phase.
    /// 当前可见会话运行阶段。
    pub run_phase: Option<crate::SessionRunPhase>,
    /// Whether a prompt compaction job is still active for this session.
    /// 当前会话是否仍有活跃的提示词压缩任务。
    pub has_active_compaction: bool,
}

/// Reason why a queued prompt cannot be dispatched yet.
/// 排队提示词暂时不能分发的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPromptDispatchBlockReason {
    /// Parent dispatch must wait for running delegated subagents.
    /// 父会话分发必须等待运行中的委派子代理。
    RunningSubagents,
    /// A root model stream is still active.
    /// 根模型流仍在运行。
    ActiveRun,
    /// The session is still in preparation.
    /// 会话仍处于准备阶段。
    Preparing,
    /// A prompt compaction job is still active.
    /// 提示词压缩任务仍在运行。
    ActiveCompaction,
    /// Automatic dispatch is blocked by a manually stopped phase.
    /// 自动分发被手动停止阶段阻塞。
    Stopped,
}

/// Returns the shared service-compatible reason that blocks queued prompt dispatch.
/// 返回与服务兼容的共享排队提示词分发阻塞原因。
///
/// # Parameters
/// 参数
/// `context` contains UI-specific runtime facts collected by the caller.
/// `context` 包含调用方收集的界面专属运行态事实。
/// `allow_from_stopped_phase` mirrors service manual insert behavior.
/// `allow_from_stopped_phase` 对齐服务手动引导行为。
///
/// # Returns
/// 返回值
/// A block reason when dispatch must wait, otherwise `None`.
/// 必须等待时返回阻塞原因，否则返回 `None`。
pub fn pending_prompt_dispatch_block_reason(
    context: PendingPromptDispatchContext,
    allow_from_stopped_phase: bool,
) -> Option<PendingPromptDispatchBlockReason> {
    pending_prompt_dispatch_block_reason_for_source(context, allow_from_stopped_phase, None)
}

/// Returns the shared dispatch block reason for the next prompt source.
/// 按下一条提示词来源返回共享分发阻塞原因。
///
/// # Parameters
/// 参数
/// `context` contains UI-specific runtime facts collected by the caller.
/// `context` 包含调用方收集的界面专属运行态事实。
/// `allow_from_stopped_phase` mirrors service manual insert behavior.
/// `allow_from_stopped_phase` 对齐服务手动引导行为。
/// `next_source` identifies the next prompt that would be dispatched, when known.
/// `next_source` 标识即将分发的下一条提示词来源，未知时传入空值。
///
/// # Returns
/// 返回值
/// A block reason when dispatch must wait, otherwise `None`.
/// 必须等待时返回阻塞原因，否则返回 `None`。
pub fn pending_prompt_dispatch_block_reason_for_source(
    context: PendingPromptDispatchContext,
    allow_from_stopped_phase: bool,
    _next_source: Option<PendingSessionPromptSource>,
) -> Option<PendingPromptDispatchBlockReason> {
    if context.has_active_run || provider_session_run_phase_is_active(context.run_phase) {
        return Some(PendingPromptDispatchBlockReason::ActiveRun);
    }
    if matches!(
        context.run_phase,
        Some(crate::SessionRunPhase::Preparing | crate::SessionRunPhase::MemoryRecall)
    ) {
        return Some(PendingPromptDispatchBlockReason::Preparing);
    }
    if context.has_active_compaction {
        return Some(PendingPromptDispatchBlockReason::ActiveCompaction);
    }
    if !allow_from_stopped_phase && context.run_phase == Some(crate::SessionRunPhase::Stopped) {
        return Some(PendingPromptDispatchBlockReason::Stopped);
    }
    None
}

/// Returns the source of the next prompt that would be dispatched from the session queues.
/// 返回会话队列中下一条将被分发的提示词来源。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped queue state.
/// `ui_state` 是会话级队列状态。
/// `_priority_override` is ignored because queued prompts use FIFO dispatch.
/// `_priority_override` 会被忽略，因为排队提示词使用 FIFO 分发。
///
/// # Returns
/// 返回值
/// The next prompt source when any dispatchable internal or visible prompt exists.
/// 存在可分发的内部或可见提示词时返回下一条提示词来源。
pub fn next_pending_prompt_dispatch_source(
    ui_state: &SessionTranscriptUiState,
    _priority_override: bool,
) -> Option<PendingSessionPromptSource> {
    if !ui_state.pending_prompt_queue.is_empty() {
        return Some(ui_state.pending_prompt_queue[0].source);
    }
    ui_state
        .pending_prompt_panel_queue
        .iter()
        .any(|entry| entry.state == PendingPromptPanelEntryState::Ready)
        .then_some(PendingSessionPromptSource::UserFollowup)
}

/// Runtime facts required by the shared pending-prompt busy-lane check.
/// 共享待发送提示词繁忙通道判断所需的运行态事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingPromptBusyLaneContext {
    /// Dispatch context shared with the queued prompt router.
    /// 与排队提示词路由器共享的分发上下文。
    pub dispatch: PendingPromptDispatchContext,
    /// Whether the session still owns an active precheck progress row.
    /// 当前会话是否仍拥有活跃的预检查进度行。
    pub has_precheck_progress: bool,
}

/// Returns whether the internal system lane is still busy using service semantics.
/// 使用服务语义判断内部系统通道是否仍然繁忙。
///
/// # Parameters
/// 参数
/// `ui_state` is the session-scoped queue state.
/// `ui_state` 是会话级队列状态。
/// `context` contains UI-specific runtime facts collected by the caller.
/// `context` 包含调用方收集的界面专属运行态事实。
///
/// # Returns
/// 返回值
/// `true` when a submitted prompt must enter the visible pending queue.
/// 当提交的提示词必须进入可见待发送队列时返回 `true`。
pub fn pending_prompt_lane_is_busy(
    ui_state: &SessionTranscriptUiState,
    context: PendingPromptBusyLaneContext,
) -> bool {
    context.dispatch.has_active_run
        || provider_session_run_phase_is_active(context.dispatch.run_phase)
        || (context.dispatch.run_phase == Some(crate::SessionRunPhase::Preparing)
            && context.has_precheck_progress)
        || context.dispatch.run_phase == Some(crate::SessionRunPhase::MemoryRecall)
        || context.dispatch.has_active_compaction
        || !ui_state.pending_prompt_queue.is_empty()
        || !ui_state.pending_prompt_panel_queue.is_empty()
}

/// 参数
/// `phase` is the optional session run phase captured by one frontend or service bridge.
/// `phase` 是某个前端或服务桥捕获到的可选会话运行阶段。
///
/// # Returns
/// 返回值
/// `true` when the phase should block dispatch as active execution.
/// 当该阶段应作为活跃执行阻塞分发时返回 `true`。
fn provider_session_run_phase_is_active(phase: Option<crate::SessionRunPhase>) -> bool {
    matches!(
        phase,
        Some(
            crate::SessionRunPhase::Requesting
                | crate::SessionRunPhase::Thinking
                | crate::SessionRunPhase::ToolCalling
                | crate::SessionRunPhase::ToolRunning
        )
    )
}
