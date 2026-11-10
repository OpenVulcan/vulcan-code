import type { TranslationKey } from "@/i18n-messages";
import { averageTurnOutputSpeed } from "./conversation-usage-display";
import type {
  ComposerAttachmentSummary,
  ConversationMessage,
  ProviderSummary,
  ToolRequestProgressSnapshot,
} from "@/types";
import type {
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";
import {
  createConversationRenderItemsProjector,
  type ConversationRenderItemsProjector,
} from "@/components/conversation/conversation-transcript";
import type {
  CollaborationAfterState,
  ConversationEventBody,
  ConversationEventSummary,
  ResourceId,
  StatBucket,
} from "@/service/conversation-event-window-store";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";
import {
  collaborationEventIsUserVisible, collaborationNotificationId, collaborationTaskMessageId, requestConfigChangeKind,
} from "@/service/conversation-event-display-projection";
import { projectCollaborationTurnLayout } from "@/service/conversation-collaboration-turn-layout";
import { decodeAgentQuestionInput, decodeAgentTaskInput } from "@/service/conversation-agent-question-input";
import { decodeShellJobNotice } from "./shell-job-notice";
import { projectTurnUsageCoverage, type TurnUsageCoverage } from "@/service/conversation-usage-coverage";
import { turnChangeSummariesV1, type TurnChangeSummaryV1 } from "@/diff/turn-change-summary-v1";
import { modelSwitchRouteLabel } from "./conversation-model-switch";
import { goalNoticeTitle } from "./conversation-goal-notice";
import { dedicatedMemoryNoticeTitle, isDedicatedMemoryNoticeKind } from "@/components/conversation/conversation-dedicated-notice";
import { parseCompactionProviderError, parseSystemProviderError } from "./conversation-system-error-card";
import { projectProviderRetryNotices, type RetryFailureAttempt } from "./conversation-provider-retry-notices";
import { nativeMediaInputPreview } from "./tool-presentations/native-media-input-preview";
import {
  contextInjectionCategoryLabel,
  contextInjectionEntryMarkdown,
  contextInjectionOperationLabel,
  contextInjectionPages,
  readContextInjectionEntries,
  shellJobNoticeLabel,
  type ContextInjectionCategory,
  type ContextInjectionEntry,
} from "./conversation-context-injection";

// V1RootInputPresentationMetadata stores the exact generation anchor used by restored edit and fork actions.
// V1RootInputPresentationMetadata 保存恢复后的编辑与分叉操作使用的精确 generation 锚点。
export interface V1RootInputPresentationMetadata {
  // Canonical root-input seq.
  // 规范根输入 seq。
  seq: number;
  // Canonical root-input text.
  // 规范根输入正文。
  text: string;
}

// ConversationBranchPresentationV1 is the complete in-memory input for the restored branch workspace.
// ConversationBranchPresentationV1 是恢复后的分支 workspace 使用的完整纯内存输入。
export interface ConversationBranchPresentationV1 {
  // Synthetic display records projected only from typed V1 events.
  // 仅从强类型 V1 事件投影得到的合成展示记录。
  messages: ConversationMessage[];
  // Restored branch render items built from the synthetic display records.
  // 从合成展示记录构建的恢复后分支渲染项。
  renderItems: ConversationRenderItem[];
}

// ApiStatsBody narrows the terminal request event used by error and duration projection.
// ApiStatsBody 收窄错误与耗时投影使用的终态请求事件。
type ApiStatsBody = Extract<ConversationEventBody, { kind: "api_stats" }>;

// ToolCallBody narrows one canonical V1 tool event.
// ToolCallBody 收窄一条规范 V1 工具事件。
type ToolCallBody = Extract<ConversationEventBody, { kind: "tool_call" }>;

// SystemContextBody narrows one canonical V1 system prompt injection event.
// SystemContextBody 收窄一条规范 V1 系统提示词注入事件。
type SystemContextBody = Extract<ConversationEventBody, { kind: "system_context" }>;

// ContextExpirationCategory describes a persistent snapshot invalidation with or without a known memory source.
// ContextExpirationCategory 区分记忆来源已确认和未确认的持久快照失效。
type ContextExpirationCategory = "memoryExpiration" | "injection";

// UserInputBody narrows one canonical V1 user input event.
// UserInputBody 收窄一条规范 V1 用户输入事件。
type UserInputBody = Extract<ConversationEventBody, { kind: "user_input" }>;

// Translation resolves one branch presentation key at render time.
// Translation 在渲染时解析一个分支展示文案键。
type Translation = (key: TranslationKey) => string;

// projectConversationBranchPresentationV1 adapts canonical V1 events to the restored branch presentation model.
// projectConversationBranchPresentationV1 将规范 V1 事件适配为恢复后的分支展示模型。
// Parameters: sessionId owns the event window, events are strictly ordered, resources resolve project ResourceIds, and t localizes branch summaries.
// 参数：sessionId 拥有事件窗口，events 严格有序，resources 解析项目级 ResourceId，t 本地化分支摘要。
// Returns: one pure in-memory presentation that never becomes a persistence or RPC authority.
// 返回值：一份绝不成为持久化或 RPC 权威的纯内存展示结果。
export function projectConversationBranchPresentationV1(
  sessionId: string,
  events: readonly ConversationEventSummary[],
  resources: ConversationSessionResourceCatalogV1,
  t: Translation,
  // Configured display catalog; isolated hosts may have no provider configuration.
  // 已配置的显示目录；隔离宿主可以不带供应商配置。
  providers: readonly ProviderSummary[] = [],
  // Transient seeds never become canonical events or executable inputs.
  // 瞬态种子绝不成为规范事件或可执行输入。
  requestDrafts: readonly ToolRequestProgressSnapshot[] = [],
): ConversationBranchPresentationV1 {
  return createConversationBranchPresentationProjectorV1()(sessionId, events, resources, t, providers, requestDrafts);
}

// ConversationBranchPresentationProjectorV1 projects one branch surface across successive event-window snapshots.
// ConversationBranchPresentationProjectorV1 在连续事件窗口快照之间投影单个分支界面。
export type ConversationBranchPresentationProjectorV1 = (
  sessionId: string,
  events: readonly ConversationEventSummary[],
  resources: ConversationSessionResourceCatalogV1,
  t: Translation,
  providers?: readonly ProviderSummary[],
  requestDrafts?: readonly ToolRequestProgressSnapshot[],
) => ConversationBranchPresentationV1;

// NO_PROVIDERS is the shared empty provider catalog, so omitting providers never invalidates the projector cache.
// NO_PROVIDERS 是共享的空供应商目录，使省略供应商参数不会使投影器缓存失效。
const NO_PROVIDERS: readonly ProviderSummary[] = [];

// NO_REQUEST_DRAFTS is the shared empty draft list for surfaces without a running request.
// NO_REQUEST_DRAFTS 是没有运行中请求的界面共享的空草稿列表。
export const NO_REQUEST_DRAFTS: readonly ToolRequestProgressSnapshot[] = [];

// BranchPresentationContextV1 is the set of inputs every cached projection depends on.
// BranchPresentationContextV1 是全部缓存投影共同依赖的输入集合。
interface BranchPresentationContextV1 {
  sessionId: string;
  resources: ConversationSessionResourceCatalogV1;
  t: Translation;
  providers: readonly ProviderSummary[];
}

// CachedEventMessagesV1 records the display messages of one immutable event and the cross-event facts they read.
// CachedEventMessagesV1 记录一条不可变事件的展示消息及其读取的跨事件事实。
interface CachedEventMessagesV1 {
  // Repeated retry failures can change when the live window appends another terminal result.
  // 实时窗口追加另一终态结果时，重复重试失败分页可能变化。
  retryFailures: readonly RetryFailureAttempt[] | undefined;
  // Canonical API purpose can arrive when an older page is prepended.
  // 前置更早分页时可能补齐规范 API 用途。
  compaction: boolean;
  // Loading the original injection can identify a previously generic expiration as a memory expiration.
  // 加载原注入事件后，可以把之前的通用失效提示识别为记忆失效提示。
  contextExpiration: ContextExpirationCategory | null;
  // Root input of the event Turn when projected.
  // 投影时该事件所属 Turn 的根输入。
  rootInputSeq: number | undefined;
  rootInputText: string | undefined;
  // Terminal API facts the event read when projected.
  // 投影时该事件读取的终态 API 事实。
  apiStats: ApiStatsBody | undefined;
  // Display messages derived from these inputs.
  // 由这些输入派生的展示消息。
  messages: ConversationMessage[];
}

// createConversationBranchPresentationProjectorV1 creates an incremental equivalent of the branch presentation.
// createConversationBranchPresentationProjectorV1 创建分支展示的增量等价实现。
// The first call, and any call after the session, resources, translator or providers change, projects every event;
// later calls reuse the messages of every event object the store kept unchanged and re-project only replaced or new
// events, so one streaming delta costs work proportional to what changed rather than to the whole conversation.
// 首次调用以及会话、资源、翻译函数或供应商变化后的调用投影全部事件；此后的调用复用 store 保持不变的每个
// 事件对象的消息，只重新投影被替换或新增的事件，使一次流式增量的开销与变化量成正比，而不是与整段会话成正比。
// Returns: a projector whose output always equals projectConversationBranchPresentationV1 for the same inputs.
// 返回值：对相同输入，其输出始终等于 projectConversationBranchPresentationV1 的投影器。
export function createConversationBranchPresentationProjectorV1(): ConversationBranchPresentationProjectorV1 {
  let context: BranchPresentationContextV1 | null = null;
  let eventMessages = new WeakMap<ConversationEventSummary, CachedEventMessagesV1>();
  let projectRenderItems: ConversationRenderItemsProjector = createConversationRenderItemsProjector();
  return (sessionId, events, resources, t, providers = NO_PROVIDERS, requestDrafts = NO_REQUEST_DRAFTS) => {
    if (
      context === null
      || context.sessionId !== sessionId
      || context.resources !== resources
      || context.t !== t
      || context.providers !== providers
    ) {
      context = { sessionId, resources, t, providers };
      eventMessages = new WeakMap();
      projectRenderItems = createConversationRenderItemsProjector(t);
    }
    const cache = eventMessages;
    // Retry visibility is reconstructed from the same event window for live updates and reopened sessions.
    // 实时更新与重新打开的会话都从同一事件窗口重建重试提示的可见性。
    const retryNotices = projectProviderRetryNotices(events);
    // usageCoverageBySeq retains known API contributions separately from strict persisted totals.
    // usageCoverageBySeq 将已知 API 贡献与严格持久化总量分开保留。
    const usageCoverageBySeq = projectTurnUsageCoverage(events);
    // turnChanges merges each Turn's applied ChangeSets into the statistics row of that Turn.
    // turnChanges 将每个 Turn 的已应用 ChangeSet 合并到该 Turn 的统计行。
    const turnChanges = turnChangeSummariesV1(events);
    // apiStatsByCall makes terminal request facts available without changing event display order.
    // apiStatsByCall 让终态请求事实可供读取，同时不改变事件显示顺序。
    const apiStatsByCall = new Map<number, ApiStatsBody>();
    // Compaction ownership comes from typed API facts, never from generated summary text.
    // 压缩归属来自强类型 API 事实，绝不根据生成的摘要正文猜测。
    const compactionCalls = new Set<number>();
    // rootInputByTurn binds final assistant actions to the root input of the same logical Turn.
    // rootInputByTurn 将最终回答操作绑定到同一逻辑 Turn 的根输入。
    const rootInputByTurn = new Map<number, V1RootInputPresentationMetadata>();
    // Only an actually loaded outbound question card can replace its successful ask-tool receipt.
    // 只有实际加载的发出提问卡，才能替代对应的成功提问工具回执。
    const visibleQuestionIds = new Set<string>();
    // Deliveries and notifications whose sending tool card is loaded show their status on that card.
    // 发送它们的工具卡已加载时，投递与通知在该卡上显示状态。
    const claimed: CollaborationReceiptClaims = { deliveryIds: new Set(), notificationIds: new Set() };
    // Results whose disclosure to the requester was revoked stay visible on the executing side.
    // 向请求方披露已被撤销的结果在执行侧保持可见。
    const revokedResultIds = new Set<string>();
    // Turns whose memory recall already has its own card do not repeat the injected memory text.
    // 已有记忆召回卡的回合不再重复显示注入的记忆正文。
    const memoryRecallTurns = new Set<number>();
    // Index VMM baseline events by their canonical sequence so invalidations can resolve their exact source.
    // 按规范序号索引 VMM 基线事件，使失效记录能定位其精确来源。
    const memoryBaselineSeqs = new Set<number>();
    for (const summary of events) {
      if (summary.event.kind === "memory_recall" && summary.turn_id !== null) memoryRecallTurns.add(summary.turn_id);
      if (summary.event.kind === "system_context" && isVmmMemoryBaseline(summary.event)) memoryBaselineSeqs.add(summary.seq);
      if (summary.event.kind === "tool_call") claimCollaborationReceipt(summary.event, claimed);
      if (summary.event.kind === "collaboration" && summary.event.after_state.kind === "task_result_projection"
        && summary.event.after_state.projection.disclosure === "authorization_revoked") {
        revokedResultIds.add(summary.event.after_state.projection.delivery_id);
      }
      if (summary.event.kind === "collaboration" && summary.event.event_kind === "question_created"
        && summary.event.after_state.kind === "question") {
        visibleQuestionIds.add(summary.event.after_state.question.question_id);
      }
      if ((summary.event.kind === "api_started" || summary.event.kind === "api_stats") && summary.event.purpose === "compaction") {
        compactionCalls.add(summary.event.api_call_id);
      }
      if (summary.event.kind === "api_stats") {
        apiStatsByCall.set(summary.event.api_call_id, summary.event);
      }
      if (
        summary.turn_id !== null
        && summary.event.kind === "user_input"
        && summary.event.input_source === "root"
      ) {
        rootInputByTurn.set(summary.turn_id, {
          seq: summary.seq,
          text: summary.event.text,
        });
      }
    }

    // Display copies bind collaboration to its real turn and put execution results before that turn's statistics.
    // 展示副本将协作绑定到真实回合，并把执行结果放到所属回合统计之前。
    const messages: ConversationMessage[] = [];
    // Source and target receipts update the first card for the exact same delivery.
    // 来源与目标回执更新同一精确投递的首张卡片。
    // Notification lifecycle snapshots likewise update the first card of the same notification.
    // 通知生命周期快照同样更新同一通知的首张卡片。
    const mergedIds = new Set<string>();
    for (const summary of projectCollaborationTurnLayout(events)) {
      // Skip recovered and merged failures before caching the display rows.
      // 在缓存展示行前跳过已恢复或已合并的失败。
      const retryFailures = retryNotices.get(summary.seq);
      if (retryFailures === null) continue;
      // Filter before consulting the event cache so pagination and live question arrival remain reversible.
      // 读取事件缓存前过滤，使分页及实时问题到达后的显示切换保持可逆。
      if (summary.event.kind === "tool_call" && questionCardReplacesToolReceipt(summary.event, visibleQuestionIds)) continue;
      if (summary.event.kind === "system_context" && contextInjectionIsHidden(summary, summary.event, memoryRecallTurns)) continue;
      if (summary.event.kind === "collaboration") {
        const state = summary.event.after_state;
        if (sendingCardShowsCollaboration(state, claimed)) continue;
        if (executingResultIsSettled(state, revokedResultIds)) continue;
        const deliveryId = collaborationTaskMessageId(state);
        const notificationId = collaborationNotificationId(state);
        const mergeId = deliveryId !== null ? `task-message:${deliveryId}`
          : notificationId !== null ? `notification:${notificationId}` : null;
        if (mergeId !== null) {
          if (mergedIds.has(mergeId)) continue;
          mergedIds.add(mergeId);
        }
      }
      const rootInput = summary.turn_id === null ? undefined : rootInputByTurn.get(summary.turn_id);
      // Only events with an explicit API identity can join its compaction card.
      // 只有带显式 API 身份的事件才能归入对应压缩卡。
      const compaction = "api_call_id" in summary.event && summary.event.api_call_id !== undefined
        && compactionCalls.has(summary.event.api_call_id);
      // Turn statistics read coverage aggregated across the whole Turn, so they are always re-projected.
      // Turn 统计读取整个 Turn 聚合的覆盖度，因此始终重新投影。
      if (summary.event.kind === "turn_stats") {
        messages.push(...conversationMessagesForV1Event(
          sessionId, summary, rootInput, apiStatsByCall, resources, providers, t, compaction, usageCoverageBySeq.get(summary.seq),
          summary.turn_id === null ? undefined : turnChanges.get(summary.turn_id),
        ));
        continue;
      }
      const apiStats = eventApiStatsDependency(summary.event, apiStatsByCall);
      // Resolve the category from the referenced source whenever it is present in the current window.
      // 当被引用的来源已加载到当前窗口时，按来源解析失效类别。
      const contextExpiration = summary.event.kind === "system_context"
        ? contextExpirationCategory(summary.event, memoryBaselineSeqs) : null;
      const cached = cache.get(summary);
      if (
        cached
        && cached.rootInputSeq === rootInput?.seq
        && cached.rootInputText === rootInput?.text
        && cached.apiStats === apiStats
        && cached.compaction === compaction
        && cached.contextExpiration === contextExpiration
        && sameRetryFailurePages(cached.retryFailures, retryFailures)
      ) {
        messages.push(...cached.messages);
        continue;
      }
      const projected = retryFailures === undefined ? conversationMessagesForV1Event(
        sessionId, summary, rootInput, apiStatsByCall, resources, providers, t, compaction, undefined, undefined, contextExpiration,
      ) : [retryFailureMessage(conversationMessageBase(sessionId, summary, rootInput), retryFailures)];
      cache.set(summary, { rootInputSeq: rootInput?.seq, rootInputText: rootInput?.text, apiStats, compaction, contextExpiration, retryFailures, messages: projected });
      messages.push(...projected);
    }

    // Bind drafts to the exact unfinished API and reject already handed-off or terminal identities.
    // 将草稿绑定到精确未结束 API，排除已交接或终态身份。
    const projectedMessages = appendToolRequestDraftMessages(
      sessionId, mergeConsecutiveContextInjections(messages, t), events, requestDrafts,
    );
    return {
      messages: projectedMessages,
      renderItems: projectRenderItems(projectedMessages),
    };
  };
}

// sameRetryFailurePages compares the cached and current page sources so unchanged cards keep their render identity.
// sameRetryFailurePages 比较缓存和当前分页的来源，使未变化的卡片保留渲染标识。
function sameRetryFailurePages(
  cached: readonly RetryFailureAttempt[] | undefined,
  current: readonly RetryFailureAttempt[] | undefined,
): boolean {
  return cached === current || (cached !== undefined && current !== undefined
    && cached.length === current.length && cached.every((page, index) => page.attempt === current[index].attempt
      && page.stats === current[index].stats && page.notice.event === current[index].notice.event));
}

// retryFailureMessage anchors a group at its first notice and keeps the original diagnostic for every page.
// retryFailureMessage 将一组失败锚定到首条提示，并保留每一页的原始诊断。
function retryFailureMessage(
  base: ReturnType<typeof conversationMessageBase>,
  failures: readonly RetryFailureAttempt[],
): ConversationMessage {
  // Pages carry the existing error-card body so reports and raw details follow the selected attempt.
  // 分页携带现有错误卡正文，使问题反馈和原始详情随所选尝试切换。
  const pages = failures.map(({ attempt, notice, stats }) => ({ attempt, body: providerErrorBody(notice.event, stats) }));
  return {
    ...base,
    role: "system",
    recordKind: "transcript",
    body: pages[pages.length - 1].body,
    metadata: { ...base.metadata, provider_retry_failure_pages: pages },
  };
}

/** Delivery and notification identities claimed by loaded sending tool cards.
 * 已加载的发送工具卡所认领的投递与通知身份。 */
interface CollaborationReceiptClaims {
  deliveryIds: Set<string>;
  notificationIds: Set<string>;
}

/** Records the delivery and notification identities one collaboration tool receipt carries.
 * 记录一条协作工具回执携带的投递与通知身份。 */
function claimCollaborationReceipt(event: ToolCallBody, claimed: CollaborationReceiptClaims): void {
  const result = toolHostResult(event);
  if (result?.kind !== "collaboration_receipt_v1") return;
  const payload = result.payload;
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return;
  const { delivery_id: deliveryId, notification_id: notificationId } = payload as Record<string, unknown>;
  if (typeof notificationId === "string") claimed.notificationIds.add(notificationId);
  else if (typeof deliveryId === "string") claimed.deliveryIds.add(deliveryId);
}

/** Returns whether a sent message or notification is already shown by its sending tool card.
 * 返回发出的消息或通知是否已由其发送工具卡展示。
 * Only the sender's own records qualify; received messages and results keep their own cards.
 * 只有发送方自身的记录符合条件；收到的消息与结果保留各自的卡片。 */
function sendingCardShowsCollaboration(state: CollaborationAfterState, claimed: CollaborationReceiptClaims): boolean {
  switch (state.kind) {
    case "outbound_task_message": return claimed.deliveryIds.has(state.record.message.delivery_id);
    case "task_message_effect": return claimed.deliveryIds.has(state.effect.delivery_id);
    case "outbound_notification": return claimed.notificationIds.has(state.notification.notification_id);
    default: return false;
  }
}

/** Returns whether the executing session's own result record needs no card: the task completed and its answer is
 * already shown directly above. Stopped, abandoned, undelivered or revoked results keep their card.
 * 返回执行会话自身的结果记录是否无需卡片：任务已完成且回答已显示在正上方。停止、放弃、未送达或已撤销的结果保留卡片。 */
function executingResultIsSettled(state: CollaborationAfterState, revokedResultIds: ReadonlySet<string>): boolean {
  return state.kind === "task_result" && state.result.terminal?.outcome === "completed"
    && !revokedResultIds.has(state.result.effect.delivery_id);
}

/** Returns whether a completed ask receipt already has its exact outbound question card in this window.
 * 返回当前窗口是否已有可替代成功提问回执的精确发出提问卡。
 * Inputs are canonical tool facts and visible question IDs; failed, active or unmatched calls stay visible.
 * 参数为规范工具事实与可见问题身份；失败、活动中或未匹配的调用保持显示。 */
function questionCardReplacesToolReceipt(event: ToolCallBody, visibleQuestionIds: ReadonlySet<string>): boolean {
  if (event.status !== "completed" || (event.tool_name !== "ask_question" && event.tool_name !== "ask_parent")) return false;
  // The native result_markdown producer keeps this join ID in collaboration_receipt_v1, never in display prose.
  // 原生 result_markdown 生产者在 collaboration_receipt_v1 中保留关联身份，绝不从展示正文提取。
  const result = toolHostResult(event);
  if (result?.kind !== "collaboration_receipt_v1") return false;
  const payload = result.payload;
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return false;
  const questionId = (payload as Record<string, unknown>).question_id;
  return typeof questionId === "string" && visibleQuestionIds.has(questionId);
}

// eventApiStatsDependency returns the terminal API facts one event display reads, if any.
// eventApiStatsDependency 返回单条事件展示所读取的终态 API 事实（如有）。
function eventApiStatsDependency(
  event: ConversationEventBody,
  apiStatsByCall: ReadonlyMap<number, ApiStatsBody>,
): ApiStatsBody | undefined {
  if (event.kind === "assistant_reasoning") return apiStatsByCall.get(event.api_call_id);
  if (event.kind === "display_note" && event.api_call_id !== undefined) return apiStatsByCall.get(event.api_call_id);
  return undefined;
}

// Adds safe formation rows to their exact active API; returns display copies without inventing ledger seqs.
// 为精确活动 API 添加安全构造行；返回展示副本，不伪造账本序号。
// Inputs are canonical messages/events and membership seeds; full arguments remain host-owned.
// 输入为规范消息、事件和成员种子；完整参数仍归宿主持有。
export function appendToolRequestDraftMessages(
  sessionId: string,
  messages: ConversationMessage[],
  events: readonly ConversationEventSummary[],
  drafts: readonly ToolRequestProgressSnapshot[],
): ConversationMessage[] {
  // Start events supply exact Turn/time ownership; terminal and prepared facts retire draft rows.
  // 开始事件提供精确回合及时间归属；终态和准备完成事实淘汰草稿行。
  const activeApis = new Map<number, ConversationEventSummary>();
  const prepared = new Set<string>();
  for (const summary of events) {
    if (summary.event.kind === "api_started") activeApis.set(summary.event.api_call_id, summary);
    if (summary.event.kind === "api_stats") activeApis.delete(summary.event.api_call_id);
    if (summary.event.kind === "tool_call") prepared.add(summary.event.request_stream_id);
  }
  // Only the current unfinished API can own formation; unknown owners wait for event hydration.
  // 仅当前未结束 API 可以持有构造；未知所有者等待事件水合。
  const pending = drafts.flatMap((draft): ConversationMessage[] => {
    const owner = activeApis.get(draft.apiCallId);
    if (!owner || prepared.has(draft.requestStreamId)) return [];
    return [{
      id: `tool-request:${draft.requestStreamId}:call`,
      conversationId: sessionId,
      createdAt: String(owner.created_at_ms),
      role: "assistant",
      recordKind: "tool-call-output",
      title: draft.toolName,
      body: "",
      metadata: {
        v1_turn_id: owner.turn_id,
        api_call_id: draft.apiCallId,
        tool_name: draft.toolName,
        tool_call_id: draft.toolCallId,
        request_stream_id: draft.requestStreamId,
        tool_display_state: draft.state,
        received_bytes: draft.receivedBytes,
        received_characters: draft.receivedCharacters,
        request_revision: draft.requestRevision,
        preview: draft.preview,
      },
    }];
  });
  return pending.length ? [...messages, ...pending] : messages;
}

// conversationMessagesForV1Event maps one event without consulting legacy messages or records.
// conversationMessagesForV1Event 映射一条事件，且不读取旧 messages 或 records。
function conversationMessagesForV1Event(
  sessionId: string,
  summary: ConversationEventSummary,
  rootInput: V1RootInputPresentationMetadata | undefined,
  apiStatsByCall: ReadonlyMap<number, ApiStatsBody>,
  resources: ConversationSessionResourceCatalogV1,
  providers: readonly ProviderSummary[],
  t: Translation,
  // Canonical API purpose resolved by the current event window.
  // 当前事件窗口解析出的规范 API 用途。
  compaction: boolean,
  // Optional canonical usage evidence for the current terminal event, never persisted message content.
  // 当前终态事件的可选规范用量证据，绝不是持久化消息正文。
  usageCoverage: TurnUsageCoverage | undefined,
  // Applied file changes of the Turn, read only by its statistics row.
  // 该 Turn 的已应用文件变更，仅由其统计行读取。
  turnChanges: TurnChangeSummaryV1 | undefined,
  // Display category of a snapshot invalidation, resolved from the loaded source event when available.
  // 快照失效的展示类别，来源事件已加载时按其解析。
  contextExpiration: ContextExpirationCategory | null = null,
): ConversationMessage[] {
  const base = conversationMessageBase(sessionId, summary, rootInput);
  if (compaction) {
    // Existing compaction rendering owns progress, reasoning, summary and terminal state as one card.
    // 既有压缩渲染器用一张卡持有进度、思考、摘要和终态。
    const message = compactionMessageForV1Event(base, summary, rootInput);
    if (message) return [message];
  }
  switch (summary.event.kind) {
    case "user_input":
      // The typed collaboration event owns the visible card; its provider mirror is not a prompt.
      // 强类型协作事件拥有可见卡片，其供应商镜像不是人类提示词。
      if (summary.event.input_source === "agent_message") {
        // A background command finish notice is host guidance the model received; it joins the context cards.
        // 后台命令结束通知是模型收到的宿主指引，归入上下文卡片。
        const shellJobNotice = decodeShellJobNotice(summary.event.text);
        if (shellJobNotice) {
          return [contextInjectionNotice({ ...base, metadata: presentationMetadata(summary, rootInput) }, [{
            context_key: SHELL_JOB_NOTICE_ENTRY_KEY,
            category: "shellJob",
            operation: "upsert",
            content: summary.event.text,
            label: shellJobNoticeLabel(shellJobNotice, t),
          }], t)];
        }
        // Received questions have no typed body in inbound_delivery; their exact host envelope owns the body.
        // 接收问题的 inbound_delivery 不含强类型正文；其精确宿主信封拥有正文。
        const question = decodeAgentQuestionInput(summary.event);
        return question?.target === sessionId ? [{ ...base, role: "system", recordKind: "agent-interaction",
          body: "", agentInteraction: { ...summary, event: summary.event } }] : [];
      }
      // A child root input carrying inbox task envelopes is the parent's message, not a human prompt.
      // 携带 inbox 任务信封的子会话根输入是父代理的消息，不是人类提示词。
      if (decodeAgentTaskInput(summary.event)?.target === sessionId) {
        return [{ ...base, role: "system", recordKind: "agent-interaction",
          body: "", agentInteraction: { ...summary, event: summary.event } }];
      }
      return [userInputMessage(base, summary.event, resources)];
    case "assistant_reasoning":
      return [{
        ...base,
        role: "assistant",
        recordKind: "thinking-output",
        title: "Thinking",
        body: summary.event.text,
        metadata: {
          ...presentationMetadata(summary, rootInput),
          api_call_id: summary.event.api_call_id,
          status: summary.event.status,
          stream_state: summary.event.status === "streaming" ? "active" : "sealed",
          duration_ms: apiDurationMs(apiStatsByCall.get(summary.event.api_call_id)),
        },
      }];
    case "assistant_text":
      return [{
        ...base,
        role: "assistant",
        body: summary.event.text,
        // Captures attach with the sealing revision, so only a streaming text lacks them.
        // 抓取结果随封口修订附着，因此只有流式正文缺少它们。
        ...(summary.event.status === "streaming" ? {} : { answerCodeSnapshots: summary.event.code_snapshots ?? [] }),
        metadata: {
          ...presentationMetadata(summary, rootInput),
          api_call_id: summary.event.api_call_id,
          status: summary.event.status,
          stream_state: summary.event.status === "streaming" ? "active" : "sealed",
        },
      }];
    case "tool_call":
      return toolMessages(base, summary, summary.event, rootInput, resources);
    case "turn_stats":
      return [{
        ...base,
        role: "assistant",
        recordKind: "turn-stats",
        title: "Turn statistics",
        body: "Turn statistics",
        metadata: {
          ...presentationMetadata(summary, rootInput),
          turn_stats: turnStatsMetadata(summary.event.buckets.total, summary.event.buckets.subagent, summary.event.duration_ms, summary.event.outcome, usageCoverage),
          ...(turnChanges ? { turn_changes: turnChanges } : {}),
        },
      }];
    case "display_note":
      return [displayNoteMessage(base, summary, summary.event, rootInput, apiStatsByCall, t)];
    case "memory_recall":
      return [memoryRecallMessage(base, summary, summary.event, rootInput, t)];
    case "system_context":
      return [contextInjectionMessage(base, summary, summary.event, rootInput, contextExpiration, t)];
    case "request_config_changed": {
      // changeKind is derived from exact persisted fields, never from a display label.
      // changeKind 从精确持久化字段派生，绝不依赖展示文案。
      const changeKind = requestConfigChangeKind(summary.event);
      if (changeKind === "none") {
        return [];
      }
      if (changeKind === "thinking") {
        return [{
          ...base,
          role: "assistant",
          recordKind: "display-note",
          title: "Reasoning effort",
          body: "Reasoning effort changed",
          metadata: {
            ...presentationMetadata(summary, rootInput),
            notice_kind: "reasoning_effort_switch",
            activation: "next_provider_request",
            previous_thinking_level: summary.event.previous.thinking_level,
            current_thinking_level: summary.event.current.thinking_level,
          },
        }];
      }
      return [{
        ...base,
        role: "assistant",
        recordKind: "display-note",
        title: "Model selection",
        body: "Model selection changed",
        metadata: {
          ...presentationMetadata(summary, rootInput),
          notice_kind: "model_switch",
          previous_model_label: modelSwitchRouteLabel(summary.event.previous, providers),
          current_model_label: modelSwitchRouteLabel(summary.event.current, providers),
        },
      }];
    }
    case "api_stats":
      return [];
    case "subagent_return":
    case "collaboration":
      // Share the timeline visibility rule without deleting durable control facts or hiding real agent messages.
      // 复用时间线可见性规则，不删除持久控制事实，也不隐藏真实代理消息。
      if (summary.event.kind === "collaboration" && !collaborationEventIsUserVisible(summary.event)) return [];
      return [{
        ...base,
        role: "system",
        recordKind: "agent-interaction",
        body: summary.event.kind === "subagent_return" ? summary.event.content : "",
        agentInteraction: { ...summary, event: summary.event },
      }];
    case "queued_input_resolution":
    case "api_started":
    case "turn_boundary":
    case "execution_control":
    case "context_maintenance":
      // Maintenance ownership remains in the event window; its API facts own the visible compaction card.
      // 维护归属保留在事件窗口中；其 API 事实拥有可见压缩卡片。
      return [];
  }
}

// Adapts typed compaction API events into the shared card; returns null for unrelated event kinds.
// 将强类型压缩 API 事件适配到共享卡片；无关事件类型返回 null。
// base retains exact event identity and summary supplies the canonical API lifecycle, without persisting display metadata.
// base 保留精确事件身份，summary 提供规范 API 生命周期，不持久化展示元数据。
function compactionMessageForV1Event(
  base: ReturnType<typeof conversationMessageBase>,
  summary: ConversationEventSummary,
  // Root identity is explicit because ConversationMessage.metadata is an opaque transport value.
  // 根输入身份显式传入，因为 ConversationMessage.metadata 是不透明传输值。
  rootInput: V1RootInputPresentationMetadata | undefined,
): ConversationMessage | null {
  // event is narrowed before reading fields owned by each lifecycle variant.
  // 先收窄 event，再读取各生命周期分支持有的字段。
  const event = summary.event;
  if (event.kind !== "api_started" && event.kind !== "api_stats"
    && event.kind !== "assistant_text" && event.kind !== "assistant_reasoning") return null;
  // role and state follow API lifecycle; completed text alone does not claim a completed API.
  // role 与 state 遵循 API 生命周期；正文完成本身不代表 API 已完成。
  const role = event.kind === "api_started" ? "start" : event.kind === "api_stats" ? "end"
    : event.kind === "assistant_reasoning" ? "thinking" : "body";
  const state = event.kind === "api_stats" ? (event.status === "completed" ? "completed" : "failed") : "running";
  return {
    ...base,
    role: "assistant",
    recordKind: event.kind === "assistant_reasoning" ? "thinking-output" : "compaction-status",
    body: event.kind === "assistant_text" || event.kind === "assistant_reasoning" ? event.text : "",
    metadata: {
      ...presentationMetadata(summary, rootInput),
      api_call_id: event.api_call_id,
      context_compaction_section_id: `v1-api-${event.api_call_id}`,
      context_compaction_role: role,
      context_compaction_state: state,
      stream_state: event.kind === "assistant_text" || event.kind === "assistant_reasoning"
        ? (event.status === "streaming" ? "active" : "sealed") : "sealed",
    },
  };
}

// conversationMessageBase creates the stable display identity shared by all projections of one event.
// conversationMessageBase 创建一条事件的所有投影共享的稳定展示身份。
function conversationMessageBase(
  sessionId: string,
  summary: ConversationEventSummary,
  rootInput: V1RootInputPresentationMetadata | undefined,
): Pick<ConversationMessage, "id" | "conversationId" | "createdAt"> & { metadata: Record<string, unknown> } {
  // displayedCreatedAtMs uses the durable last-update timestamp only for editable user-input facts.
  // displayedCreatedAtMs 仅对可编辑的用户输入事实使用持久化最后更新时间。
  const displayedCreatedAtMs = summary.event.kind === "user_input"
    ? summary.updated_at_ms
    : summary.created_at_ms;
  return {
    id: `v1:${summary.seq}`,
    conversationId: sessionId,
    createdAt: String(displayedCreatedAtMs),
    metadata: presentationMetadata(summary, rootInput),
  };
}

// presentationMetadata stores only explicit V1 identities required by restored interactions.
// presentationMetadata 仅保存恢复后交互所需的显式 V1 身份。
function presentationMetadata(
  summary: ConversationEventSummary,
  rootInput: V1RootInputPresentationMetadata | undefined,
): Record<string, unknown> {
  return {
    v1_event: true,
    v1_seq: summary.seq,
    v1_event_revision: summary.event_revision,
    v1_turn_id: summary.turn_id,
    ...(rootInput ? {
      v1_root_input_seq: rootInput.seq,
      v1_root_input_text: rootInput.text,
    } : {}),
  };
}

// userInputMessage projects one typed user input and its ordered project resources.
// userInputMessage 投影一条强类型用户输入及其有序项目资源。
function userInputMessage(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  event: UserInputBody,
  resources: ConversationSessionResourceCatalogV1,
): ConversationMessage {
  return {
    ...base,
    role: "user",
    body: event.text,
    editableUserPrompt: event.input_source === "root",
    attachments: (event.resource_ids ?? []).map((resourceIdValue) => (
      composerAttachmentForResource(resourceIdValue, resources)
    )),
    metadata: {
      ...(base.metadata as Record<string, unknown>),
      starts_user_turn: event.input_source !== "pending_in_turn",
      turn_process: event.input_source === "pending_in_turn",
      client_action_id: event.client_action_id,
      input_id: event.input_id,
      input_source: event.input_source,
      resource_ids: event.resource_ids ?? [],
    },
  };
}

// composerAttachmentForResource converts browser-safe V1 resource metadata into the restored attachment strip model.
// composerAttachmentForResource 将浏览器安全的 V1 资源元数据转换为恢复后的附件条模型。
export function composerAttachmentForResource(
  currentResourceId: ResourceId,
  resources: ConversationSessionResourceCatalogV1,
): ComposerAttachmentSummary {
  const resource = resources.resources.get(currentResourceId);
  if (!resource || resource.availability === "missing") {
    return {
      kind: "file",
      fileName: resource?.placeholder ?? "[MISSING PROJECT RESOURCE]",
      localPath: "",
      projectResourceId: currentResourceId,
      byteLen: null,
      mimeType: "application/octet-stream",
    };
  }
  return {
    kind: resourceKind(resource.mime),
    fileName: resource.original_name,
    localPath: resources.contentUrl(currentResourceId),
    projectResourceId: currentResourceId,
    byteLen: resource.size,
    mimeType: resource.mime ?? "application/octet-stream",
  };
}

// resourceKind maps one declared MIME type to the established attachment families.
// resourceKind 将一个声明的 MIME 类型映射为既有附件类别。
function resourceKind(mime: string | undefined): string {
  if (mime?.startsWith("image/")) return "image";
  if (mime?.startsWith("audio/")) return "audio";
  if (mime?.startsWith("video/")) return "video";
  if (mime === "application/pdf") return "pdf";
  return "file";
}

// toolMessages expands one same-seq V1 tool state into the adjacent call/result blocks expected by the branch card system.
// toolMessages 将一条同 seq 的 V1 工具状态展开为分支卡片系统需要的相邻调用/结果块。
function toolMessages(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  summary: ConversationEventSummary,
  event: ToolCallBody,
  rootInput: V1RootInputPresentationMetadata | undefined,
  resources: ConversationSessionResourceCatalogV1,
): ConversationMessage[] {
  // displayOutput is the exact model-facing result retained inside the durable tool envelope.
  // displayOutput 是可靠工具封装中保留的精确模型可见结果。
  const displayOutput = toolDisplayOutput(event);
  const terminal = event.status === "completed"
    || event.status === "failed"
    || event.status === "cancelled"
    || event.status === "stopped"
    || event.status === "interrupted";
  const metadata = toolMetadata(summary, event, rootInput, resources);
  const call: ConversationMessage = {
    ...base,
    id: `tool-request:${event.request_stream_id}:call`,
    role: "assistant",
    recordKind: "tool-call-output",
    title: event.tool_name,
    body: displayPayload(event.input),
    metadata,
  };
  if (!terminal) return [call];
  return [
    call,
    {
      ...base,
      id: `${base.id}:result`,
      role: "assistant",
      recordKind: "tool-result-output",
      title: event.tool_name,
      body: displayPayload(displayOutput),
      metadata,
    },
  ];
}

// toolMetadata normalizes typed V1 tool facts into the branch presentation contract without a detail lookup.
// toolMetadata 在不进行详情查询的前提下，把强类型 V1 工具事实规范化为分支展示契约。
function toolMetadata(
  summary: ConversationEventSummary,
  event: ToolCallBody,
  rootInput: V1RootInputPresentationMetadata | undefined,
  resources: ConversationSessionResourceCatalogV1,
): Record<string, unknown> {
  // hostResult preserves the typed producer-owned payload required by specialized branch cards.
  // hostResult 保留专用分支卡片所需的强类型生产者权威载荷。
  const hostResult = toolHostResult(event);
  // displayOutput is shared by the visible result body and generic metadata consumers.
  // displayOutput 由可见结果正文与通用元数据消费者共同使用。
  const displayOutput = toolDisplayOutput(event);
  const terminalRef = event.external_refs?.find((reference) => reference.kind === "terminal");
  const subagentRef = event.external_refs?.find((reference) => reference.kind === "subagent");
  return {
    ...presentationMetadata(summary, rootInput),
    v1_tool_event: event,
    tool_name: event.tool_name,
    tool_call_id: String(event.tool_call_id),
    request_stream_id: event.request_stream_id,
    tool_display_state: toolDisplayState(event.status),
    arguments: event.input,
    result: {
      status: event.status,
      output: displayOutput,
    },
    ...(hostResult ? { host_result: hostResult } : {}),
    ...(terminalRef ? { command_session_id: terminalRef.id } : {}),
    ...(subagentRef ? { subagent_task_id: subagentRef.id } : {}),
    display: {
      outputs: generatedResourceOutputs(event, resources),
    },
  };
}

// toolOutputEnvelope narrows the durable success envelope without probing alternate field names.
// toolOutputEnvelope 收窄可靠成功封装，且不探测其他候选字段名。
function toolOutputEnvelope(event: ToolCallBody): Record<string, unknown> | null {
  return event.output && typeof event.output === "object" && !Array.isArray(event.output)
    ? event.output as Record<string, unknown>
    : null;
}

// toolDisplayOutput selects the exact model output or the strongly typed terminal failure document.
// toolDisplayOutput 选择精确模型输出或强类型工具终态失败文档。
function toolDisplayOutput(event: ToolCallBody): unknown {
  const output = toolOutputEnvelope(event);
  if (output && Object.prototype.hasOwnProperty.call(output, "model_output")) {
    return nativeMediaInputPreview(output.model_output) ?? output.model_output;
  }
  return event.output ?? event.changeset ?? { status: event.status };
}

// toolHostResult returns the canonical V1 ChangeSet or the exact runtime host result from the durable envelope.
// toolHostResult 返回规范 V1 ChangeSet，或可靠封装中的精确运行时宿主结果。
function toolHostResult(event: ToolCallBody): Record<string, unknown> | null {
  if (event.changeset) {
    return { kind: "change_set", payload: event.changeset };
  }
  const output = toolOutputEnvelope(event);
  const hostResult = output?.host_result;
  return hostResult && typeof hostResult === "object" && !Array.isArray(hostResult)
    ? hostResult as Record<string, unknown>
    : null;
}

// generatedResourceOutputs maps typed generated resources to authenticated opaque content URLs for the restored media cards.
// generatedResourceOutputs 将强类型生成资源映射为恢复后媒体卡使用的鉴权不透明内容 URL。
function generatedResourceOutputs(
  event: ToolCallBody,
  resources: ConversationSessionResourceCatalogV1,
): Record<string, unknown>[] {
  return (event.generated_resources ?? []).map((reference) => {
    const resource = resources.resources.get(reference.resource_id);
    return {
      kind: reference.kind === "pdf" || reference.kind === "file" ? "document" : reference.kind,
      file_name: resource?.availability === "available" ? resource.original_name : "[MISSING PROJECT RESOURCE]",
      local_path: resource?.availability === "available" ? resources.contentUrl(reference.resource_id) : "",
      mime_type: resource?.availability === "available" ? resource.mime : undefined,
      resource_id: reference.resource_id,
    };
  });
}

// toolDisplayState maps the V1 lifecycle to the branch card lifecycle without inferring from text.
// toolDisplayState 在不从正文推断的情况下，把 V1 生命周期映射为分支卡片生命周期。
function toolDisplayState(status: ToolCallBody["status"]): string {
  if (status === "completed") return "completed";
  if (status === "failed") return "failed";
  if (status === "cancelled" || status === "stopped" || status === "interrupted") return "stopped";
  return "running";
}

// displayNoteMessage projects durable display facts through the restored notice and error-card suite.
// displayNoteMessage 通过恢复后的提示与错误卡套件投影可靠显示事实。
function displayNoteMessage(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  summary: ConversationEventSummary,
  event: Extract<ConversationEventBody, { kind: "display_note" }>,
  rootInput: V1RootInputPresentationMetadata | undefined,
  apiStatsByCall: ReadonlyMap<number, ApiStatsBody>,
  t: Translation,
): ConversationMessage {
  if (event.severity === "error") {
    // The notice's explicit API identity supplies the matching terminal facts when present.
    // 提示中的显式 API 标识在可用时提供匹配的终态事实。
    const apiStats = event.api_call_id === undefined ? undefined : apiStatsByCall.get(event.api_call_id);
    return {
      ...base,
      role: "system",
      recordKind: "transcript",
      body: providerErrorBody(event, apiStats),
      metadata: presentationMetadata(summary, rootInput),
    };
  }
  return {
    ...base,
    role: "assistant",
    recordKind: "display-note",
    title: event.notice_kind,
    body: displayNoteTitle(event, t),
    metadata: {
      ...presentationMetadata(summary, rootInput),
      notice_kind: event.notice_kind === "retry_started" || event.notice_kind === "retry_exhausted"
        ? "provider_retry"
        : event.notice_kind,
      provider_retry_state: event.notice_kind === "retry_started"
        ? "retrying"
        : event.notice_kind === "retry_exhausted" ? "failed" : undefined,
      retry_count: event.retry_count,
      retry_limit: event.retry_limit,
      reason_code: event.reason_code,
      summary: displayNoteTitle(event, t),
      status: event.severity,
    },
  };
}

// providerErrorBody formats a display note and its optional API facts for the shared diagnostic card.
// providerErrorBody 将展示提示及其可选 API 事实格式化为共享诊断卡正文。
function providerErrorBody(
  event: Extract<ConversationEventBody, { kind: "display_note" }>,
  apiStats: ApiStatsBody | undefined,
): string {
  // Preserve producer-owned structured errors and their exact dispatch, route and stage facts.
  // 保留生产端结构化错误及其精确的派发、路由与阶段事实。
  if (event.redacted_detail !== undefined && parseSystemProviderError(event.redacted_detail)) {
    return event.redacted_detail;
  }
  // The original wrapper stays in the expanded diagnostic while the card uses the parsed provider message.
  // 原始外壳保留在展开诊断中，卡片使用解析出的供应商消息。
  const compactionProviderError = (event.reason_code === "context_compaction_failed"
    || event.reason_code === "automatic_context_compaction_failed") && event.redacted_detail !== undefined
    ? parseCompactionProviderError(event.redacted_detail)
    : null;
  // Compaction carries its own structured provider identity; other notes read the matching terminal facts.
  // 压缩携带自身的结构化供应商标识，其他提示读取匹配的终态事实。
  const provider = compactionProviderError?.provider ?? apiStats?.provider ?? "unknown";
  // Pre-API host failures have no matching terminal error.
  // API 建立前的宿主失败没有对应的终态错误。
  const error = apiStats?.error;
  // The V1 balance category allows a user to Continue after replenishment, independently of automatic retry.
  // V1 余额类别允许用户充值后主动继续，与自动重试独立。
  const balanceInsufficient = error?.category === "balance";
  // Keep the expanded diagnostic compatible with ordinary single failures and paged retries.
  // 使展开诊断兼容普通单次失败及分页重试。
  const payload = compactionProviderError === null ? {
    category: balanceInsufficient ? "payment_required" : error?.category ?? event.notice_kind,
    stage: "terminal",
    // An absent API receipt is not evidence that an external request was sent.
    // 缺少 API 回执不能作为外部请求已经发出的证据。
    dispatch_outcome: apiStats === undefined ? "unknown" : apiStats.status === "prepared" ? "not_dispatched" : "dispatched",
    semantic_output_observed: false,
    side_effect_observed: false,
    terminal: true,
    retryable: !balanceInsufficient && apiStats?.continuation_eligibility === "allowed",
    upstream_code: error?.error_code ?? event.reason_code,
    upstream_message: event.redacted_detail ?? error?.message ?? "",
    provider_id: apiStats?.provider,
    model_id: apiStats?.model,
    message: event.redacted_detail ?? error?.message ?? event.reason_code,
  } : {
    ...compactionProviderError.structured,
    retryable: false,
    message: compactionProviderError.message,
    compaction_diagnostic: event.redacted_detail,
  };
  return `Provider streaming execution failed: provider ${provider} failure: ${JSON.stringify(payload)}`;
}

// memoryRecallMessage shows what long-term memory recall gave the turn as one work-process notice.
// memoryRecallMessage 把长期记忆召回为回合提供的内容显示为一条工作过程提示。
// The detail body repeats the queries and memory text exactly as the model received them.
// 详情正文原样列出检索词与模型收到的记忆正文。
function memoryRecallMessage(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  summary: ConversationEventSummary,
  event: Extract<ConversationEventBody, { kind: "memory_recall" }>,
  rootInput: V1RootInputPresentationMetadata | undefined,
  t: Translation,
): ConversationMessage {
  const memories = event.memories ?? [];
  const queries = event.queries ?? [];
  const headline = event.outcome === "matched"
    ? t("conversation.memoryRecall.matched").replace("{count}", String(memories.length))
    : event.outcome === "no_match" ? t("conversation.memoryRecall.noMatch") : t("conversation.memoryRecall.failed");
  const sections: string[] = [];
  if (event.outcome === "no_match") sections.push(t("conversation.memoryRecall.noMatchDetail"));
  if (event.outcome === "failed") sections.push(t("conversation.memoryRecall.failedDetail"));
  if (queries.length > 0) {
    sections.push(`**${t("conversation.memoryRecall.queries")}**\n\n${queries.map((query) => `- ${markdownCodeSpan(query)}`).join("\n")}`);
  }
  if (memories.length > 0) {
    sections.push(`**${t("conversation.memoryRecall.memories")}**`);
    for (const memory of memories) {
      sections.push(`#### ${t("conversation.memoryRecall.memory").replace("{id}", String(memory.memory_id))}\n\n${memory.text}`);
    }
  }
  return {
    ...base,
    role: "assistant",
    recordKind: "display-note",
    title: t("conversation.memoryRecall.title"),
    body: sections.join("\n\n"),
    metadata: {
      ...presentationMetadata(summary, rootInput),
      notice_kind: "memory_recall",
      summary: headline,
      status: event.outcome === "failed" ? "failed" : "completed",
    },
  };
}

// SHELL_JOB_NOTICE_ENTRY_KEY identifies card entries projected from finish notices rather than from system contexts.
// SHELL_JOB_NOTICE_ENTRY_KEY 标识由结束通知（而非系统上下文）投影出的卡片条目。
const SHELL_JOB_NOTICE_ENTRY_KEY = "shell_job_notice";

// contextInjectionCategory maps a backend context key to its card category; unknown keys stay visible as other.
// contextInjectionCategory 将后端上下文键映射为卡片类别；未知键仍以“其他”显示。
function contextInjectionCategory(contextKey: string): ContextInjectionCategory {
  switch (contextKey) {
    case "goal_state":
    case "goal_continuation":
      return "goal";
    case "shell_job_continuation":
      return "shellJob";
    case "plan_mode":
      return "plan";
    case "dynamic.provider_tools":
    case "dynamic.tool_guidance":
      return "tools";
    case "dynamic.permission_mode":
      return "permission";
    case "dynamic.model_selection":
      return "model";
    case "dynamic.html_answers":
      return "answer";
    case "dynamic.host_message_guidance":
      return "guidance";
    case "dynamic.system":
    case "dynamic.developer":
    case "dynamic.runtime_context":
      return "base";
    case "injection.persistent":
    case "injection.transient":
      return "injection";
  }
  if (contextKey.startsWith("continuation_guidance/")) return "guidance";
  return "other";
}

// contextInjectionIsHidden suppresses automatic turn-end expiration and memory text already shown by the recall card.
// contextInjectionIsHidden 隐藏回合结束时的自动失效和召回卡已展示的记忆正文。
function contextInjectionIsHidden(
  summary: ConversationEventSummary,
  event: SystemContextBody,
  memoryRecallTurns: ReadonlySet<number>,
): boolean {
  if (event.operation === "remove" && event.scope === "turn") return true;
  return event.context_key === "injection.transient"
    && summary.turn_id !== null
    && memoryRecallTurns.has(summary.turn_id);
}

// isVmmMemoryBaseline recognizes the exact baseline envelope emitted into the persistent injection snapshot.
// isVmmMemoryBaseline 识别写入持久注入快照的规范 VMM 基线信封。
function isVmmMemoryBaseline(event: SystemContextBody): boolean {
  return event.context_key === "injection.persistent" && event.operation === "upsert" && event.scope === "session"
    && event.content.startsWith("<VMM_MEMORY_BASELINE>\n") && event.content.endsWith("\n</VMM_MEMORY_BASELINE>");
}

// contextExpirationCategory maps a persistent cleanup marker to the category proved by its source sequence.
// contextExpirationCategory 根据持久上下文清理标记引用的来源序号确定展示类别。
function contextExpirationCategory(
  event: SystemContextBody,
  memoryBaselineSeqs: ReadonlySet<number>,
): ContextExpirationCategory | null {
  if (event.operation !== "remove" || event.scope !== "session" || event.context_key !== "injection.persistent") return null;
  // The source can be on a page that has yet to load, so an unresolved marker stays in added context.
  // 来源可能在尚未加载的页面上，因此未解析的标记仍归为附加上下文。
  const marker = /^expired_source_seq=([1-9][0-9]*)$/u.exec(event.content);
  if (!marker) return null;
  return memoryBaselineSeqs.has(Number(marker[1])) ? "memoryExpiration" : "injection";
}

// contextInjectionMessage turns a context event into a categorized notice with instruction text or a compact expiration state.
// contextInjectionMessage 将上下文事件转换成分类提示，显示指令正文或简短的失效状态。
function contextInjectionMessage(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  summary: ConversationEventSummary,
  event: SystemContextBody,
  rootInput: V1RootInputPresentationMetadata | undefined,
  expiration: ContextExpirationCategory | null,
  t: Translation,
): ConversationMessage {
  return contextInjectionNotice({ ...base, metadata: presentationMetadata(summary, rootInput) }, [{
    context_key: event.context_key,
    category: expiration ?? contextInjectionCategory(event.context_key),
    operation: expiration ? "expire" : event.operation,
    content: expiration ? "" : event.content,
  }], t);
}

// contextInjectionNotice builds one card for one or more consecutive injections: the title names each
// category once, in order, and the detail lists every exact text the model received.
// contextInjectionNotice 为一条或多条连续注入构建一张卡片：标题按顺序列出各类别（各一次），
// 详情列出模型收到的每段原文。
function contextInjectionNotice(
  base: Pick<ConversationMessage, "id" | "conversationId" | "createdAt" | "metadata">,
  entries: readonly ContextInjectionEntry[],
  t: Translation,
): ConversationMessage {
  const pages = contextInjectionPages(entries);
  const categories = pages.map((page) => page.category);
  const operations = new Set(entries.map((entry) => entry.operation));
  const sections = pages.flatMap((page) => [
    `### ${contextInjectionCategoryLabel(page.category, t)}`,
    ...page.entries.map((entry) => contextInjectionEntryMarkdown(entry, t)),
  ]);
  return {
    ...base,
    role: "assistant",
    recordKind: "display-note",
    title: categories.length === 1 && categories[0] === "memoryExpiration"
      ? t("conversation.contextInjection.memoryExpirationTitle")
      : t("conversation.contextInjection.title").replace(
        "{category}", categories.map((category) => contextInjectionCategoryLabel(category, t)).join("/"),
      ),
    body: sections.join("\n\n"),
    metadata: {
      ...(base.metadata as Record<string, unknown> | undefined),
      notice_kind: "context_injection",
      context_entries: entries,
      context_categories: categories,
      summary: operations.size > 1
        ? t("conversation.contextInjection.mixed")
        : contextInjectionOperationLabel(entries[0]!.operation, t),
      status: "completed",
    },
  };
}

// mergeConsecutiveContextInjections folds every run of adjacent injection cards into one card, so a
// request that injects several prompts shows a single combined notice.
// mergeConsecutiveContextInjections 把每段相邻的注入卡片合并为一张，使一次注入多段提示词的请求只显示一条合并提示。
function mergeConsecutiveContextInjections(
  messages: readonly ConversationMessage[],
  t: Translation,
): ConversationMessage[] {
  const merged: ConversationMessage[] = [];
  let run: ConversationMessage[] = [];
  const flush = () => {
    if (run.length === 1) merged.push(run[0]!);
    if (run.length > 1) {
      merged.push(contextInjectionNotice(run[0]!, run.flatMap((message) => readContextInjectionEntries(message.metadata) ?? []), t));
    }
    run = [];
  };
  for (const message of messages) {
    if (readContextInjectionEntries(message.metadata)) {
      run.push(message);
      continue;
    }
    flush();
    merged.push(message);
  }
  flush();
  return merged;
}

// markdownCodeSpan wraps text in a code span whose fence outlasts any backtick run inside it.
// markdownCodeSpan 用长于内部任何反引号串的围栏把文本包为行内代码。
function markdownCodeSpan(text: string): string {
  const longestRun = Math.max(0, ...(text.match(/`+/g) ?? []).map((run) => run.length));
  const fence = "`".repeat(longestRun + 1);
  const padding = text.startsWith("`") || text.endsWith("`") ? " " : "";
  return `${fence}${padding}${text}${padding}${fence}`;
}

// displayNoteTitle localizes host-owned notice kinds and keeps producer detail for the rest.
// displayNoteTitle 本地化宿主自有通知种类，其余保留生产端细节。
function displayNoteTitle(event: Extract<ConversationEventBody, { kind: "display_note" }>, t: Translation): string {
  if (event.notice_kind === "goal_status") return goalNoticeTitle(event.reason_code, t);
  if (isDedicatedMemoryNoticeKind(event.notice_kind)) return dedicatedMemoryNoticeTitle(event.notice_kind, t);
  return event.redacted_detail ?? event.reason_code;
}

// turnStatsMetadata maps one typed bucket set to the branch tooltip's established field contract.
// turnStatsMetadata 将一组强类型统计桶映射为分支悬浮卡的既有字段契约。
// total/subagent are exact persisted scopes; durationMs/outcome describe the terminal Turn; coverage is optional view evidence.
// total/subagent 是精确持久化范围；durationMs/outcome 描述终态 Turn；coverage 是可选视图证据。
// Returns display metadata while leaving all persisted accounting fields unchanged.
// 返回展示元数据，保持全部持久化核算字段不变。
function turnStatsMetadata(
  total: StatBucket,
  subagent: StatBucket,
  durationMs: number | undefined,
  outcome: "completed" | "abandoned" | "stopped",
  coverage: TurnUsageCoverage | undefined,
): Record<string, unknown> {
  return {
    turn_kind: "user_turn",
    status: outcome,
    duration_ms: durationMs,
    output_tokens_per_second: averageTurnOutputSpeed(coverage?.outputTiming?.outputTokens, coverage?.outputTiming?.outputDurationMs),
    output_duration_ms: coverage?.outputTiming?.outputDurationMs,
    time_to_first_token_ms: coverage?.outputTiming?.requestCount ? coverage.outputTiming.firstTokenLatencyMs / coverage.outputTiming.requestCount : undefined,
    timed_api_call_count: coverage?.outputTiming?.requestCount,
    provider_call_count: total.api_call_count,
    unknown_usage_api_call_count: total.unknown_usage_api_call_count,
    tool_call_count: total.tool_call_count,
    subagent_return_count: total.subagent_return_count,
    ...usageFields(total),
    usage_coverage: coverage?.total,
    usage_groups: subagent.api_call_count > 0 || subagent.tool_call_count > 0 || subagent.subagent_return_count > 0
      ? [{
        kind: "subagent",
        provider_call_count: subagent.api_call_count,
        unknown_usage_api_call_count: subagent.unknown_usage_api_call_count,
        tool_call_count: subagent.tool_call_count,
        subagent_return_count: subagent.subagent_return_count,
        ...usageFields(subagent),
        usage_coverage: coverage?.subagent,
      }]
      : [],
  };
}

// usageFields preserves unknown token dimensions by omitting them from the display metadata.
// usageFields 通过省略未知 token 维度来保留其未知语义。
function usageFields(bucket: StatBucket): Record<string, unknown> {
  return {
    ...(bucket.usage.input_tokens === undefined ? {} : { input_tokens: bucket.usage.input_tokens }),
    ...(bucket.usage.uncached_input_tokens === undefined ? {} : { uncached_input_tokens: bucket.usage.uncached_input_tokens }),
    ...(bucket.usage.output_tokens === undefined ? {} : { output_tokens: bucket.usage.output_tokens }),
    ...(bucket.usage.reasoning_tokens === undefined ? {} : { reasoning_tokens: bucket.usage.reasoning_tokens }),
    ...(bucket.usage.total_tokens === undefined ? {} : { total_tokens: bucket.usage.total_tokens }),
    ...(bucket.usage.cache_read_tokens === undefined ? {} : { cached_input_tokens: bucket.usage.cache_read_tokens }),
    ...(bucket.usage.cache_write_tokens === undefined ? {} : { cache_creation_input_tokens: bucket.usage.cache_write_tokens }),
  };
}

// apiDurationMs derives one display-only duration from persisted request timing checkpoints.
// apiDurationMs 从持久化请求时间点派生一个纯展示耗时。
function apiDurationMs(stats: ApiStatsBody | undefined): number | undefined {
  const start = stats?.timing.first_event_at_ms ?? stats?.timing.dispatched_at_ms ?? stats?.timing.prepared_at_ms;
  const end = stats?.timing.completed_at_ms;
  return start === undefined || end === undefined ? undefined : Math.max(0, end - start);
}

// displayPayload serializes one typed tool payload without text parsing or fallback field probing.
// displayPayload 序列化一个强类型工具载荷，不进行正文解析或后备字段探测。
function displayPayload(payload: unknown): string {
  if (typeof payload === "string") return payload;
  return JSON.stringify(payload, null, 2);
}

// rootInputPresentationMetadata reads the exact adapter marker from one restored display record.
// rootInputPresentationMetadata 从一条恢复后的展示记录读取精确适配标记。
export function rootInputPresentationMetadata(
  message: ConversationMessage | undefined,
): V1RootInputPresentationMetadata | null {
  if (!message?.metadata || typeof message.metadata !== "object" || Array.isArray(message.metadata)) {
    return null;
  }
  const metadata = message.metadata as Record<string, unknown>;
  return Number.isSafeInteger(metadata.v1_root_input_seq) && typeof metadata.v1_root_input_text === "string"
    ? {
      seq: metadata.v1_root_input_seq as number,
      text: metadata.v1_root_input_text,
    }
    : null;
}

// completedTurnEndSeqForPresentation resolves the terminal TurnStats seq for one exact displayed Turn.
// completedTurnEndSeqForPresentation 为一个精确展示回合解析终态 TurnStats seq。
// Parameters: messages are canonical V1 projections and sourceMessage is the Turn-end action owner.
// 参数：messages 是规范 V1 投影，sourceMessage 是回合末尾操作的拥有者。
// Returns: the exact terminal seq, or null when the Turn has not completed.
// 返回值：精确终态 seq；回合尚未完成时返回 null。
export function completedTurnEndSeqForPresentation(
  messages: readonly ConversationMessage[],
  sourceMessage: ConversationMessage | undefined,
): number | null {
  if (
    !sourceMessage?.metadata
    || typeof sourceMessage.metadata !== "object"
    || Array.isArray(sourceMessage.metadata)
  ) {
    return null;
  }
  const sourceMetadata = sourceMessage.metadata as Record<string, unknown>;
  if (!Number.isSafeInteger(sourceMetadata.v1_turn_id)) return null;
  const turnStatsMessage = messages.find((message) => {
    if (
      message.recordKind !== "turn-stats"
      || !message.metadata
      || typeof message.metadata !== "object"
      || Array.isArray(message.metadata)
    ) {
      return false;
    }
    const metadata = message.metadata as Record<string, unknown>;
    return metadata.v1_turn_id === sourceMetadata.v1_turn_id;
  });
  if (
    !turnStatsMessage?.metadata
    || typeof turnStatsMessage.metadata !== "object"
    || Array.isArray(turnStatsMessage.metadata)
  ) {
    return null;
  }
  const turnStatsMetadata = turnStatsMessage.metadata as Record<string, unknown>;
  return Number.isSafeInteger(turnStatsMetadata.v1_seq)
    ? turnStatsMetadata.v1_seq as number
    : null;
}
