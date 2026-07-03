use crate::model::SessionNodeId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::collections::VecDeque;

/// Input item kind accepted by one provider turn.
/// 单个 Provider 回合可接收的输入项类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnInputKind {
    /// User-authored prompt input.
    /// 用户编写的提示词输入。
    UserInput,
    /// Result emitted by a tool call.
    /// 工具调用产出的结果。
    ToolResult,
    /// Message delivered from a child agent.
    /// 从子代理投递回来的消息。
    SubagentMessage,
    /// System-owned injected input item.
    /// 系统拥有的注入输入项。
    SystemInjected,
}

/// Mailbox delivery phase for the currently active turn.
/// 当前活动回合的 mailbox 投递阶段。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailboxDeliveryPhase {
    /// Mailbox items may still join the current active turn.
    /// mailbox 输入项仍可进入当前活动回合。
    AcceptingCurrentTurn,
    /// Mailbox items must wait for the next turn.
    /// mailbox 输入项必须等待下一个回合。
    #[default]
    DeferredToNextTurn,
    /// The current turn is closing and cannot accept mailbox input.
    /// 当前回合正在关闭，不能再接收 mailbox 输入。
    Closed,
}

/// One queued input item owned by a session turn or mailbox.
/// 由会话回合或 mailbox 拥有的一条排队输入项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnInput {
    /// Stable input identifier used for dedupe and audit.
    /// 用于去重与审计的稳定输入标识符。
    pub input_id: String,
    /// Owning root or task session node.
    /// 所属根会话或子任务会话节点。
    pub node_id: SessionNodeId,
    /// Optional parent turn identifier that produced this input.
    /// 产出该输入的可选父级回合标识符。
    pub parent_turn_id: Option<String>,
    /// Input kind.
    /// 输入项类型。
    pub kind: TurnInputKind,
    /// Whether this mailbox item may start a new turn when the parent is idle.
    /// 父会话空闲时该 mailbox 输入项是否可以启动新回合。
    pub trigger_turn: bool,
    /// Unix timestamp in milliseconds.
    /// Unix 毫秒时间戳。
    pub timestamp_ms: u64,
    /// Caller-owned structured payload.
    /// 调用方拥有的结构化载荷。
    pub payload: Value,
}

impl TurnInput {
    /// Builds one queued turn input item.
    /// 构建一条排队回合输入项。
    pub fn new(
        input_id: impl Into<String>,
        node_id: SessionNodeId,
        kind: TurnInputKind,
        trigger_turn: bool,
        timestamp_ms: u64,
        payload: Value,
    ) -> Self {
        Self {
            input_id: input_id.into(),
            node_id,
            parent_turn_id: None,
            kind,
            trigger_turn,
            timestamp_ms,
            payload,
        }
    }

    /// Attaches the parent turn identifier that produced this input.
    /// 附加产出该输入的父级回合标识符。
    pub fn with_parent_turn_id(mut self, parent_turn_id: impl Into<String>) -> Self {
        self.parent_turn_id = Some(parent_turn_id.into());
        self
    }
}

/// Session-scoped input queue with separate active-turn and mailbox buffers.
/// 具备活动回合与 mailbox 双缓冲的会话级输入队列。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnInputQueue {
    /// Input items already accepted by the current turn.
    /// 已被当前回合接收的输入项。
    current_turn_inputs: VecDeque<TurnInput>,
    /// Input items waiting at the session mailbox boundary.
    /// 等待在会话 mailbox 边界的输入项。
    session_mailbox_inputs: VecDeque<TurnInput>,
    /// Current mailbox delivery phase.
    /// 当前 mailbox 投递阶段。
    delivery_phase: MailboxDeliveryPhase,
    /// Input identifiers already consumed by wait scopes or equivalent runtime paths.
    /// 已被 wait scope 或等价运行路径消费的输入标识符。
    consumed_input_ids: BTreeSet<String>,
}

