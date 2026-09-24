import * as React from "react";
import { ChevronDown, MessagesSquare } from "lucide-react";
import { ConversationMeasuredCollapse, useConversationMeasuredExpansion } from "./conversation-measured-collapse";
import { useI18n, type TranslationKey } from "@/i18n";
import { ConversationCollaborationStateContext } from "./conversation-collaboration-state";
import { collaborationTaskMessageId } from "@/service/conversation-event-display-projection";
import { MarkdownMessage } from "./conversation-markdown";
import type { ConversationAgentInteraction } from "@/types/conversation";
import type {
  CollaborationAfterState,
  CollaborationAnswerDeliveryState,
  CollaborationChildTaskState,
  CollaborationChildTaskTerminalOutcome,
  CollaborationDeliveryState,
  CollaborationEventKind,
  CollaborationNotificationState,
  CollaborationQuestionExecutionOutcome,
  CollaborationQuestionExecutionState,
  CollaborationQuestionState,
  CollaborationRunControl,
  CollaborationTargetSettlementState,
  CollaborationTaskTerminal,
  ConversationEventBody,
} from "@/service/conversation-event-window-store";

// ConversationAgentInteractionActions defines host callbacks; event text never grants these capabilities.
// ConversationAgentInteractionActions 定义宿主回调，事件正文绝不授予这些能力。
export interface ConversationAgentInteractionActions {
  // Names keyed by exact child session id from persisted tools and current host navigation.
  // 来自持久工具记录与当前宿主导航、以精确子会话身份索引的名字。
  subagentNames?: ReadonlyMap<string, string>;
  // Opens an exact participant session through an already resolved host route.
  // 通过已解析的宿主路由打开精确参与会话。
  onOpenSession?: (sessionId: string) => void;
  // Contains the current host-owned set of routable session identities.
  // 包含当前宿主持有的可路由会话身份集合。
  availableSessionIds?: ReadonlySet<string>;
}

// ConversationAgentInteractionActionsContext scopes navigation and user actions to one mounted surface.
// ConversationAgentInteractionActionsContext 将导航和用户操作限定在一个已挂载的界面内。
export const ConversationAgentInteractionActionsContext = React.createContext<ConversationAgentInteractionActions>({});

// ConversationAgentInteractionView renders exact V1 facts using the surface-owned action context.
// ConversationAgentInteractionView 使用界面持有的操作上下文渲染精确 V1 事实。
// Parameters: interaction is the original typed event; returns a distinct collaboration or child-result card.
// 参数：interaction 是原始强类型事件；返回独立的协作或子任务结果卡片。
export function ConversationAgentInteractionView({ interaction }: { interaction: ConversationAgentInteraction }) {
  // actions contains callbacks only, never persisted authority or reconstructed message roles.
  // actions 仅包含回调，不包含持久化权威或重建的消息角色。
  const actions = React.useContext(ConversationAgentInteractionActionsContext);
  const { locale, t } = useI18n();
  if (interaction.event.kind === "collaboration") {
    return <ConversationCollaborationCard key={interaction.event.operation_id} {...actions} event={interaction.event} seq={interaction.seq} createdAtMs={interaction.created_at_ms} />;
  }
  return (
    <CollaborationToolCard id={`subagent-return:${interaction.event.child_session_id}:${interaction.seq}`} seq={interaction.seq}
      eventKind="subagent_return" cardKind="task-result" title={t("conversation.collaboration.agentCompleted")}
      name={actions.subagentNames?.get(interaction.event.child_session_id)}>
      <div className="conversation-v1-collaboration-content"><MarkdownMessage content={interaction.event.content} eager /></div>
      <footer className="conversation-v1-collaboration-footer">
        <CollaborationSessionButton {...actions} sessionId={interaction.event.child_session_id} label={t("conversation.collaboration.source")} title={t("conversation.collaboration.openSource")} />
        <time dateTime={new Date(interaction.created_at_ms).toISOString()}>{new Date(interaction.created_at_ms).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })}</time>
      </footer>
    </CollaborationToolCard>
  );
}

/** Renders one collaboration event as an independent user-facing timeline card.
 * 将一条协作事件渲染为独立的用户可见时间线卡片。 */
