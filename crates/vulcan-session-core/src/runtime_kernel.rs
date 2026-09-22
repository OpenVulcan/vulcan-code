use crate::model::SessionNodeId;
use crate::turn_input_queue::{MailboxDeliveryPhase, TurnInput, TurnInputQueue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Runtime kind for an active session turn.
/// 活动会话回合的运行类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveTurnKind {
    /// User prompted provider turn.
    /// 用户提示词触发的 Provider 回合。
    UserPrompt,
    /// Mailbox-triggered provider turn.
    /// mailbox 触发的 Provider 回合。
    MailboxTriggered,
    /// Dedicated temporary question execution that must not complete the business task.
    /// 临时问题专用执行，不得结束业务任务。
    QuestionExecution {
        /// Durable execution identity owned by this exact kernel generation.
        /// 当前精确内核代际拥有的可靠执行身份。
        execution_id: crate::v1::collaboration::CollaborationExecutionId,
    },
    /// Hidden provider turn triggered by an active goal while the session is idle.
    /// 会话空闲时由活跃目标触发的隐藏 Provider 回合。
    GoalContinuation,
    /// Context compaction turn.
    /// 上下文压缩回合。
    ContextCompaction,
    /// Runtime maintenance turn.
    /// 运行时维护回合。
    Maintenance,
}

/// Terminal status for one active turn.
/// 单个活动回合的终态状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnTerminalStatus {
    /// Turn completed normally.
    /// 回合正常完成。
    Completed,
    /// Turn failed with an error.
    /// 回合因错误失败。
    Failed,
    /// Turn was stopped by user or runtime cancellation.
    /// 回合被用户或运行时取消停止。
    Stopped,
}

/// One active turn owned by a session runtime kernel.
/// 由会话运行时内核拥有的单个活动回合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveTurn {
    /// Stable turn identifier.
    /// 稳定回合标识符。
    pub turn_id: String,
    /// Owning session node.
    /// 所属会话节点。
    pub node_id: SessionNodeId,
    /// Active turn kind.
    /// 活动回合类型。
    pub kind: ActiveTurnKind,
    /// Monotonic generation captured when the turn starts.
    /// 回合启动时捕获的单调代际。
    pub generation: u64,
    /// Unix timestamp when the turn started.
    /// 回合开始时的 Unix 毫秒时间戳。
    pub started_at_ms: u64,
    /// Unix timestamp for the latest observed activity.
    /// 最近一次观测到活动的 Unix 毫秒时间戳。
    pub last_activity_at_ms: u64,
}

impl ActiveTurn {
    /// Builds one active turn record.
    /// 构建一条活动回合记录。
    pub fn new(
        turn_id: impl Into<String>,
        node_id: SessionNodeId,
        kind: ActiveTurnKind,
        generation: u64,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            turn_id: turn_id.into(),
            node_id,
            kind,
            generation,
            started_at_ms: timestamp_ms,
            last_activity_at_ms: timestamp_ms,
        }
    }

    /// Updates the last-activity timestamp.
    /// 更新最近活动时间戳。
    pub fn touch(&mut self, timestamp_ms: u64) {
        self.last_activity_at_ms = timestamp_ms;
    }
}

/// Error returned by session runtime kernel state transitions.
/// 会话运行时内核状态迁移返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRuntimeKernelError {
    /// Human-readable error message.
    /// 面向人的错误信息。
    pub message: String,
}

impl SessionRuntimeKernelError {
    /// Builds one kernel error.
    /// 构建一个内核错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Active wait scope registered by a model-requested wait tool.
/// 由模型请求的 wait 工具注册的活动等待范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitScope {
    /// Stable wait-scope identifier.
    /// 稳定等待范围标识符。
    pub scope_id: String,
    /// Target child-agent identifiers; empty means every child under the parent.
    /// 目标子代理标识符；为空表示父级下全部子代理。
    pub target_agent_ids: BTreeSet<String>,
    /// Input identifiers consumed by this wait scope.
    /// 该等待范围已消费的输入标识符。
    pub consumed_input_ids: BTreeSet<String>,
    /// Unix timestamp when the wait scope was registered.
    /// 等待范围注册时的 Unix 毫秒时间戳。
    pub started_at_ms: u64,
}