impl TurnInputQueue {
    /// Builds an empty input queue.
    /// 构建一个空输入队列。
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current mailbox delivery phase.
    /// 返回当前 mailbox 投递阶段。
    pub const fn delivery_phase(&self) -> MailboxDeliveryPhase {
        self.delivery_phase
    }

    /// Enqueues one input directly into the current turn buffer.
    /// 将一条输入直接加入当前回合缓冲区。
    pub fn enqueue_current_turn_input(&mut self, input: TurnInput) {
        self.current_turn_inputs.push_back(input);
    }

    /// Enqueues one mailbox input according to the current delivery phase.
    /// 根据当前投递阶段加入一条 mailbox 输入。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the input was accepted; `false` when the input id already exists or was consumed.
    /// 输入被接受时返回 `true`；输入标识已存在或已消费时返回 `false`。
    pub fn enqueue_mailbox_input(&mut self, input: TurnInput) -> bool {
        if self.has_input_id(input.input_id.as_str())
            || self.consumed_input_ids.contains(input.input_id.as_str())
        {
            return false;
        }
        if self.delivery_phase == MailboxDeliveryPhase::AcceptingCurrentTurn {
            self.current_turn_inputs.push_back(input);
        } else {
            self.session_mailbox_inputs.push_back(input);
        }
        true
    }

    /// Allows queued mailbox items to join the current turn and moves existing mailbox items into it.
    /// 允许已排队 mailbox 输入进入当前回合，并移动现有 mailbox 输入项。
    pub fn accept_mailbox_delivery_for_current_turn(&mut self) {
        self.delivery_phase = MailboxDeliveryPhase::AcceptingCurrentTurn;
        while let Some(input) = self.session_mailbox_inputs.pop_front() {
            if !self.consumed_input_ids.contains(&input.input_id) {
                self.current_turn_inputs.push_back(input);
            }
        }
    }

    /// Defers all later mailbox items to the next turn.
    /// 将后续全部 mailbox 输入推迟到下一回合。
    pub fn defer_mailbox_delivery_to_next_turn(&mut self) {
        self.delivery_phase = MailboxDeliveryPhase::DeferredToNextTurn;
    }

    /// Closes mailbox delivery for the current turn.
    /// 关闭当前回合的 mailbox 投递。
    pub fn close_current_turn_mailbox_delivery(&mut self) {
        self.delivery_phase = MailboxDeliveryPhase::Closed;
    }

    /// Returns whether any current-turn input is waiting.
    /// 返回当前回合是否存在等待输入。
    pub fn has_pending_input(&self) -> bool {
        !self.current_turn_inputs.is_empty()
    }

    /// Returns whether the session mailbox contains a trigger-turn item.
    /// 返回会话 mailbox 是否包含可触发新回合的输入项。
    pub fn has_trigger_turn_mailbox_items(&self) -> bool {
        self.session_mailbox_inputs
            .iter()
            .any(|input| input.trigger_turn && !self.consumed_input_ids.contains(&input.input_id))
    }

    /// Returns a cloned snapshot of current-turn pending input without consuming it.
    /// 返回当前回合待处理输入的克隆快照且不消费。
    pub fn get_pending_input(&self) -> Vec<TurnInput> {
        self.current_turn_inputs
            .iter()
            .filter(|input| !self.consumed_input_ids.contains(&input.input_id))
            .cloned()
            .collect()
    }

    /// Returns a cloned snapshot of mailbox input without consuming it.
    /// 返回 mailbox 输入项的克隆快照且不消费。
    ///
    /// # Returns
    /// 返回值
    /// Current mailbox inputs in FIFO order.
    /// 按 FIFO 顺序排列的当前 mailbox 输入项。
    pub fn get_mailbox_input(&self) -> Vec<TurnInput> {
        self.session_mailbox_inputs.iter().cloned().collect()
    }

    /// Returns an unconsumed cloned snapshot of mailbox input without consuming it.
    /// 返回未消费 mailbox 输入项的克隆快照且不消费。
    pub fn get_unconsumed_mailbox_input(&self) -> Vec<TurnInput> {
        self.current_turn_inputs
            .iter()
            .chain(self.session_mailbox_inputs.iter())
            .filter(|input| !self.consumed_input_ids.contains(&input.input_id))
            .cloned()
            .collect()
    }