export function ConversationCollaborationCard(props: ConversationAgentInteractionActions & {
  // Immutable host event snapshot; this is never reconstructed from provider text.
  // 不可变宿主事件快照，绝不由供应商正文重建。
  event: Extract<ConversationEventBody, { kind: "collaboration" }>;
  // Canonical event sequence within the owning session.
  // 所属会话内的规范事件序号。
  seq: number;
  // Original event timestamp, retained across delivery updates.
  // 原始事件时间戳，在投递更新后仍保留。
  createdAtMs: number;
}) {
  const { locale, t } = useI18n();
  // Live display state updates historical question cards without rewriting their source event.
  // 实时展示状态更新历史问题卡片，不改写其来源事件。
  const collaborationState = React.useContext(ConversationCollaborationStateContext);
  const event = props.event;
  // Task delivery IDs bind status changes to the original content and stable fold identity.
  // 任务投递身份将状态变化绑定到原正文与稳定折叠身份。
  const taskMessageId = collaborationTaskMessageId(event.after_state);
  const afterState = event.after_state.kind === "question"
    ? collaborationState.questions.get(event.after_state.question.question_id) ?? event.after_state
    : taskMessageId !== null ? collaborationState.taskMessages.get(taskMessageId) ?? event.after_state : event.after_state;
  const isNotification = afterState.kind === "outbound_notification" || afterState.kind === "inbound_notification";
  // Task-message cards describe delivery, never a child task's terminal result.
  // 任务消息卡片描述投递，绝不表示子任务终态结果。
  // Failure receipts describe message application, not the business task's final answer.
  // 失败回执描述消息应用，不表示业务任务的最终回答。
  const isFailedTaskDelivery = (afterState.kind === "task_result" && afterState.result.terminal === null)
    || (afterState.kind === "received_task_result" && afterState.acceptance.result.terminal === null);
  const isTaskMessage = afterState.kind === "outbound_task_message"
    || afterState.kind === "inbound_task_message" || afterState.kind === "task_message_effect"
    || isFailedTaskDelivery;
  // A durable child non-success report is a result, while its earlier lifecycle events remain controls.
  // 可靠子非成功报告属于结果，其较早生命周期事件仍属于控制信息。
  const isTaskResult = ((afterState.kind === "task_result" || afterState.kind === "received_task_result")
    && !isFailedTaskDelivery) || event.event_kind === "child_task_non_success_recorded";
  const participants = collaborationParticipants(afterState);
  const taskMessageEffect = taskMessageId !== null ? collaborationState.taskMessageEffects.get(taskMessageId) : undefined;
  const status = collaborationStatus(taskMessageEffect ?? afterState, t);
  const title = isFailedTaskDelivery
    ? t("conversation.collaboration.timeline.taskMessage")
    : collaborationTimelineTitle(event.event_kind, t);
  const body = event.event_kind === "child_task_non_success_projected" ? null : collaborationBody(afterState);
  const secondaryBody = collaborationSecondaryBody(afterState);
  return (
    <CollaborationToolCard
      id={taskMessageId !== null ? `collaboration:task-message:${taskMessageId}` : `collaboration:${event.operation_id}:${props.seq}`} seq={props.seq}
      eventKind="collaboration" collaborationEventKind={event.event_kind}
      cardKind={isTaskResult ? "task-result" : isNotification ? "notification" : isTaskMessage ? "task-message" : "interaction"}
      title={title} status={status}
      name={[...new Set(participants.map((participant) => props.subagentNames?.get(participant.sessionId)).filter(Boolean))].join(" · ")}
    >
      {body ? <div className="conversation-v1-collaboration-content"><MarkdownMessage content={body} eager /></div> : null}
      {secondaryBody ? (
        <section className="conversation-v1-collaboration-reply">
          <span className="conversation-v1-collaboration-caption">{t(secondaryBody.label)}</span>
          <MarkdownMessage content={secondaryBody.body} eager />
        </section>
      ) : null}
      {afterState.kind === "question" ? <ConversationCollaborationStatus questionId={afterState.question.question_id} /> : null}
      <footer className="conversation-v1-collaboration-footer">
        {participants.length > 0 ? (
          <div className="conversation-v1-collaboration-participants">
            {participants.map((participant) => (
              <CollaborationSessionButton
                availableSessionIds={props.availableSessionIds}
                subagentNames={props.subagentNames}
                key={`${participant.role}:${participant.sessionId}`}
                label={participant.role === "source" ? t("conversation.collaboration.source") : t("conversation.collaboration.target")}
                onOpenSession={props.onOpenSession}
                sessionId={participant.sessionId}
                title={participant.role === "source" ? t("conversation.collaboration.openSource") : t("conversation.collaboration.openTarget")}
              />
            ))}
          </div>
        ) : null}
        <time dateTime={new Date(props.createdAtMs).toISOString()}>
          {new Date(props.createdAtMs).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })}
        </time>
      </footer>
    </CollaborationToolCard>
  );
}