impl WaitScope {
    /// Builds one active wait scope.
    /// 构建一个活动等待范围。
    pub fn new(
        scope_id: impl Into<String>,
        target_agent_ids: impl IntoIterator<Item = String>,
        started_at_ms: u64,
    ) -> Self {
        Self {
            scope_id: scope_id.into(),
            target_agent_ids: target_agent_ids.into_iter().collect(),
            consumed_input_ids: BTreeSet::new(),
            started_at_ms,
        }
    }

    /// Returns whether this scope targets the provided child-agent identifier.
    /// 返回该等待范围是否匹配指定子代理标识符。
    pub fn targets_agent(&self, agent_id: &str) -> bool {
        self.target_agent_ids.is_empty() || self.target_agent_ids.contains(agent_id)
    }

    /// Marks one input identifier as consumed by this wait scope.
    /// 将一个输入标识符标记为已被该等待范围消费。
    pub fn mark_input_consumed(&mut self, input_id: impl Into<String>) -> bool {
        self.consumed_input_ids.insert(input_id.into())
    }
}

/// Lightweight projection of one session runtime kernel for UI and scheduler decisions.
/// 单个会话运行时内核提供给 UI 与调度器判断使用的轻量投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimeKernelSnapshot {
    /// Owning session node.
    /// 所属会话节点。
    pub node_id: SessionNodeId,
    /// Current runtime generation.
    /// 当前运行时代际。
    pub generation: u64,
    /// Whether the current generation has been stopped.
    /// 当前代际是否已经停止。
    pub stopped: bool,
    /// Active turn identifier when a turn is running.
    /// 存在运行中回合时的活动回合标识。
    pub active_turn_id: Option<String>,
    /// Active turn kind when a turn is running.
    /// 存在运行中回合时的活动回合类型。
    pub active_turn_kind: Option<ActiveTurnKind>,
    /// Generation captured by the active turn when a turn is running.
    /// 存在运行中回合时该回合捕获的代际。
    pub active_turn_generation: Option<u64>,
    /// Current mailbox delivery phase.
    /// 当前 mailbox 投递阶段。
    pub mailbox_delivery_phase: MailboxDeliveryPhase,
    /// Number of inputs accepted into the current turn buffer.
    /// 已进入当前回合缓冲区的输入数量。
    pub current_turn_input_count: usize,
    /// Number of mailbox inputs retained at the session boundary.
    /// 保留在会话边界的 mailbox 输入数量。
    pub mailbox_input_count: usize,
    /// Number of unconsumed mailbox inputs retained at the session boundary.
    /// 保留在会话边界且尚未消费的 mailbox 输入数量。
    pub unconsumed_mailbox_input_count: usize,
    /// Whether an unconsumed mailbox input may trigger a turn.
    /// 是否存在可触发回合的未消费 mailbox 输入。
    pub trigger_mailbox_input_available: bool,
    /// Number of active wait scopes.
    /// 活动等待范围数量。
    pub wait_scope_count: usize,
}

impl SessionRuntimeKernelSnapshot {
    /// Returns whether this snapshot has an active turn.
    /// 返回该快照是否存在活动回合。
    pub fn has_active_turn(&self) -> bool {
        self.active_turn_id.is_some()
    }

    /// Returns whether this snapshot should be considered busy by schedulers.
    /// 返回调度器是否应将该快照视为繁忙。
    pub fn is_busy(&self) -> bool {
        self.has_active_turn()
            || self.current_turn_input_count > 0
            || self.unconsumed_mailbox_input_count > 0
            || self.wait_scope_count > 0
    }
}

/// Minimal single-source runtime state for one session node.
/// 单个会话节点的最小单一事实运行态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRuntimeKernel {
    /// Owning session node.
    /// 所属会话节点。
    pub node_id: SessionNodeId,
    /// Current active turn when one is running.
    /// 正在运行时的当前活动回合。
    active_turn: Option<ActiveTurn>,
    /// Queue that owns active-turn input and deferred mailbox input.
    /// 持有活动回合输入与延迟 mailbox 输入的队列。
    input_queue: TurnInputQueue,
    /// Current runtime generation used to suppress late stopped events.
    /// 用于抑制停止后迟到事件的当前运行时代际。
    generation: u64,
    /// Whether the current generation has been stopped.
    /// 当前代际是否已经被停止。
    stopped: bool,
    /// Active wait scopes keyed by stable scope identifier.
    /// 按稳定范围标识索引的活动等待范围。
    wait_scopes: BTreeMap<String, WaitScope>,
}

