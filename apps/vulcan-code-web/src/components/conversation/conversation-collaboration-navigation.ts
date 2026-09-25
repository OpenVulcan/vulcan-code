import type { ConversationSummary, SubagentNavigationSnapshot } from "@/types";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import { resolveV1SubagentOrchestrationPresentation } from "./tool-presentations/v1-subagent-orchestration-presentation";

/** Resolves names from persisted tool identities and current navigation labels, never assistant prose.
 * 从持久工具身份与当前导航标签解析名字，绝不解析助手正文。
 * `events` belongs to one window; current host labels override historical labels. Returns exact session-name pairs.
 * `events` 属于同一窗口；当前宿主标签覆盖历史标签。返回精确会话名称映射。 */
export function conversationV1SubagentNames(events: readonly ConversationEventSummary[], current?: ReadonlyMap<string, string>): ReadonlyMap<string, string> {
  // Closed children remain identifiable through their original typed tool resource references.
  // 已关闭子代理仍可通过原有强类型工具资源引用识别。
  const names = new Map<string, string>();
  for (const summary of events) {
    if (summary.event.kind !== "tool_call") continue;
    // Reuse the same contract adapter as the existing create/list tool cards.
    // 复用既有创建及列表工具卡使用的契约适配器。
    const presentation = resolveV1SubagentOrchestrationPresentation({
      ...summary, kind: "tool_call", tool_call: summary.event,
      group: { turn_id: summary.turn_id, api_call_id: summary.event.api_call_id },
    });
    if (!presentation) continue;
    if (presentation.operation === "create" && presentation.target) {
      for (const reference of summary.event.external_refs ?? []) {
        if (reference.kind === "subagent") names.set(reference.id, presentation.target);
      }
    }
    for (const agent of presentation.agents) names.set(agent.id, agent.name);
  }
  for (const [id, name] of current ?? []) names.set(id, name);
  return names;
}

/** Host callbacks and authoritative ids used to navigate collaboration participants.
 * 用于导航协作参与者的宿主回调与权威身份集合。*/
export interface ConversationV1CollaborationNavigation {
  /** Live conversations that the host can select directly.
   * 宿主可以直接选择的实时会话。*/
  conversations: readonly (Pick<ConversationSummary, "id"> & Partial<Pick<ConversationSummary, "title">>)[];
  /** Backend-routed parent and child task snapshot for the current conversation.
   * 当前会话对应的后端父会话与子任务导航快照。*/
  navigation: SubagentNavigationSnapshot | null;
  /** Selects one direct conversation through the existing workspace route.
   * 通过现有工作区路由选择一个直接会话。*/
  onSelectConversation: (conversationId: string) => void;
  /** Opens one embedded child through the existing routed context-panel path.
   * 通过现有右侧上下文面板路由打开一个嵌入式子会话。*/
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean> | boolean;
}

/** Output of the exact collaboration navigation adapter used by the V1 renderer.
 * V1 渲染器使用的精确协作导航适配器输出。*/
export interface ConversationV1CollaborationNavigationResult {
  /** Authoritative agent labels indexed by their child session identity.
   * 以子会话身份索引的权威智能体标签。 */
  subagentNames: ReadonlyMap<string, string>;
  /** Session ids with a known direct or child route in the host snapshot.
   * 宿主快照中存在直接路由或子任务路由的会话身份。*/
  availableSessionIds: ReadonlySet<string>;
  /** Opens a direct conversation or an embedded child using its exact route.
   * 使用精确路由打开直接会话或嵌入式子会话。*/
  onOpenSession: (sessionId: string) => void;
}

/** Builds the exact session-id set that collaboration cards may navigate to.
 * 构建协作卡片可以导航到的精确会话身份集合。
 *
 * @param conversations direct conversation summaries currently owned by the host.
 *                       宿主当前拥有的直接会话摘要。
 * @param navigation backend-routed subagent snapshot for the selected parent.
 *                    当前选中父会话的后端子代理导航快照。
 * @returns ids with an explicit direct-conversation or child-task route.
 *          具备明确直接会话或子任务路由的身份集合。*/
export function conversationV1CollaborationSessionIds(
  conversations: readonly Pick<ConversationSummary, "id">[],
  navigation: SubagentNavigationSnapshot | null,
): ReadonlySet<string> {
  const ids = new Set(conversations.map((conversation) => conversation.id));
  if (navigation === null) return ids;
  for (const task of navigation.tasks ?? []) {
    ids.add(task.taskId);
  }
  // The active child can remain routable while its task row is temporarily absent from the projection.
  // 活动子会话的任务行暂时未投影时，仍通过精确 activeTaskId 保留可路由性。
  if (navigation.activeTaskId !== null && navigation.activeTaskId !== undefined) {
    ids.add(navigation.activeConversationId);
  }
  return ids;
}

/** Creates the host-owned collaboration opener without parsing event text or ids.
 * 创建宿主持有的协作打开器，不解析事件正文或猜测身份。
 *
 * @param navigation exact host route callbacks and backend navigation snapshot.
 *                    精确的宿主路由回调与后端导航快照。
 * @returns the available-id set and exact route opener consumed by the renderer.
 *          渲染器消费的可用身份集合与精确路由打开器。*/
export function createConversationV1CollaborationNavigation(
  navigation: ConversationV1CollaborationNavigation,
): ConversationV1CollaborationNavigationResult {
  const availableSessionIds = conversationV1CollaborationSessionIds(
    navigation.conversations,
    navigation.navigation,
  );
  const onOpenSession = (sessionId: string): void => {
    if (navigation.conversations.some((conversation) => conversation.id === sessionId)) {
      navigation.onSelectConversation(sessionId);
      return;
    }
    const snapshot = navigation.navigation;
    if (snapshot === null) return;
    const task = snapshot.tasks?.find((candidate) => candidate.taskId === sessionId);
    if (task !== undefined) {
      void navigation.onOpenSubagentPanel(snapshot.parentConversationId, task.taskId);
      return;
    }
    if (snapshot.activeConversationId === sessionId
      && snapshot.activeTaskId !== null
      && snapshot.activeTaskId !== undefined) {
      void navigation.onOpenSubagentPanel(snapshot.parentConversationId, snapshot.activeTaskId);
    }
  };
  // taskId is the canonical child session id, and label is the backend's human-facing name.
  // taskId 是规范子会话身份，label 是后端提供的可读名称。
  const subagentNames = new Map((navigation.navigation?.tasks ?? []).map((task) => [task.taskId, task.label]));
  // Cross-session messages use the same authoritative titles as the host conversation list.
  // 跨会话消息使用与宿主会话列表一致的权威标题。
  for (const conversation of navigation.conversations) {
    if (conversation.title && !subagentNames.has(conversation.id)) subagentNames.set(conversation.id, conversation.title);
  }
  return { availableSessionIds, onOpenSession, subagentNames };
}
