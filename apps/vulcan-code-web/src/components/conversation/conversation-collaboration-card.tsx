import * as React from "react";
import { ArrowUpRight, ChevronDown, MessagesSquare } from "lucide-react";
import { ConversationMeasuredCollapse, useConversationMeasuredExpansion } from "./conversation-measured-collapse";
import { USER_PROMPT_COLLAPSE_LINE_LIMIT } from "./conversation-bubble-limits";
import { useI18n, type TranslationKey } from "@/i18n";
import { ConversationCollaborationStateContext } from "./conversation-collaboration-state";
import { collaborationNotificationId, collaborationTaskMessageId } from "@/service/conversation-event-display-projection";
import { decodeAgentQuestionInput, type ConversationAgentQuestionInput } from "@/service/conversation-agent-question-input";
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
  if (interaction.event.kind === "user_input") {
    // Only the production projection's host-authored question mirror reaches this branch.
    // 仅生产投影中的宿主问题镜像进入此分支。
    const question = decodeAgentQuestionInput(interaction.event);
    return question ? <ConversationReceivedQuestion {...actions} question={question} seq={interaction.seq} createdAtMs={interaction.created_at_ms} /> : null;
  }
  if (interaction.event.kind === "collaboration") {
    return <ConversationCollaborationCard key={interaction.event.operation_id} {...actions} event={interaction.event} seq={interaction.seq} createdAtMs={interaction.created_at_ms} />;
  }
  return (
    <CollaborationToolCard id={`subagent-return:${interaction.event.child_session_id}:${interaction.seq}`} seq={interaction.seq}
      eventKind="subagent_return" cardKind="task-result" title={t("conversation.collaboration.agentCompleted")}
      participant={<CollaborationSessionButton {...actions} sessionId={interaction.event.child_session_id} title={t("conversation.collaboration.openSource")} />}>
      <div className="vk-collab-content"><MarkdownMessage content={interaction.event.content} eager /></div>
      <footer className="vk-collab-foot">
        <time dateTime={new Date(interaction.created_at_ms).toISOString()}>{new Date(interaction.created_at_ms).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })}</time>
      </footer>
    </CollaborationToolCard>
  );
}

/** Renders a received question body with the shared incoming bubble and exact delivery state.
 * 使用共享接收气泡及精确投递状态渲染收到的问题正文。
 * Props contain the decoded host question, event coordinates and host-owned navigation; returns one card.
 * 参数包含解码宿主问题、事件坐标和宿主导航；返回一张卡片。 */