impl SessionRuntimeKernel {
    /// Builds an idle runtime kernel for one session node.
    /// 为单个会话节点构建空闲运行时内核。
    pub fn new(node_id: SessionNodeId) -> Self {
        Self {
            node_id,
            active_turn: None,
            input_queue: TurnInputQueue::new(),
            generation: 0,
            stopped: false,
            wait_scopes: BTreeMap::new(),
        }
    }

    /// Returns the current active turn when present.
    /// 存在时返回当前活动回合。
    pub const fn active_turn(&self) -> Option<&ActiveTurn> {
        self.active_turn.as_ref()
    }

    /// Returns the current runtime generation.
    /// 返回当前运行时代际。
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns whether the kernel has an active turn.
    /// 返回内核是否存在活动回合。
    pub fn has_active_turn(&self) -> bool {
        self.active_turn.is_some()
    }

    /// Returns whether the current generation has been stopped.
    /// 返回当前代际是否已经停止。
    pub const fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Returns the input queue as an immutable reference.
    /// 返回输入队列的不可变引用。
    pub const fn input_queue(&self) -> &TurnInputQueue {
        &self.input_queue
    }

    /// Returns the input queue as a mutable reference.
    /// 返回输入队列的可变引用。
    pub fn input_queue_mut(&mut self) -> &mut TurnInputQueue {
        &mut self.input_queue
    }

    /// Builds a lightweight runtime projection without consuming queued input.
    /// 构建不消费排队输入的轻量运行时投影。
    pub fn snapshot(&self) -> SessionRuntimeKernelSnapshot {
        SessionRuntimeKernelSnapshot {
            node_id: self.node_id,
            generation: self.generation,
            stopped: self.stopped,
            active_turn_id: self.active_turn.as_ref().map(|turn| turn.turn_id.clone()),
            active_turn_kind: self.active_turn.as_ref().map(|turn| turn.kind.clone()),
            active_turn_generation: self.active_turn.as_ref().map(|turn| turn.generation),
            mailbox_delivery_phase: self.input_queue.delivery_phase(),
            current_turn_input_count: self.input_queue.current_turn_len(),
            mailbox_input_count: self.input_queue.mailbox_len(),
            unconsumed_mailbox_input_count: self.input_queue.unconsumed_mailbox_len(),
            trigger_mailbox_input_available: self.input_queue.has_trigger_turn_mailbox_items(),
            wait_scope_count: self.wait_scopes.len(),
        }
    }

    /// Returns whether a new user-authored turn may be started immediately.
    /// 返回是否可以立即启动新的用户编写回合。
    pub fn can_start_user_turn(&self) -> bool {
        self.active_turn.is_none()
    }

    /// Returns whether a mailbox-triggered turn may be started immediately.
    /// 返回是否可以立即启动 mailbox 触发回合。
    pub fn can_start_mailbox_turn(&self) -> bool {
        self.active_turn.is_none()
            && !self.stopped
            && self.input_queue.has_trigger_turn_mailbox_items()
    }

    /// Returns whether at least one wait scope is active.
    /// 返回是否存在至少一个活动等待范围。
    pub fn has_wait_scopes(&self) -> bool {
        !self.wait_scopes.is_empty()
    }

    /// Returns the active wait-scope count.
    /// 返回活动等待范围数量。
    pub fn wait_scope_count(&self) -> usize {
        self.wait_scopes.len()
    }

    /// Registers one wait scope for model-requested child-agent waiting.
    /// 为模型请求的子代理等待注册一个等待范围。
    pub fn register_wait_scope(
        &mut self,
        scope_id: impl Into<String>,
        target_agent_ids: impl IntoIterator<Item = String>,
        timestamp_ms: u64,
    ) -> Result<WaitScope, SessionRuntimeKernelError> {
        let scope = WaitScope::new(scope_id, target_agent_ids, timestamp_ms);
        if self.wait_scopes.contains_key(&scope.scope_id) {
            return Err(SessionRuntimeKernelError::new(
                "The session already has a wait scope with this identifier.",
            ));
        }
        self.wait_scopes
            .insert(scope.scope_id.clone(), scope.clone());
        Ok(scope)
    }