/** Renders delivery and lifecycle status inside its owning tool card, never as a timeline entry.
 * 在所属工具卡内部渲染投递与生命周期状态，绝不成为独立时间线条目。
 * Exact question/execution identities select current snapshots; absent facts render nothing.
 * 精确问题或执行身份选择当前快照；没有事实时不渲染。 */
export function ConversationCollaborationStatus(props: { questionId?: string | null; executionId?: string | null }) {
  // Context belongs to the mounted session, so identical foreign IDs cannot leak across panes.
  // 上下文归已挂载会话所有，避免其他面板的身份串入。
  const state = React.useContext(ConversationCollaborationStateContext);
  const { t } = useI18n();
  const question = props.questionId ? state.questions.get(props.questionId) : undefined;
  const delivery = props.questionId ? state.deliveries.get(props.questionId) : undefined;
  const task = props.executionId ? state.tasks.get(props.executionId) : undefined;
  const labels = [delivery, question, task].flatMap((fact) => fact ? [collaborationStatus(fact, t)] : []).filter(Boolean);
  if (labels.length === 0) return null;
  return <span className="conversation-v1-collaboration-status" data-collaboration-attached-status>{labels.join(" · ")}</span>;
}

/** Renders collaboration facts with the shared tool-card layout and measured folding.
 * 使用共享工具卡布局和测量折叠渲染协作事实。
 * Props retain exact event identity, host names and detail content; returns an initially folded card.
 * 属性保留精确事件身份、宿主名字及详情正文；返回默认收起的卡片。 */
function CollaborationToolCard(props: React.PropsWithChildren<{
  // Stable fold identity and persisted event coordinates.
  // 稳定折叠身份与持久事件坐标。
  id: string; seq: number; eventKind: "collaboration" | "subagent_return";
  // Distinct semantic card kind and optional collaboration subtype.
  // 独立语义卡片类型与可选协作子类型。
  cardKind: string; collaborationEventKind?: CollaborationEventKind;
  // Localized heading and state, plus the authoritative participant label when available.
  // 本地化标题和状态，以及可用时的权威参与者标签。
  title: string; status?: string | null; name?: string;
}>) {
  // Expansion follows the same session-scoped memory and viewport measurement as tool calls.
  // 展开状态遵循与工具调用一致的会话记忆与视口测量。
  const [expanded, setExpanded] = useConversationMeasuredExpansion(props.id, false);
  return (
    <article className={`tool-presentation-card subagent-orchestration conversation-v1-collaboration-card${expanded ? " expanded" : ""}${props.cardKind === "notification" ? " conversation-v1-collaboration-notification-card" : ""}`}
      data-event-kind={props.eventKind} data-collaboration-card-kind={props.cardKind}
      data-collaboration-event-kind={props.collaborationEventKind} data-seq={props.seq}>
      <button type="button" className="tool-presentation-summary subagent-orchestration-summary"
        aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>
        <MessagesSquare size={14} className="subagent-orchestration-icon" aria-hidden="true" />
        <span className="conversation-v1-collaboration-heading">
          <span className="conversation-v1-collaboration-title">{props.title}</span>
          {props.name ? <span className="conversation-v1-collaboration-name" title={props.name}>{props.name}</span> : null}
        </span>
        {props.status ? <span className="conversation-v1-collaboration-badge">{props.status}</span> : null}
        <ChevronDown size={14} className="tool-presentation-toggle" aria-hidden="true" />
      </button>
      <ConversationMeasuredCollapse collapseId={props.id} expanded={expanded}>
        <div className="subagent-orchestration-detail">{props.children}</div>
      </ConversationMeasuredCollapse>
    </article>
  );
}