function ConversationReceivedQuestion(props: ConversationAgentInteractionActions & {
  /** Original question and its immutable event coordinates.
   * 原问题及其不可变事件坐标。 */
  question: ConversationAgentQuestionInput; seq: number; createdAtMs: number;
}) {
  // Exact question identity attaches delivery updates without importing an answer into this body.
  // 精确问题身份关联投递更新，不将答案混入本正文。
  const { locale, t } = useI18n();
  const state = React.useContext(ConversationCollaborationStateContext);
  const delivery = state.deliveries.get(props.question.questionId);
  // Execution is local to the receiver and belongs on this bubble, never on an independent empty card.
  // 执行属于接收方，应显示在本气泡上，绝不生成独立空卡片。
  const execution = state.questionExecutions.get(props.question.questionId);
  // A recorded answer/cancellation takes precedence over an earlier running execution snapshot.
  // 已记录的回答或撤销优先于此前仍在运行的执行快照。
  const settled = delivery?.kind === "inbound_delivery" && delivery.delivery.settlement !== undefined;
  const status = [...new Set([
    delivery ? collaborationStatus(delivery, t) : null,
    execution && !settled ? collaborationStatus(execution, t) : null,
  ].filter(Boolean))].join(" · ");
  return <CollaborationToolCard id={`collaboration:received-question:${props.question.eventId}`} seq={props.seq}
    eventKind="user_input" cardKind="question" incoming title={t("conversation.collaboration.timeline.question")}
    status={status}
    participant={<CollaborationSessionButton {...props} sessionId={props.question.source} title={t("conversation.collaboration.openSource")} />}
    footer={<footer className="vk-prompt-foot vk-collab-time">
      <time dateTime={new Date(props.createdAtMs).toISOString()}>{new Date(props.createdAtMs).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })}</time>
    </footer>}>
    <div className="vk-collab-content"><MarkdownMessage content={props.question.body} eager /></div>
  </CollaborationToolCard>;
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
  // Notification IDs likewise bind later lifecycle snapshots to the first card.
  // 通知身份同样将后续生命周期快照绑定到首张卡片。
  const notificationId = collaborationNotificationId(event.after_state);
  const afterState = event.after_state.kind === "question"
    ? collaborationState.questions.get(event.after_state.question.question_id) ?? event.after_state
    : taskMessageId !== null ? collaborationState.taskMessages.get(taskMessageId) ?? event.after_state
      : notificationId !== null ? collaborationState.notifications.get(notificationId) ?? event.after_state : event.after_state;
  // The original event selects question versus reply; later snapshots update status, never change that identity.
  // 原始事件决定提问还是回复；后续快照只更新状态，绝不改变此身份。
  const isAnswer = event.event_kind === "question_answered" && afterState.kind === "question";
  const answer = isAnswer ? afterState.question.answer : undefined;
  const answerReceipt = isAnswer ? collaborationState.answerDeliveries.get(afterState.question.question_id) : undefined;
  const answerDelivery = answerReceipt && answer && answerReceipt.delivery.delivery_revision >= answer.delivery.delivery_revision
    ? answerReceipt.delivery : answer?.delivery;
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
  // Direction is declared by the stored receiver variant, never inferred from names or message text.
  // 方向由持久化的接收方分支声明，绝不通过名称或消息正文推断。
  const incoming = afterState.kind === "inbound_task_message" || afterState.kind === "received_task_result"
    || afterState.kind === "inbound_notification" || isAnswer;
  const taskMessageEffect = taskMessageId !== null ? collaborationState.taskMessageEffects.get(taskMessageId) : undefined;
  // Only result snapshots own result receipts; message cards keep their independent application state.
  // 只有结果快照承载结果回执，消息卡保留独立的应用状态。
  const taskResultId = afterState.kind === "task_result" ? afterState.result.effect.delivery_id
    : afterState.kind === "received_task_result" ? afterState.acceptance.result.effect.delivery_id : null;
  const resultProgress = taskResultId !== null ? collaborationState.taskResultProgress.get(taskResultId) : undefined;
  const resultProgressLabel = resultProgress?.kind === "task_result_projection"
    ? (resultProgress.projection.disclosure === "authorization_revoked"
      ? t("conversation.collaboration.status.resultUnavailable") : t("conversation.collaboration.status.resultInContext"))
    : resultProgress?.kind === "task_result_acknowledgement" || afterState.kind === "received_task_result"
      ? t("conversation.collaboration.status.resultDelivered") : null;
  const status = [...new Set([collaborationStatus(taskMessageEffect ?? afterState, t), resultProgressLabel,
    answerDelivery ? collaborationAnswerDeliveryStateLabel(answerDelivery.state, t) : null].filter(Boolean))].join(" · ");
  const title = isFailedTaskDelivery
    ? t("conversation.collaboration.timeline.taskMessage")
    : collaborationTimelineTitle(event.event_kind, t);
  // Directional sentences surround the linked name; unnamed participants retain the generic heading.
  // 方向句围绕可跳转的名称显示；尚无名称的参与者保留通用标题。
  const messageDirection = participants.some((participant) => props.subagentNames?.get(participant.sessionId))
    ? afterState.kind === "outbound_task_message" ? "sent" : afterState.kind === "inbound_task_message" ? "received"
      : afterState.kind === "task_result" && !isFailedTaskDelivery ? "reply" : null
    : null;
  const body = isAnswer ? answer?.body ?? null
    : event.event_kind === "child_task_non_success_projected" ? null : collaborationBody(afterState);
  // Cancellation diagnostics belong to their own event, never the original question or answer body.
  // 撤销诊断属于自身事件，绝不混入原问题或答案正文。
  const secondaryBody = event.event_kind === "question_cancelled" && afterState.kind === "question" && afterState.question.cancellation
    ? { label: "conversation.collaboration.detail.cancellationReason" as const, body: afterState.question.cancellation.reason }
    : collaborationSecondaryBody(afterState);
  return (
    <CollaborationToolCard
      id={taskMessageId !== null ? `collaboration:task-message:${taskMessageId}`
        : notificationId !== null ? `collaboration:notification:${notificationId}` : `collaboration:${event.operation_id}:${props.seq}`} seq={props.seq}
      eventKind="collaboration" collaborationEventKind={event.event_kind}
      cardKind={isTaskResult ? "task-result" : isNotification ? "notification" : isTaskMessage ? "task-message" : "interaction"}
      title={title} status={status} incoming={incoming}
      titlePrefix={messageDirection === "sent" ? t("conversation.collaboration.message.sentPrefix")
        : messageDirection === "reply" ? t("conversation.collaboration.message.replyPrefix")
        : messageDirection === "received" ? t("conversation.collaboration.message.receivedPrefix") : undefined}
      titleSuffix={messageDirection === "sent" ? t("conversation.collaboration.message.sentSuffix")
        : messageDirection === "reply" ? t("conversation.collaboration.message.replySuffix")
        : messageDirection === "received" ? t("conversation.collaboration.message.receivedSuffix") : undefined}
      participant={participants.map((participant) => (
        <CollaborationSessionButton {...props} key={`${participant.role}:${participant.sessionId}`}
          sessionId={participant.sessionId}
          title={isAnswer || participant.role === "source" ? t("conversation.collaboration.openSource") : t("conversation.collaboration.openTarget")} />
      ))}
      footer={<footer className={incoming ? "vk-prompt-foot vk-collab-time" : "vk-collab-foot"}>
        <time dateTime={new Date(props.createdAtMs).toISOString()}>
          {new Date(props.createdAtMs).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })}
        </time>
      </footer>}
    >
      {body ? <div className="vk-collab-content"><MarkdownMessage content={body} eager /></div> : null}
      {secondaryBody ? (
        <section className="vk-collab-reply">
          <span className="vk-collab-caption">{t(secondaryBody.label)}</span>
          <MarkdownMessage content={secondaryBody.body} eager />
        </section>
      ) : null}
      {afterState.kind === "question" && !isAnswer ? <ConversationCollaborationStatus questionId={afterState.question.question_id} /> : null}
    </CollaborationToolCard>
  );
}

