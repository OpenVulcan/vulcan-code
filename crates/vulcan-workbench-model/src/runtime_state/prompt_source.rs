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
    /// Host-created hidden continuation that resumes the parent after one durable subagent return.
    /// 宿主创建的隐藏续跑输入，用于在子代理结果可靠回流后恢复父会话。
    SubagentReturnContinuation,
    /// Host-created attempt that restores a claimed execution checkpoint.
    /// 宿主创建的已领取执行 checkpoint 恢复尝试。
    ExecutionRetry,
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

    /// Returns the source semantics to use when a queued prompt starts from an idle session.
    /// 返回排队提示词从空闲会话启动时应使用的来源语义。
    ///
    /// # Returns
    /// 返回值
    /// A missed interrupt becomes a regular follow-up because no active turn remains to fold into.
    /// 错过活动回合的插队消息会转为常规跟进，因为此时已无可折叠的活动回合。
    pub fn for_idle_dispatch(self) -> Self {
        match self {
            Self::UserInterrupt => Self::UserFollowup,
            source => source,
        }
    }

    /// Returns whether this source allows destructive user-prompt editing.
    /// 返回该来源是否允许破坏性用户提示词编辑。
    pub fn allows_user_prompt_edit(self) -> bool {
        self.starts_user_parent_turn()
    }
}

/// Runtime facts required by the shared pending-prompt dispatch gate.
/// 共享待发送提示词分发门禁所需的运行态事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingPromptDispatchContext {
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

/// Returns whether the V1 admission lane is still busy using service semantics.
/// 使用服务语义判断 V1 准入通道是否仍然繁忙。
///
/// # Parameters
/// 参数
/// `has_queued_inputs` reports whether the exact session still owns durable V1 queued inputs; it is
/// the sole queue occupancy authority and is always read from the V1 session document, never from a
/// process-local compatibility mirror.
/// `has_queued_inputs` 表示当前会话是否仍持有可靠的 V1 排队输入；它是唯一的队列占用权威，
/// 必须始终读取自 V1 会话文档，而不是进程内兼容镜像。
/// `context` contains UI-specific runtime facts collected by the caller.
/// `context` 包含调用方收集的界面专属运行态事实。
///
/// # Returns
/// 返回值
/// `true` when a submitted prompt must enter the visible pending queue.
/// 当提交的提示词必须进入可见待发送队列时返回 `true`。
pub fn pending_prompt_lane_is_busy(
    has_queued_inputs: bool,
    context: PendingPromptBusyLaneContext,
) -> bool {
    context.dispatch.has_active_run
        || provider_session_run_phase_is_active(context.dispatch.run_phase)
        || (context.dispatch.run_phase == Some(crate::SessionRunPhase::Preparing)
            && context.has_precheck_progress)
        || context.dispatch.run_phase == Some(crate::SessionRunPhase::MemoryRecall)
        || context.dispatch.has_active_compaction
        || has_queued_inputs
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