    /// Clears one wait scope by identifier.
    /// 按标识符清理一个等待范围。
    pub fn clear_wait_scope(&mut self, scope_id: &str) -> Option<WaitScope> {
        self.wait_scopes.remove(scope_id)
    }

    /// Clears every active wait scope and returns the removed count.
    /// 清理全部活动等待范围并返回被移除的数量。
    pub fn clear_wait_scopes(&mut self) -> usize {
        let removed = self.wait_scopes.len();
        self.wait_scopes.clear();
        removed
    }

    /// Returns whether any active wait scope targets one child-agent identifier.
    /// 返回是否有任一活动等待范围匹配某个子代理标识符。
    pub fn wait_scope_targets_agent(&self, agent_id: &str) -> bool {
        self.wait_scopes
            .values()
            .any(|scope| scope.targets_agent(agent_id))
    }

    /// Marks one mailbox input as consumed by a wait scope and the global input queue.
    /// 将一个 mailbox 输入标记为被等待范围及全局输入队列消费。
    pub fn consume_mailbox_input_for_wait_scope(&mut self, scope_id: &str, input_id: &str) -> bool {
        let Some(scope) = self.wait_scopes.get_mut(scope_id) else {
            return false;
        };
        let scope_consumed = scope.mark_input_consumed(input_id.to_string());
        let queue_consumed = self.input_queue.mark_input_consumed(input_id.to_string());
        scope_consumed || queue_consumed
    }

    /// Starts a new active turn when the session is idle.
    /// 会话空闲时启动一个新的活动回合。
    pub fn start_turn(
        &mut self,
        turn_id: impl Into<String>,
        kind: ActiveTurnKind,
        timestamp_ms: u64,
    ) -> Result<ActiveTurn, SessionRuntimeKernelError> {
        if self.active_turn.is_some() {
            return Err(SessionRuntimeKernelError::new(
                "The session already has an active turn.",
            ));
        }
        self.stopped = false;
        if kind != ActiveTurnKind::MailboxTriggered {
            self.generation = self.generation.saturating_add(1);
        }
        if kind == ActiveTurnKind::MailboxTriggered {
            self.input_queue.accept_mailbox_delivery_for_current_turn();
        } else {
            self.input_queue.accept_mailbox_delivery_for_current_turn();
            self.input_queue.defer_mailbox_delivery_to_next_turn();
        }
        let turn = ActiveTurn::new(turn_id, self.node_id, kind, self.generation, timestamp_ms);
        self.active_turn = Some(turn.clone());
        Ok(turn)
    }

    /// Finishes the current active turn and closes current-turn mailbox delivery.
    /// 结束当前活动回合并关闭当前回合 mailbox 投递。
    pub fn finish_turn(
        &mut self,
        _status: TurnTerminalStatus,
        timestamp_ms: u64,
    ) -> Option<ActiveTurn> {
        if let Some(turn) = self.active_turn.as_mut() {
            turn.touch(timestamp_ms);
        }
        self.input_queue.close_current_turn_mailbox_delivery();
        self.input_queue
            .defer_unconsumed_current_turn_inputs_to_mailbox();
        self.active_turn.take()
    }

    /// Stops the current generation and removes the active turn.
    /// 停止当前代际并移除活动回合。
    pub fn stop_current_generation(&mut self, timestamp_ms: u64) -> Option<ActiveTurn> {
        self.stopped = true;
        self.wait_scopes.clear();
        if let Some(turn) = self.active_turn.as_mut() {
            turn.touch(timestamp_ms);
        }
        self.input_queue.close_current_turn_mailbox_delivery();
        self.input_queue.discard_current_turn_inputs();
        self.active_turn.take()
    }

    /// Enqueues a mailbox input if the parent generation is still allowed to receive it.
    /// 当父级代际仍允许接收时加入一条 mailbox 输入。
    pub fn enqueue_mailbox_input_for_generation(
        &mut self,
        input: TurnInput,
        source_generation: u64,
    ) -> bool {
        if source_generation != self.generation || self.stopped {
            return false;
        }
        self.input_queue.enqueue_mailbox_input(input)
    }

    /// Starts a mailbox-triggered turn when idle and trigger input exists.
    /// 空闲且存在触发输入时启动一个 mailbox 触发回合。
    pub fn try_start_turn_if_idle(
        &mut self,
        turn_id: impl Into<String>,
        timestamp_ms: u64,
    ) -> Result<Option<ActiveTurn>, SessionRuntimeKernelError> {
        if !self.can_start_mailbox_turn() {
            return Ok(None);
        }
        self.start_turn(turn_id, ActiveTurnKind::MailboxTriggered, timestamp_ms)
            .map(Some)
    }