/** Renders delivery and lifecycle status inside its owning tool card, never as a timeline entry.
 * 在所属工具卡内部渲染投递与生命周期状态，绝不成为独立时间线条目。
 * Exact question, execution, task-message delivery or notification identities select current snapshots;
 * absent facts render nothing.
 * 精确的问题、执行、任务消息投递或通知身份选择当前快照；没有事实时不渲染。 */
export function ConversationCollaborationStatus(props: {
  questionId?: string | null;
  executionId?: string | null;
  deliveryId?: string | null;
  notificationId?: string | null;
}) {
  // Context belongs to the mounted session, so identical foreign IDs cannot leak across panes.
  // 上下文归已挂载会话所有，避免其他面板的身份串入。
  const state = React.useContext(ConversationCollaborationStateContext);
  const { t } = useI18n();
  const question = props.questionId ? state.questions.get(props.questionId) : undefined;
  const delivery = props.questionId ? state.deliveries.get(props.questionId) : undefined;
  const task = props.executionId ? state.tasks.get(props.executionId) : undefined;
  // A task message shows how the target applied it once known, else its delivery.
  // 任务消息在已知时显示目标的应用结果，否则显示其投递状态。
  const taskMessage = props.deliveryId
    ? state.taskMessageEffects.get(props.deliveryId) ?? state.taskMessages.get(props.deliveryId) : undefined;
  const notification = props.notificationId ? state.notifications.get(props.notificationId) : undefined;
  const labels = [delivery, question, task, taskMessage, notification].flatMap((fact) => fact ? [collaborationStatus(fact, t)] : []).filter(Boolean);
  if (labels.length === 0) return null;
  return <span className="vk-collab-attached" data-collaboration-attached-status>{labels.join(" · ")}</span>;
}

/** Renders collaboration facts with the shared tool-card layout and measured folding.
 * 使用共享工具卡布局和测量折叠渲染协作事实。
 * Props retain event identity and host names; incoming bubbles show content immediately, other cards start folded.
 * 属性保留事件身份及宿主名字；收到的气泡直接显示正文，其他卡片默认收起。 */