/** Renders one source or target session navigation control without exposing its id in the label.
 * 渲染一条不在标签中暴露 ID 的来源或目标会话导航控件。 */
function CollaborationSessionButton(props: {
  // Authoritative session or agent labels; absent labels never expose internal IDs.
  // 权威会话或代理名称；缺少名称时绝不暴露内部身份。
  subagentNames?: ReadonlyMap<string, string>;
  label: string;
  title: string;
  sessionId: string;
  onOpenSession?: (sessionId: string) => void;
  availableSessionIds?: ReadonlySet<string>;
}) {
  const navigable = Boolean(props.onOpenSession)
    && (props.availableSessionIds === undefined || props.availableSessionIds.has(props.sessionId));
  // Names come from the exact host route, rather than being inferred from message text.
  // 名称来自精确宿主路由，不从消息正文推断。
  const name = props.subagentNames?.get(props.sessionId);
  return (
    <button
      aria-label={props.title}
      className="conversation-v1-collaboration-session-button"
      disabled={!navigable}
      onClick={() => props.onOpenSession?.(props.sessionId)}
      type="button"
    >
      <span className="conversation-v1-collaboration-caption">{props.label}</span>
      {name ? <span className="conversation-v1-collaboration-participant-name" title={name}>{name}</span> : null}
    </button>
  );
}

/** Extracts exact source and target locators from the typed after-state variants.
 * 从强类型后状态分支提取精确的来源和目标定位。 */
function collaborationParticipants(afterState: CollaborationAfterState): Array<{
  role: "source" | "target";
  sessionId: string;
}> {
  switch (afterState.kind) {
    case "received_task_result":
      return [
        { role: "source", sessionId: afterState.acceptance.result.acceptance.message.target.session_id },
        { role: "target", sessionId: afterState.acceptance.result.acceptance.message.source.session_id },
      ];
    case "task_result_acknowledgement":
    case "task_result_projection":
      return [];
    case "task_result":
      // The executor sends the result back to the original task requester.
      // 执行者将结果发回原任务请求者。
      return [
        { role: "source", sessionId: afterState.result.acceptance.message.target.session_id },
        { role: "target", sessionId: afterState.result.acceptance.message.source.session_id },
      ];
    case "outbound_task_message":
      return [
        { role: "source", sessionId: afterState.record.message.source.session_id },
        { role: "target", sessionId: afterState.record.message.target.session_id },
      ];
    case "inbound_task_message":
      return [
        { role: "source", sessionId: afterState.acceptance.message.source.session_id },
        { role: "target", sessionId: afterState.acceptance.message.target.session_id },
      ];
    case "task_message_effect":
      // This receipt contains no participant snapshot; do not guess one from delivery identity.
      // 此回执不含参与者快照；不从投递身份猜测参与者。
      return [];
    case "question":
      return [
        { role: "source", sessionId: afterState.question.source.session_id },
        { role: "target", sessionId: afterState.question.target.session_id },
      ];
    case "outbound_delivery":
      return [
        { role: "source", sessionId: afterState.delivery.source.session_id },
        { role: "target", sessionId: afterState.delivery.target.session_id },
      ];
    case "inbound_delivery":
      return [
        { role: "source", sessionId: afterState.delivery.source.session_id },
        { role: "target", sessionId: afterState.delivery.target.session_id },
      ];
    case "outbound_notification":
      return [
        { role: "source", sessionId: afterState.notification.source.session_id },
        { role: "target", sessionId: afterState.notification.target.session_id },
      ];
    case "inbound_notification":
      return [
        { role: "source", sessionId: afterState.notification.source.session_id },
        { role: "target", sessionId: afterState.notification.target.session_id },
      ];
    case "child_task":
      return [{ role: afterState.task.non_success_result === null ? "target" : "source", sessionId: afterState.task.child.session_id }];
    case "question_execution":
      return [{ role: "target", sessionId: afterState.execution.participant.session_id }];
    case "grant":
      return [
        { role: "source", sessionId: afterState.grant.source_session_id },
        { role: "target", sessionId: afterState.grant.target_session_id },
      ];
    case "answer_delivery":
    case "run_control":
    case "generation_ownership_cleared":
      return [];
  }
}