    /// Drains input items accepted by the current turn in FIFO order.
    /// 按先进先出顺序取出当前回合已接收输入项。
    pub fn drain_current_turn_inputs(&mut self) -> Vec<TurnInput> {
        self.current_turn_inputs
            .drain(..)
            .filter(|input| !self.consumed_input_ids.contains(&input.input_id))
            .collect()
    }

    /// Drains current-turn inputs of the requested kind while retaining every other input.
    /// 取出指定类型的当前回合输入，并保留其他所有输入。
    ///
    /// # Parameters
    /// 参数
    /// `kind` is the input kind that should be removed and returned.
    /// `kind` 是需要移除并返回的输入类型。
    ///
    /// # Returns
    /// 返回值
    /// Matching unconsumed inputs in FIFO order.
    /// 按 FIFO 顺序返回匹配的未消费输入。
    pub fn drain_current_turn_inputs_by_kind(&mut self, kind: TurnInputKind) -> Vec<TurnInput> {
        let mut retained = VecDeque::new();
        let mut drained = Vec::new();
        while let Some(input) = self.current_turn_inputs.pop_front() {
            if self.consumed_input_ids.contains(&input.input_id) {
                continue;
            }
            if input.kind == kind {
                drained.push(input);
            } else {
                retained.push_back(input);
            }
        }
        self.current_turn_inputs = retained;
        drained
    }

    /// Moves unconsumed current-turn inputs back to the session mailbox for a later turn.
    /// 将当前回合尚未消费的输入移回会话 mailbox，等待后续回合处理。
    ///
    /// # Returns
    /// 返回值
    /// Number of current-turn inputs deferred back to the session mailbox.
    /// 被推回会话 mailbox 的当前回合输入数量。
    pub fn defer_unconsumed_current_turn_inputs_to_mailbox(&mut self) -> usize {
        let retained_mailbox_inputs = self.session_mailbox_inputs.drain(..).collect::<Vec<_>>();
        let deferred_inputs = self
            .current_turn_inputs
            .drain(..)
            .filter(|input| !self.consumed_input_ids.contains(&input.input_id))
            .collect::<Vec<_>>();
        let deferred_count = deferred_inputs.len();
        self.session_mailbox_inputs.extend(deferred_inputs);
        self.session_mailbox_inputs.extend(retained_mailbox_inputs);
        deferred_count
    }

    /// Discards every queued current-turn input.
    /// 丢弃当前回合中排队的全部输入。
    ///
    /// # Returns
    /// 返回值
    /// Number of discarded current-turn inputs.
    /// 被丢弃的当前回合输入数量。
    pub fn discard_current_turn_inputs(&mut self) -> usize {
        let discarded = self.current_turn_inputs.len();
        self.current_turn_inputs.clear();
        discarded
    }

    /// Drains all session-mailbox input items in FIFO order.
    /// 按先进先出顺序取出全部会话 mailbox 输入项。
    pub fn drain_mailbox_input_items(&mut self) -> Vec<TurnInput> {
        self.session_mailbox_inputs.drain(..).collect()
    }

    /// Drains unconsumed session-mailbox input items in FIFO order.
    /// 按先进先出顺序取出未消费的会话 mailbox 输入项。
    pub fn drain_unconsumed_mailbox_input_items(&mut self) -> Vec<TurnInput> {
        self.session_mailbox_inputs
            .drain(..)
            .filter(|input| !self.consumed_input_ids.contains(&input.input_id))
            .collect()
    }

    /// Marks one input identifier as consumed.
    /// 将一个输入标识符标记为已消费。
    pub fn mark_input_consumed(&mut self, input_id: impl Into<String>) -> bool {
        self.consumed_input_ids.insert(input_id.into())
    }

    /// Returns whether one input identifier has already been consumed.
    /// 返回某个输入标识符是否已经被消费。
    pub fn is_input_consumed(&self, input_id: &str) -> bool {
        self.consumed_input_ids.contains(input_id)
    }