function CollaborationToolCard(props: React.PropsWithChildren<{
  // Stable fold identity and persisted event coordinates.
  // 稳定折叠身份与持久事件坐标。
  id: string; seq: number; eventKind: "collaboration" | "subagent_return" | "user_input";
  // Distinct semantic card kind and optional collaboration subtype.
  // 独立语义卡片类型与可选协作子类型。
  cardKind: string; collaborationEventKind?: CollaborationEventKind;
  // Localized heading and state, plus independent navigation to the remote participant.
  // 本地化标题与状态，以及指向对端参与者的独立导航。
  title: string; status?: string | null; participant?: React.ReactNode;
  // Optional sentence parts place the navigable participant inside a directional heading.
  // 可选句子片段将可跳转参与者放入表示方向的标题。
  titlePrefix?: string; titleSuffix?: string;
  // Received session messages reuse the user bubble appearance without adopting the user role.
  // 收到的会话消息复用用户气泡外观，不采用用户角色。
  incoming?: boolean;
  // Timestamp stays visible even when excess body lines are collapsed.
  // 超出正文行收起时仍显示时间。
  footer?: React.ReactNode;
}>) {
  // Expansion follows the same session-scoped memory and viewport measurement as tool calls.
  // 展开状态遵循与工具调用一致的会话记忆与视口测量。
  const [expanded, setExpanded] = useConversationMeasuredExpansion(props.id, false);
  return (
    <article className="vk-collab" data-open={expanded || undefined} data-incoming={props.incoming || undefined}
      data-event-kind={props.eventKind} data-collaboration-card-kind={props.cardKind}
      data-collaboration-event-kind={props.collaborationEventKind} data-seq={props.seq}>
      <div className="vk-collab-head">
        <MessagesSquare size={14} className="vk-collab-icon" aria-hidden="true" />
        <span className="vk-collab-heading">
          <span className="vk-collab-title">{props.titlePrefix ?? props.title}</span>
          {props.participant}
          {props.titleSuffix ? <span className="vk-collab-title">{props.titleSuffix}</span> : null}
        </span>
        {props.status ? <span className="vk-collab-badge">{props.status}</span> : null}
        {!props.incoming ? <button type="button" className="vk-collab-toggle" aria-label={props.title}
          aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>
          <ChevronDown size={14} aria-hidden="true" />
        </button> : null}
      </div>
      {props.incoming ? <IncomingCollaborationBubble id={props.id} footer={props.footer}>{props.children}</IncomingCollaborationBubble> : (
        <ConversationMeasuredCollapse collapseId={props.id} expanded={expanded}>
          <div className="vk-collab-body">{props.children}{props.footer}</div>
        </ConversationMeasuredCollapse>
      )}
    </article>
  );
}

/** Renders one source or target session navigation control without exposing its id in the label.
 * 渲染一条不在标签中暴露 ID 的来源或目标会话导航控件。 */