    /// Returns the current mailbox delivery phase.
    /// 返回当前 mailbox 投递阶段。
    pub fn mailbox_delivery_phase(&self) -> MailboxDeliveryPhase {
        self.input_queue.delivery_phase()
    }

    /// Converts a persisted kernel into a safe idle state after process restart.
    /// 进程重启后将持久化内核转换为安全的空闲状态。
    ///
    /// # Parameters
    /// 参数
    /// `timestamp_ms` is the Unix millisecond timestamp used for terminal activity bookkeeping.
    /// `timestamp_ms` 是用于终态活动记录的 Unix 毫秒时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when an active turn was cleared during restore.
    /// 恢复期间清理了活动回合时返回 `true`。
    pub fn restore_idle_after_process_restart(&mut self, timestamp_ms: u64) -> bool {
        let had_active_turn = self.active_turn.is_some();
        if let Some(turn) = self.active_turn.as_mut() {
            turn.touch(timestamp_ms);
        }
        self.input_queue.close_current_turn_mailbox_delivery();
        self.input_queue
            .defer_unconsumed_current_turn_inputs_to_mailbox();
        self.input_queue.defer_mailbox_delivery_to_next_turn();
        self.active_turn = None;
        self.wait_scopes.clear();
        if had_active_turn {
            self.stopped = true;
        }
        had_active_turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn_input_queue::{TurnInput, TurnInputKind};
    use serde_json::json;

    /// Builds one trigger-turn subagent mailbox input.
    /// 构建一条可触发回合的子代理 mailbox 输入。
    fn trigger_input(node_id: SessionNodeId) -> TurnInput {
        TurnInput::new(
            "child-completed",
            node_id,
            TurnInputKind::SubagentMessage,
            true,
            100,
            json!({ "result": "done" }),
        )
    }

    /// Verifies only one active turn may exist at a time.
    /// 验证同一时间只能存在一个活动回合。
    #[test]
    fn kernel_allows_only_one_active_turn() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);

        kernel
            .start_turn("turn-1", ActiveTurnKind::UserPrompt, 10)
            .expect("first turn should start");
        let error = kernel
            .start_turn("turn-2", ActiveTurnKind::UserPrompt, 20)
            .expect_err("second turn should be rejected");