/** Returns the exact persisted body that is safe to show in one collaboration card.
 * 返回协作卡片可直接展示的精确持久化正文。 */
function collaborationBody(afterState: CollaborationAfterState): string | null {
  switch (afterState.kind) {
    case "received_task_result":
      return afterState.acceptance.disclosure === "authorization_revoked"
        ? null
        : afterState.acceptance.result.terminal?.final_assistant?.source_text.text ?? null;
    case "task_result_acknowledgement":
    case "task_result_projection":
      return null;
    case "task_result":
      return afterState.result.terminal?.final_assistant?.source_text.text ?? null;
    case "outbound_task_message":
      return afterState.record.message.body;
    case "inbound_task_message":
      return afterState.acceptance.message.body;
    case "task_message_effect":
      return null;
    case "question":
      return afterState.question.body;
    case "outbound_notification":
    case "inbound_notification":
      return afterState.notification.body;
    case "answer_delivery":
      return null;
    case "child_task":
      return afterState.task.non_success_result === null ? null
        : afterState.task.non_success_result.proof.failure_notes.map((note) =>
          note.redacted_detail === null ? note.reason_code : `${note.reason_code}: ${note.redacted_detail}`).join("\n\n");
    case "outbound_delivery":
    case "inbound_delivery":
    case "question_execution":
    case "grant":
    case "run_control":
    case "generation_ownership_cleared":
      return null;
  }
}

/** Returns the exact answer or cancellation body for a settled question.
 * 返回已结算问题的精确回答或撤销正文。 */
function collaborationSecondaryBody(
  afterState: CollaborationAfterState,
): { label: TranslationKey; body: string } | null {
  if (afterState.kind !== "question") return null;
  if (afterState.question.answer) {
    return {
      label: "conversation.collaboration.detail.answer",
      body: afterState.question.answer.body,
    };
  }
  if (afterState.question.cancellation) {
    return {
      label: "conversation.collaboration.detail.cancellationReason",
      body: afterState.question.cancellation.reason,
    };
  }
  return null;
}

/** Maps one typed event kind to localized timeline copy.
 * 将一种强类型事件类型映射为本地化时间线文案。 */
function collaborationTimelineTitle(kind: CollaborationEventKind, t: (key: TranslationKey) => string): string {
  switch (kind) {
    case "task_result_recorded":
    case "task_result_accepted":
      return t("conversation.collaboration.timeline.taskResult");
    case "task_result_acknowledged":
      return t("conversation.collaboration.timeline.taskResultAcknowledged");
    case "task_result_projected":
      return t("conversation.collaboration.timeline.taskResultProjected");
    case "task_message_sent":
    case "task_message_accepted":
    case "task_message_acknowledged":
    case "task_message_effect":
      return t("conversation.collaboration.timeline.taskMessage");
    case "question_created":
    case "question_cancelled":
      return t("conversation.collaboration.timeline.question");
    case "question_answered":
    case "answer_delivered":
    case "answer_delivery_failed":
      return t("conversation.collaboration.timeline.answer");
    case "delivery_recorded":
    case "delivery_accepted":
    case "delivery_projected":
    case "delivery_failed":
    case "delivery_settled":
    case "delivery_settlement_acknowledged":
      return t("conversation.collaboration.timeline.delivery");
    case "notification_sent":
    case "notification_progress":
    case "notification_accepted":
    case "notification_projected":
    case "notification_failed":
      return t("conversation.collaboration.timeline.notification");
    case "child_task_reserved":
    case "child_task_activated":
    case "child_task_terminal":
    case "child_task_non_success_recorded":
    case "child_task_non_success_projected":
      return t("conversation.collaboration.timeline.task");
    case "question_execution_started":
    case "question_execution_completed":
      return t("conversation.collaboration.timeline.execution");
    case "question_successor_scheduled":
      return t("conversation.collaboration.timeline.queuedSuccessor");
    case "grant_added":
    case "grant_removed":
      return t("conversation.collaboration.timeline.grant");
    case "run_control_changed":
      return t("conversation.collaboration.timeline.runControl");
    case "generation_ownership_cleared":
      return t("conversation.collaboration.timeline.historyReset");
  }
}

