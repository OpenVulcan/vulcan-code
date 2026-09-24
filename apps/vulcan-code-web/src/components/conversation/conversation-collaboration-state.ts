import * as React from "react";
import type { CollaborationAfterState, ConversationEventSummary } from "@/service/conversation-event-window-store";

/** Latest presentation-only facts indexed by their producer-owned identities.
 * 按生产者权威身份索引的最新只读展示事实。 */
export interface ConversationCollaborationState {
  /** Source-owned question snapshots, including their final answer or cancellation.
   * 来源持有的问题快照，包含最终回答或撤销。 */
  questions: ReadonlyMap<string, Extract<CollaborationAfterState, { kind: "question" }>>;
  /** Latest delivery progress for each exact question in this session.
   * 当前会话内每个精确问题的最新投递进度。 */
  deliveries: ReadonlyMap<string, Extract<CollaborationAfterState, { kind: "outbound_delivery" | "inbound_delivery" }>>;
  /** Child lifecycle facts keyed by execution, never by reusable agent name.
   * 按执行身份索引的子任务生命周期事实，绝不按可复用代理名称索引。 */
  tasks: ReadonlyMap<string, Extract<CollaborationAfterState, { kind: "child_task" }>>;
  /** Latest task-message content and acceptance, indexed by immutable delivery ID.
   * 按不可变投递身份索引的最新任务消息正文与接纳状态。 */
  taskMessages: ReadonlyMap<string, Extract<CollaborationAfterState, { kind: "outbound_task_message" | "inbound_task_message" }>>;
  /** Application outcomes enrich the original message without replacing its body.
   * 应用结果补充原消息，不替换其正文。 */
  taskMessageEffects: ReadonlyMap<string, Extract<CollaborationAfterState, { kind: "task_message_effect" }>>;
}

/** Shares the exact mounted session's facts with both tool and message cards.
 * 向工具与消息卡片共享精确已挂载会话的事实。 */
export const ConversationCollaborationStateContext = React.createContext<ConversationCollaborationState>({
  questions: new Map(), deliveries: new Map(), tasks: new Map(), taskMessages: new Map(), taskMessageEffects: new Map(),
});

/** Folds canonical event order into latest snapshots without modifying persisted history.
 * 按规范事件顺序折叠为最新快照，不修改持久化历史。
 * Accepts one session window and returns identity-bound display state.
 * 接收单个会话窗口，返回绑定身份的展示状态。 */
export function projectConversationCollaborationState(events: readonly ConversationEventSummary[]): ConversationCollaborationState {
  // Separate maps preserve the distinction between question settlement and transport progress.
  // 分离映射以保留问题结算和传输进度的区别。
  const questions = new Map<string, Extract<CollaborationAfterState, { kind: "question" }>>();
  const deliveries = new Map<string, Extract<CollaborationAfterState, { kind: "outbound_delivery" | "inbound_delivery" }>>();
  const tasks = new Map<string, Extract<CollaborationAfterState, { kind: "child_task" }>>();
  // Content and effect snapshots remain separate so state-only receipts cannot erase the message.
  // 正文与效果快照分开保留，避免纯状态回执擦除消息。
  const taskMessages = new Map<string, Extract<CollaborationAfterState, { kind: "outbound_task_message" | "inbound_task_message" }>>();
  const taskMessageEffects = new Map<string, Extract<CollaborationAfterState, { kind: "task_message_effect" }>>();
  for (const summary of events) {
    if (summary.event.kind !== "collaboration") continue;
    // The discriminant fixes ownership; no chronological-neighbor or name matching is permitted.
    // 判别字段确定归属；不按相邻时间或名称匹配。
    const state = summary.event.after_state;
    switch (state.kind) {
      case "outbound_task_message":
        taskMessages.set(state.record.message.delivery_id, state);
        break;
      case "inbound_task_message":
        taskMessages.set(state.acceptance.message.delivery_id, state);
        break;
      case "task_message_effect":
        taskMessageEffects.set(state.effect.delivery_id, state);
        break;
      case "question":
        questions.set(state.question.question_id, state);
        deliveries.set(state.question.question_id, { kind: "outbound_delivery", delivery: state.delivery });
        break;
      case "outbound_delivery":
      case "inbound_delivery":
        deliveries.set(state.delivery.question_id, state);
        break;
      case "child_task":
        tasks.set(state.task.execution_id, state);
        break;
    }
  }
  return { questions, deliveries, tasks, taskMessages, taskMessageEffects };
}
