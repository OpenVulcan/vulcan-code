import type { ConversationSummary, SubagentNavigationSnapshot } from "@/types";

/** Host callbacks and authoritative ids used to navigate collaboration participants.
 * 用于导航协作参与者的宿主回调与权威身份集合。*/
export interface ConversationV1CollaborationNavigation {
  /** Live conversations that the host can select directly.
   * 宿主可以直接选择的实时会话。*/
  conversations: readonly Pick<ConversationSummary, "id">[];
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
  return { availableSessionIds, onOpenSession };
}