function CollaborationSessionButton(props: {
  // Authoritative session or agent labels; absent labels never expose internal IDs.
  // 权威会话或代理名称；缺少名称时绝不暴露内部身份。
  subagentNames?: ReadonlyMap<string, string>;
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
  // Unknown names do not create empty navigation controls or expose internal identifiers.
  // 未知名称不创建空导航控件，也不暴露内部标识。
  if (!name) return null;
  return (
    <button
      aria-label={props.title}
      className="vk-collab-who"
      disabled={!navigable}
      onClick={() => props.onOpenSession?.(props.sessionId)}
      type="button"
    >
      <span aria-hidden="true">[</span><ArrowUpRight size={12} aria-hidden="true" />
      <span className="vk-collab-name" title={name}>{name}</span>
      <span aria-hidden="true">]</span>
    </button>
  );
}

/** Selects only the remote participant from each owner-specific persisted variant.
 * 从各个具有明确所有者的持久化分支中仅选择对端参与者。
 * Receivers expose the sender; senders expose the recipient. No label or message parsing is used.
 * 接收方展示发送者，发送方展示接收者；不解析标签或消息来推测。 */
function collaborationParticipants(afterState: CollaborationAfterState): Array<{
  role: "source" | "target";
  sessionId: string;
}> {
  switch (afterState.kind) {
    case "received_task_result":
      return [
        { role: "source", sessionId: afterState.acceptance.result.acceptance.message.target.session_id },
      ];
    case "task_result_acknowledgement":
    case "task_result_projection":
      return [];
    case "task_result":
      // The executor sends the result back to the original task requester.
      // 执行者将结果发回原任务请求者。
      return [
        { role: "target", sessionId: afterState.result.acceptance.message.source.session_id },
      ];
    case "outbound_task_message":
      return [
        { role: "target", sessionId: afterState.record.message.target.session_id },
      ];
    case "inbound_task_message":
      return [
        { role: "source", sessionId: afterState.acceptance.message.source.session_id },
      ];
    case "task_message_effect":
      // This receipt contains no participant snapshot; do not guess one from delivery identity.
      // 此回执不含参与者快照；不从投递身份猜测参与者。
      return [];
    case "question":
      return [
        { role: "target", sessionId: afterState.question.target.session_id },
      ];
    case "outbound_delivery":
      return [{ role: "target", sessionId: afterState.delivery.target.session_id }];
    case "inbound_delivery":
      return [
        { role: "source", sessionId: afterState.delivery.source.session_id },
      ];
    case "outbound_notification":
      return [{ role: "target", sessionId: afterState.notification.target.session_id }];
    case "inbound_notification":
      return [
        { role: "source", sessionId: afterState.notification.source.session_id },
      ];
    case "child_task":
      return [{ role: afterState.task.non_success_result === null ? "target" : "source", sessionId: afterState.task.child.session_id }];
    case "question_execution":
      return [{ role: "target", sessionId: afterState.execution.participant.session_id }];
    case "grant":
      return [
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

/** Returns answer details or the original task reference, respecting result disclosure.
 * 返回回答详情或原任务引用，并遵守结果披露约束。 */
function collaborationSecondaryBody(
  afterState: CollaborationAfterState,
): { label: TranslationKey; body: string } | null {
  // Results retain their request context; revoked receipts cannot disclose either body.
  // 结果保留原请求语境；撤权回执不得披露任何正文。
  if (afterState.kind === "received_task_result") {
    return afterState.acceptance.disclosure === "authorization_revoked" ? null : {
      label: "conversation.collaboration.detail.originalTask",
      body: afterState.acceptance.result.acceptance.message.body,
    };
  }
  if (afterState.kind === "task_result") {
    return {
      label: "conversation.collaboration.detail.originalTask",
      body: afterState.result.acceptance.message.body,
    };
  }
  // Question and answer bodies have independent cards; neither duplicates the other's text.
  // 问题与答案正文各有独立卡片，互不重复对方文本。
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

/** Shows received content immediately and folds only lines beyond the shared prompt preview limit.
 * 立即显示收到的正文，仅折叠超出共享提示词预览限制的行。
 * Props contain stable delivery identity, rendered body and timestamp; returns a right-aligned bubble.
 * 属性包含稳定投递身份、已渲染正文与时间；返回右对齐气泡。 */
function IncomingCollaborationBubble(props: React.PropsWithChildren<{ id: string; footer?: React.ReactNode }>) {
  const { t } = useI18n();
  // Measure natural content height, not the clipped wrapper, so expanding cannot change overflow detection.
  // 测量正文自然高度而非裁切容器，避免展开操作改变溢出判定。
  const contentRef = React.useRef<HTMLDivElement>(null);
  const [previewHeight, setPreviewHeight] = React.useState(0);
  const [overflowing, setOverflowing] = React.useState(false);
  // Body expansion has its own identity; the former whole-card fold state must not hide short content.
  // 正文展开使用独立身份；此前整卡折叠状态不得隐藏短正文。
  const bodyId = `collaboration-bubble-body:${props.id}`;
  const [expanded, setExpanded] = useConversationMeasuredExpansion(bodyId, false);
  React.useLayoutEffect(() => {
    const element = contentRef.current;
    if (!element) return;
    /** Recomputes the line budget after layout, resizing or Markdown media load; returns nothing.
     * 布局、缩放或 Markdown 媒体加载后重新计算行数预算；无返回值。 */
    const measure = () => {
      const lineHeight = Number.parseFloat(window.getComputedStyle(element).lineHeight);
      const height = lineHeight * USER_PROMPT_COLLAPSE_LINE_LIMIT;
      setPreviewHeight(height);
      setOverflowing(element.scrollHeight > height + 1);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [props.children]);
  return <><div className="vk-prompt vk-collab-bubble">
    <ConversationMeasuredCollapse collapseId={bodyId} expanded={!overflowing || expanded} collapsedHeight={previewHeight}>
      <div className="vk-collab-bubble-body" ref={contentRef}>{props.children}</div>
    </ConversationMeasuredCollapse>
    {overflowing ? <button type="button" className="vk-collab-more"
      aria-label={expanded ? t("conversation.userPrompt.collapse") : t("conversation.userPrompt.expand")}
      aria-expanded={expanded} onClick={() => setExpanded(!expanded)}><ChevronDown size={14} aria-hidden="true" /></button> : null}
  </div>{props.footer}</>;
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