/** Maps one exact collaboration state to localized user-facing status copy.
 * 将一种精确协作状态映射为用户可见的本地化状态文案。 */
function collaborationStatus(
  afterState: CollaborationAfterState,
  t: (key: TranslationKey) => string,
): string | null {
  switch (afterState.kind) {
    case "task_result":
      return afterState.result.terminal === null
        ? t("conversation.collaboration.status.failed")
        : collaborationTaskOutcomeLabel(afterState.result.terminal.outcome, t);
    case "received_task_result":
      return afterState.acceptance.disclosure === "authorization_revoked"
        ? t("conversation.collaboration.status.resultUnavailable")
        : afterState.acceptance.result.terminal === null
        ? t("conversation.collaboration.status.failed")
        : collaborationTaskOutcomeLabel(afterState.acceptance.result.terminal.outcome, t);
    case "task_result_acknowledgement":
      return t("conversation.collaboration.status.delivered");
    case "task_result_projection":
      return afterState.projection.disclosure === "authorization_revoked"
        ? t("conversation.collaboration.status.resultUnavailable")
        : t("conversation.collaboration.status.projected");
    case "outbound_task_message":
      return collaborationDeliveryStateLabel(afterState.record.acceptance === null ? "recorded" : "accepted", t);
    case "inbound_task_message":
      return collaborationDeliveryStateLabel("accepted", t);
    case "task_message_effect":
      return afterState.effect.outcome.kind === "claimed"
        ? t("conversation.collaboration.status.taskClaimed")
        : collaborationDeliveryStateLabel(afterState.effect.outcome.kind === "steered" ? "projected" : "failed", t);
    case "question":
      return collaborationQuestionStateLabel(afterState.question.state, t);
    case "outbound_delivery":
      return collaborationDeliveryStateLabel(afterState.delivery.state, t);
    case "inbound_delivery":
      return collaborationDeliveryStateLabel(afterState.delivery.state, t)
        + (afterState.delivery.settlement ? ` · ${collaborationSettlementLabel(afterState.delivery.settlement.state, t)}` : "");
    case "outbound_notification":
    case "inbound_notification":
      return collaborationNotificationStateLabel(afterState.notification.state, t);
    case "answer_delivery":
      return collaborationAnswerDeliveryStateLabel(afterState.delivery.state, t);
    case "child_task":
      return collaborationChildTaskStateLabel(afterState.task.state, t)
        + (afterState.task.terminal_outcome ? ` · ${collaborationChildTaskOutcomeLabel(afterState.task.terminal_outcome, t)}` : "");
    case "question_execution":
      return collaborationExecutionStateLabel(afterState.execution.state, afterState.execution.outcome, t);
    case "grant":
      return afterState.grant.revoked_at_ms === undefined
        ? t("conversation.collaboration.status.granted")
        : t("conversation.collaboration.status.revoked");
    case "run_control":
      return collaborationRunControlLabel(afterState.transition.state, t);
    case "generation_ownership_cleared":
      return t("conversation.collaboration.status.cleared");
  }
}

/** Maps exact business closure outcomes to display labels; abandoned work is not successful.
 * 将精确业务闭合结果映射为显示标签；放弃的任务不表示成功。
 * Parameters: outcome is the host terminal state and t translates; returns the outcome label.
 * 参数：outcome 是宿主终态，t 执行翻译；返回结果标签。 */
function collaborationTaskOutcomeLabel(outcome: CollaborationTaskTerminal["outcome"], t: (key: TranslationKey) => string): string {
  switch (outcome) {
    case "completed": return t("conversation.collaboration.status.completed");
    case "stopped": return t("conversation.collaboration.status.stopped");
    case "abandoned": return t("conversation.collaboration.status.abandoned");
  }
}

/** Maps question settlement state to localized status.
 * 将问题结算状态映射为本地化状态。 */
function collaborationQuestionStateLabel(state: CollaborationQuestionState, t: (key: TranslationKey) => string): string {
  switch (state) {
    case "pending": return t("conversation.collaboration.status.pending");
    case "answered": return t("conversation.collaboration.status.answered");
    case "cancelled": return t("conversation.collaboration.status.cancelled");
  }
}