        assert!(error.message.contains("active turn"));
        assert_eq!(
            kernel.active_turn().map(|turn| turn.turn_id.as_str()),
            Some("turn-1")
        );
    }

    /// Question identity survives projection and serialization, then leaves with the stopped generation.
    /// 问题身份跨投影与序列化保留，随后随被停止代际一起移出。
    #[test]
    fn question_execution_identity_is_owned_by_one_kernel_generation() {
        // execution_id represents one reliable auxiliary operation rather than a display label.
        // execution_id 表示一个可靠辅助操作，而非显示标签。
        let execution_id =
            crate::v1::collaboration::CollaborationExecutionId::try_new("question-run").unwrap();
        // kernel is isolated and uses a real runtime turn transition.
        // kernel 独立且使用真实运行时回合转换。
        let mut kernel = SessionRuntimeKernel::new(SessionNodeId::new_v4());
        let kind = ActiveTurnKind::QuestionExecution {
            execution_id: execution_id.clone(),
        };
        kernel
            .start_turn("qa-preparation", kind.clone(), 10)
            .unwrap();
        assert_eq!(kernel.snapshot().active_turn_kind, Some(kind.clone()));
        // restored preserves the exact current-contract identity with no legacy decoder.
        // restored 保留当前契约的精确身份，不使用历史解码器。
        let mut restored: SessionRuntimeKernel =
            serde_json::from_str(&serde_json::to_string(&kernel).unwrap()).unwrap();
        assert_eq!(restored.stop_current_generation(20).unwrap().kind, kind);
        assert!(restored.snapshot().active_turn_kind.is_none());
        restored
            .start_turn("business", ActiveTurnKind::UserPrompt, 30)
            .unwrap();
        assert_eq!(
            restored.snapshot().active_turn_kind,
            Some(ActiveTurnKind::UserPrompt)
        );
    }

    /// Verifies trigger mailbox input starts an idle mailbox turn.
    /// 验证触发型 mailbox 输入会启动空闲 mailbox 回合。
    #[test]
    fn trigger_mailbox_input_starts_idle_turn() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        assert!(kernel.enqueue_mailbox_input_for_generation(trigger_input(node_id), 0));

        let turn = kernel
            .try_start_turn_if_idle("mailbox-turn-1", 200)
            .expect("try start should not fail")
            .expect("trigger mailbox should start turn");

        assert_eq!(turn.kind, ActiveTurnKind::MailboxTriggered);
        assert_eq!(kernel.input_queue().current_turn_len(), 1);
        assert_eq!(kernel.input_queue().mailbox_len(), 0);
    }

    /// Verifies mailbox-triggered turns preserve the parent generation for sibling child-agent callbacks.
    /// 验证 mailbox 触发回合会保留父级代际，使兄弟子代理回流仍可进入。
    #[test]
    fn mailbox_turn_preserves_generation_for_sibling_callbacks() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        kernel
            .start_turn("turn-1", ActiveTurnKind::UserPrompt, 10)
            .expect("user turn should start");
        let child_generation = kernel.generation();
        kernel.finish_turn(TurnTerminalStatus::Completed, 20);
        assert!(kernel.enqueue_mailbox_input_for_generation(
            TurnInput::new(
                "child-a",
                node_id,
                TurnInputKind::SubagentMessage,
                true,
                30,
                json!({ "result": "a" }),
            ),
            child_generation
        ));

        kernel
            .try_start_turn_if_idle("mailbox-turn-1", 40)
            .expect("mailbox start should not fail")
            .expect("mailbox turn should start");

        assert_eq!(kernel.generation(), child_generation);
        assert!(kernel.enqueue_mailbox_input_for_generation(
            TurnInput::new(
                "child-b",
                node_id,
                TurnInputKind::SubagentMessage,
                true,
                50,
                json!({ "result": "b" }),
            ),
            child_generation
        ));
    }

    /// Verifies unconsumed mailbox inputs that arrive during a mailbox turn trigger a later turn.
    /// 验证 mailbox 回合期间到达且未消费的输入会继续触发后续回合。
    #[test]
    fn unfinished_mailbox_turn_inputs_defer_to_later_turn() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        assert!(kernel.enqueue_mailbox_input_for_generation(trigger_input(node_id), 0));
        kernel
            .try_start_turn_if_idle("mailbox-turn-1", 20)
            .expect("mailbox start should not fail")
            .expect("first mailbox turn should start");
        let initial_inputs = kernel.input_queue_mut().drain_current_turn_inputs();
        assert_eq!(initial_inputs.len(), 1);
        assert!(kernel.enqueue_mailbox_input_for_generation(
            TurnInput::new(
                "child-late",
                node_id,
                TurnInputKind::SubagentMessage,
                true,
                30,
                json!({ "result": "late" }),
            ),
            0
        ));

        kernel.finish_turn(TurnTerminalStatus::Completed, 40);

        assert_eq!(kernel.input_queue().current_turn_len(), 0);
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
        assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
        let next_turn = kernel
            .try_start_turn_if_idle("mailbox-turn-2", 50)
            .expect("second mailbox start should not fail")
            .expect("late child input should start another mailbox turn");
        assert_eq!(next_turn.kind, ActiveTurnKind::MailboxTriggered);
    }

    /// Verifies stopped generations suppress late mailbox-triggered work.
    /// 验证已停止代际会抑制迟到的 mailbox 触发工作。
    #[test]
    fn stopped_generation_suppresses_late_mailbox_input() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        let turn = kernel
            .start_turn("turn-1", ActiveTurnKind::UserPrompt, 10)
            .expect("turn should start");
        kernel.stop_current_generation(20);

        let accepted =
            kernel.enqueue_mailbox_input_for_generation(trigger_input(node_id), turn.generation);
        let next_turn = kernel
            .try_start_turn_if_idle("mailbox-turn-1", 200)
            .expect("try start should not fail");

        assert!(!accepted);
        assert!(next_turn.is_none());
        assert_eq!(kernel.input_queue().mailbox_len(), 0);
    }

    /// Verifies late mailbox input from an older generation cannot enter a newer turn.
    /// 验证旧代际迟到 mailbox 输入不能进入新的回合。
    #[test]
    fn stale_generation_mailbox_input_is_rejected_after_new_user_turn() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        let stopped_turn = kernel
            .start_turn("turn-1", ActiveTurnKind::UserPrompt, 10)
            .expect("first turn should start");
        kernel.stop_current_generation(20);
        let new_turn = kernel
            .start_turn("turn-2", ActiveTurnKind::UserPrompt, 30)
            .expect("new user turn should start after stop");

        let accepted = kernel
            .enqueue_mailbox_input_for_generation(trigger_input(node_id), stopped_turn.generation);

        assert_ne!(stopped_turn.generation, new_turn.generation);
        assert!(!accepted);
        assert_eq!(kernel.input_queue().mailbox_len(), 0);
    }

    /// Verifies the kernel snapshot exposes projection facts without consuming queued input.
    /// 验证内核快照能暴露投影事实且不会消费排队输入。
    #[test]
    fn kernel_snapshot_exposes_projection_state_without_consuming_input() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        kernel
            .register_wait_scope("wait-1", vec![String::from("agent-a")], 10)
            .expect("wait scope should register");
        assert!(kernel.enqueue_mailbox_input_for_generation(trigger_input(node_id), 0));

        let snapshot = kernel.snapshot();

        assert_eq!(snapshot.node_id, node_id);
        assert_eq!(snapshot.generation, 0);
        assert!(!snapshot.stopped);
        assert_eq!(snapshot.active_turn_id, None);
        assert_eq!(
            snapshot.mailbox_delivery_phase,
            MailboxDeliveryPhase::DeferredToNextTurn
        );
        assert_eq!(snapshot.current_turn_input_count, 0);
        assert_eq!(snapshot.mailbox_input_count, 1);
        assert_eq!(snapshot.unconsumed_mailbox_input_count, 1);
        assert!(snapshot.trigger_mailbox_input_available);
        assert_eq!(snapshot.wait_scope_count, 1);
        assert!(snapshot.is_busy());
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
    }

    /// Verifies wait scopes consume mailbox input and suppress automatic turns.
    /// 验证等待范围会消费 mailbox 输入并抑制自动回合。
    #[test]
    fn wait_scope_consumes_mailbox_input_and_suppresses_auto_turn() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        kernel
            .register_wait_scope("wait-1", vec![String::from("agent-a")], 10)
            .expect("wait scope should register");
        assert!(kernel.wait_scope_targets_agent("agent-a"));
        assert!(!kernel.wait_scope_targets_agent("agent-b"));
        assert!(kernel.enqueue_mailbox_input_for_generation(trigger_input(node_id), 0));

        assert!(kernel.consume_mailbox_input_for_wait_scope("wait-1", "child-completed"));

        let next_turn = kernel
            .try_start_turn_if_idle("mailbox-turn-1", 200)
            .expect("try start should not fail");
        assert!(next_turn.is_none());
        assert!(kernel.input_queue().is_input_consumed("child-completed"));
    }

    /// Verifies stopping a generation clears active wait scopes.
    /// 验证停止代际会清理活动等待范围。
    #[test]
    fn stop_current_generation_clears_wait_scopes() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        kernel
            .register_wait_scope("wait-1", Vec::<String>::new(), 10)
            .expect("wait scope should register");
        kernel
            .start_turn("turn-1", ActiveTurnKind::UserPrompt, 20)
            .expect("turn should start");

        kernel.stop_current_generation(30);

        assert!(!kernel.has_wait_scopes());
        assert_eq!(kernel.wait_scope_count(), 0);
    }

    /// Verifies explicit wait-scope cleanup removes all active scopes without stopping the session.
    /// 验证显式等待范围清理会移除全部活动范围且不会停止会话。
    #[test]
    fn clear_wait_scopes_removes_all_scopes_without_stopping_kernel() {
        let node_id = SessionNodeId::new_v4();
        let mut kernel = SessionRuntimeKernel::new(node_id);
        kernel
            .register_wait_scope("wait-a", Vec::<String>::new(), 10)
            .expect("first wait scope should register");
        kernel
            .register_wait_scope("wait-b", vec![String::from("agent-a")], 20)
            .expect("second wait scope should register");

        assert_eq!(kernel.clear_wait_scopes(), 2);
        assert_eq!(kernel.wait_scope_count(), 0);
        assert!(!kernel.is_stopped());
    }
}