    /// Returns whether the input id is already queued for the current turn or session mailbox.
    /// 返回输入标识是否已经排队于当前回合或会话 mailbox。
    pub fn has_input_id(&self, input_id: &str) -> bool {
        self.current_turn_inputs
            .iter()
            .chain(self.session_mailbox_inputs.iter())
            .any(|input| input.input_id == input_id)
    }

    /// Returns the number of current-turn input items.
    /// 返回当前回合输入项数量。
    pub fn current_turn_len(&self) -> usize {
        self.current_turn_inputs.len()
    }

    /// Returns the number of session-mailbox input items.
    /// 返回会话 mailbox 输入项数量。
    pub fn mailbox_len(&self) -> usize {
        self.session_mailbox_inputs.len()
    }

    /// Returns the number of session-mailbox input items that have not been consumed.
    /// 返回尚未被消费的会话 mailbox 输入项数量。
    pub fn unconsumed_mailbox_len(&self) -> usize {
        self.session_mailbox_inputs
            .iter()
            .filter(|input| !self.consumed_input_ids.contains(&input.input_id))
            .count()
    }

    /// Returns the number of consumed input identifiers.
    /// 返回已消费输入标识符数量。
    pub fn consumed_input_count(&self) -> usize {
        self.consumed_input_ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds one subagent mailbox input for queue tests.
    /// 为队列测试构建一条子代理 mailbox 输入。
    fn subagent_input(id: &str, node_id: SessionNodeId, trigger_turn: bool) -> TurnInput {
        TurnInput::new(
            id,
            node_id,
            TurnInputKind::SubagentMessage,
            trigger_turn,
            100,
            json!({ "result": id }),
        )
    }

    /// Verifies deferred mailbox items stay outside the current turn.
    /// 验证推迟投递的 mailbox 输入不会进入当前回合。
    #[test]
    fn deferred_mailbox_items_wait_outside_current_turn() {
        let node_id = SessionNodeId::new_v4();
        let mut queue = TurnInputQueue::new();

        assert!(queue.enqueue_mailbox_input(subagent_input("agent-1", node_id, true)));

        assert_eq!(queue.current_turn_len(), 0);
        assert_eq!(queue.mailbox_len(), 1);
        assert!(queue.has_trigger_turn_mailbox_items());
        assert!(!queue.has_pending_input());
    }

    /// Verifies accepting mailbox delivery moves queued items into the active turn.
    /// 验证接收 mailbox 投递会把已排队输入移动到活动回合。
    #[test]
    fn accepting_mailbox_delivery_moves_items_into_current_turn() {
        let node_id = SessionNodeId::new_v4();
        let mut queue = TurnInputQueue::new();
        queue.enqueue_mailbox_input(subagent_input("agent-1", node_id, true));

        queue.accept_mailbox_delivery_for_current_turn();

        assert_eq!(
            queue.delivery_phase(),
            MailboxDeliveryPhase::AcceptingCurrentTurn
        );
        assert_eq!(queue.current_turn_len(), 1);
        assert_eq!(queue.mailbox_len(), 0);
        assert!(!queue.has_trigger_turn_mailbox_items());
        assert_eq!(
            queue.get_pending_input()[0].payload["result"],
            json!("agent-1")
        );
    }

    /// Verifies new mailbox items join the active turn only while the phase accepts them.
    /// 验证只有接收阶段的新 mailbox 输入会进入活动回合。
    #[test]
    fn delivery_phase_controls_new_mailbox_items() {
        let node_id = SessionNodeId::new_v4();
        let mut queue = TurnInputQueue::new();
        queue.accept_mailbox_delivery_for_current_turn();
        queue.enqueue_mailbox_input(subagent_input("agent-live", node_id, true));
        queue.defer_mailbox_delivery_to_next_turn();
        queue.enqueue_mailbox_input(subagent_input("agent-late", node_id, true));

        assert_eq!(
            queue
                .drain_current_turn_inputs()
                .into_iter()
                .map(|input| input.input_id)
                .collect::<Vec<_>>(),
            vec!["agent-live"]
        );
        assert_eq!(
            queue
                .drain_mailbox_input_items()
                .into_iter()
                .map(|input| input.input_id)
                .collect::<Vec<_>>(),
            vec!["agent-late"]
        );
    }

    /// Verifies duplicate mailbox input identifiers are rejected before they can be delivered twice.
    /// 验证重复 mailbox 输入标识会被拒绝，避免同一消息被投递两次。
    #[test]
    fn duplicate_mailbox_input_ids_are_rejected() {
        let node_id = SessionNodeId::new_v4();
        let mut queue = TurnInputQueue::new();

        assert!(queue.enqueue_mailbox_input(subagent_input("agent-1", node_id, true)));
        assert!(!queue.enqueue_mailbox_input(subagent_input("agent-1", node_id, true)));

        assert_eq!(queue.mailbox_len(), 1);
        assert!(queue.has_input_id("agent-1"));
    }

    /// Verifies consumed mailbox items do not trigger a new turn.
    /// 验证已消费 mailbox 输入不会触发新回合。
    #[test]
    fn consumed_mailbox_items_do_not_trigger_turn() {
        let node_id = SessionNodeId::new_v4();
        let mut queue = TurnInputQueue::new();
        queue.enqueue_mailbox_input(subagent_input("agent-waited", node_id, true));

        assert!(queue.mark_input_consumed("agent-waited"));

        assert!(!queue.has_trigger_turn_mailbox_items());
        assert!(queue.get_unconsumed_mailbox_input().is_empty());
        assert_eq!(queue.mailbox_len(), 1);
        assert_eq!(queue.unconsumed_mailbox_len(), 0);
        assert_eq!(queue.consumed_input_count(), 1);
    }

    /// Verifies accepting delivery skips mailbox input already consumed by wait scopes.
    /// 验证接收投递时会跳过已被 wait scope 消费的 mailbox 输入。
    #[test]
    fn accepting_delivery_skips_consumed_mailbox_items() {
        let node_id = SessionNodeId::new_v4();
        let mut queue = TurnInputQueue::new();
        queue.enqueue_mailbox_input(subagent_input("agent-waited", node_id, true));
        queue.enqueue_mailbox_input(subagent_input("agent-live", node_id, true));
        queue.mark_input_consumed("agent-waited");

        queue.accept_mailbox_delivery_for_current_turn();

        let inputs = queue.drain_current_turn_inputs();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].input_id, "agent-live");
    }

    /// Verifies active-turn mailbox inputs remain visible to runtime interventions.
    /// 验证当前回合接收的 mailbox 输入对运行时干预仍然可见。
    #[test]
    fn active_turn_mailbox_inputs_are_visible_to_intervention_reads() {
        let node_id = SessionNodeId::new_v4();
        let mut queue = TurnInputQueue::new();
        queue.accept_mailbox_delivery_for_current_turn();
        queue.enqueue_mailbox_input(subagent_input("agent-live", node_id, true));

        let inputs = queue.get_unconsumed_mailbox_input();

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].input_id, "agent-live");
    }

    /// Verifies unconsumed active-turn mailbox inputs are deferred when a turn closes.
    /// 验证回合关闭时未消费的当前回合 mailbox 输入会推迟到后续回合。
    #[test]
    fn unconsumed_active_turn_inputs_defer_back_to_mailbox() {
        let node_id = SessionNodeId::new_v4();
        let mut queue = TurnInputQueue::new();
        queue.accept_mailbox_delivery_for_current_turn();
        queue.enqueue_mailbox_input(subagent_input("agent-live", node_id, true));

        let deferred = queue.defer_unconsumed_current_turn_inputs_to_mailbox();

        assert_eq!(deferred, 1);
        assert_eq!(queue.current_turn_len(), 0);
        assert_eq!(queue.mailbox_len(), 1);
        assert!(queue.has_trigger_turn_mailbox_items());
    }
}