/** Maps delivery state to localized status.
 * 将投递状态映射为本地化状态。 */
function collaborationDeliveryStateLabel(state: CollaborationDeliveryState, t: (key: TranslationKey) => string): string {
  switch (state) {
    case "recorded": return t("conversation.collaboration.status.recorded");
    case "accepted": return t("conversation.collaboration.status.accepted");
    case "projected": return t("conversation.collaboration.status.projected");
    case "failed": return t("conversation.collaboration.status.failed");
  }
}

/** Maps one notification lifecycle state to localized status copy.
 * 将通知生命周期状态映射为本地化状态文案。 */
function collaborationNotificationStateLabel(
  state: CollaborationNotificationState,
  t: (key: TranslationKey) => string,
): string {
  switch (state) {
    case "recorded": return t("conversation.collaboration.status.recorded");
    case "accepted": return t("conversation.collaboration.status.accepted");
    case "projected": return t("conversation.collaboration.status.projected");
    case "failed": return t("conversation.collaboration.status.failed");
  }
}

/** Maps answer delivery state to localized status.
 * 将答案交付状态映射为本地化状态。 */
function collaborationAnswerDeliveryStateLabel(state: CollaborationAnswerDeliveryState, t: (key: TranslationKey) => string): string {
  switch (state) {
    case "undelivered": return t("conversation.collaboration.status.undelivered");
    case "delivered": return t("conversation.collaboration.status.delivered");
    case "failed": return t("conversation.collaboration.status.failed");
  }
}

/** Maps child task lifecycle state to localized status.
 * 将子任务生命周期状态映射为本地化状态。 */
function collaborationChildTaskStateLabel(state: CollaborationChildTaskState, t: (key: TranslationKey) => string): string {
  switch (state) {
    case "reserved": return t("conversation.collaboration.status.reserved");
    case "active": return t("conversation.collaboration.status.active");
    case "terminal": return t("conversation.collaboration.status.terminal");
  }
}

/** Maps child task terminal outcome to localized status.
 * 将子任务终态结果映射为本地化状态。 */
function collaborationChildTaskOutcomeLabel(outcome: CollaborationChildTaskTerminalOutcome, t: (key: TranslationKey) => string): string {
  switch (outcome) {
    case "completed": return t("conversation.collaboration.status.completed");
    case "failed": return t("conversation.collaboration.status.failed");
    case "stopped": return t("conversation.collaboration.status.stopped");
    case "cancelled": return t("conversation.collaboration.status.cancelled");
  }
}

/** Maps question execution state and outcome to localized status.
 * 将问题执行状态与结果映射为本地化状态。 */
function collaborationExecutionStateLabel(
  state: CollaborationQuestionExecutionState,
  outcome: CollaborationQuestionExecutionOutcome | undefined,
  t: (key: TranslationKey) => string,
): string {
  if (outcome === "settled") return t("conversation.collaboration.status.settled");
  if (outcome === "failed") return t("conversation.collaboration.status.failed");
  if (outcome === "unanswered") return t("conversation.collaboration.status.unanswered");
  return state === "running"
    ? t("conversation.collaboration.status.running")
    : t("conversation.collaboration.status.completed");
}

/** Maps target settlement state to localized status.
 * 将目标结算状态映射为本地化状态。 */
function collaborationSettlementLabel(state: CollaborationTargetSettlementState, t: (key: TranslationKey) => string): string {
  return state === "answered"
    ? t("conversation.collaboration.status.answered")
    : t("conversation.collaboration.status.cancelled");
}

/** Maps run-control state to localized status.
 * 将运行控制状态映射为本地化状态。 */
function collaborationRunControlLabel(state: CollaborationRunControl, t: (key: TranslationKey) => string): string {
  switch (state) {
    case "running": return t("conversation.collaboration.status.running");
    case "naturally_idle": return t("conversation.collaboration.status.naturallyIdle");
    case "stopped": return t("conversation.collaboration.status.stopped");
    case "main_only": return t("conversation.collaboration.status.mainOnly");
    case "main_and_subagents": return t("conversation.collaboration.status.mainAndSubagents");
  }
}
