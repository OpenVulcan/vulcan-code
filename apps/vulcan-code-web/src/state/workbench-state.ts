import { normalizedSessionRunPhase } from "@/components/conversation/conversation-runtime";
import type {
  ConversationArchivedDelta,
  ContextInjectionChangedDelta,
  ConversationSummary,
  PendingPromptsChangedDelta,
  PendingQuestionSnapshot,
  PendingPromptSummary,
  ProjectRemovedDelta,
  SessionAddonsChangedResponse,
  SessionAddonsChangedDelta,
  SessionVmmPolicyByConversation,
  SubagentNavigationSnapshot,
  SubagentPanelRouteResponse,
  UpdateConversationRunPhaseDelta,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";

// WorkbenchMergeOptions tunes snapshot merge behavior for local interaction state.
// WorkbenchMergeOptions 调整快照合并时本地交互状态的处理方式。
export interface WorkbenchMergeOptions {
  // Whether a confirmed service-instance change may reset monotonically increasing revisions.
  // 已确认服务实例切换时，是否允许重置单调递增的版本。
  allowServiceRevisionReset?: boolean;
  // Whether local project expansion should win over incoming snapshots.
  // 本地项目展开状态是否优先于传入快照。
  preserveExpandedProjectIds?: boolean;
  // Controls whether selectedConversationId comes from local UI state or the incoming service snapshot.
  // 控制 selectedConversationId 来自本地界面状态还是传入的服务端快照。
  selectedConversationSource?: "local" | "incoming";
  // Focused conversation id for session-scoped hydration responses.
  // 会话范围水合响应对应的明确目标会话标识。
  focusedConversationId?: string | null;
}

// shouldAcceptWorkbenchRevision returns whether one incoming service revision can mutate UI state.
// shouldAcceptWorkbenchRevision 判断传入服务版本是否允许修改界面状态。
export function shouldAcceptWorkbenchRevision(currentRevision: number, incomingRevision: number): boolean {
  return currentRevision <= 0 || incomingRevision > currentRevision;
}

// visibleConversationId returns a candidate only when it still exists in the provided sidebar list.
// visibleConversationId 仅在候选会话仍存在于给定侧栏列表时返回该候选。
//
// Parameters:
// 参数：
// conversations is the authoritative conversation list for this projection.
// conversations 是本次投影使用的权威会话列表。
// candidate is the selected conversation candidate to validate.
// candidate 是需要验证的选中会话候选值。
//
// Returns:
// 返回值：
// The candidate id when it is visible in conversations; otherwise null.
// 当候选标识存在于会话列表中时返回该标识，否则返回 null。
function visibleConversationId(
  conversations: ConversationSummary[],
  candidate: string | null | undefined,
): string | null {
  return candidate && conversations.some((conversation) => conversation.id === candidate)
    ? candidate
    : null;
}

// mergeWorkbenchSnapshot merges one authoritative structural snapshot while preserving UI-local selection state.
// mergeWorkbenchSnapshot 合并一份权威结构快照，同时保留界面本地选择状态。
export function mergeWorkbenchSnapshot(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  options: WorkbenchMergeOptions = {},
): WorkbenchSnapshot {
  const currentRevision = current.revision ?? 0;
  const incomingRevision = incoming.revision ?? 0;
  if (
    !options.allowServiceRevisionReset
    && !shouldAcceptWorkbenchRevision(currentRevision, incomingRevision)
  ) {
    return current;
  }

  const incomingProjectIds = new Set(incoming.projects.map((project) => project.id));
  const expandedProjectSource = options.preserveExpandedProjectIds === false
    ? incoming.expandedProjectIds ?? current.expandedProjectIds
    : current.expandedProjectIds ?? incoming.expandedProjectIds;
  const expandedProjectIds = (expandedProjectSource ?? [])
    .filter((projectId) => incomingProjectIds.has(projectId));
  const incomingSelectedConversationId = visibleConversationId(
    incoming.conversations,
    incoming.selectedConversationId,
  );
  const localSelectedConversationId = visibleConversationId(
    incoming.conversations,
    current.selectedConversationId,
  );
  const selectedConversationId = options.selectedConversationSource === "incoming"
    ? incomingSelectedConversationId ?? localSelectedConversationId ?? incoming.conversations[0]?.id ?? null
    : localSelectedConversationId ?? incomingSelectedConversationId ?? incoming.conversations[0]?.id ?? null;
  const incomingSubagentNavigations = deriveIncomingSubagentNavigations(incoming);
  const mergedWorkbench = {
    ...incoming,
    selectedConversationId,
    expandedProjectIds,
    conversations: mergeConversationSummaries(current.conversations, incoming.conversations, selectedConversationId),
    pendingPromptsByConversation: incoming.pendingPromptsByConversation ?? current.pendingPromptsByConversation ?? [],
    pendingQuestions: incoming.pendingQuestions ?? current.pendingQuestions ?? [],
    sessionAddonsByConversation: incoming.sessionAddonsByConversation ?? current.sessionAddonsByConversation ?? [],
    sessionVmmPoliciesByConversation: incoming.sessionVmmPoliciesByConversation ?? current.sessionVmmPoliciesByConversation ?? [],
    contextInjectionByConversation: mergeContextInjectionByConversation(
      current.contextInjectionByConversation ?? [],
      incoming.contextInjectionByConversation ?? [],
    ),
    subagentNavigationByConversation: mergeSubagentNavigationsByConversation(
      current.subagentNavigationByConversation ?? [],
      incomingSubagentNavigations,
      focusedSubagentNavigationClearIds(incomingSubagentNavigations, options.focusedConversationId ?? null),
    ),
  };
  return deriveSelectedConversationFields({
    ...mergedWorkbench,
    conversations: applyNormalizedConversationAttention(mergedWorkbench.conversations, mergedWorkbench),
  });
}

// deriveSelectedConversationFields projects top-level selected-session fields from normalized per-session caches.
// deriveSelectedConversationFields 从按会话归一化缓存派生顶层当前会话字段。
function deriveSelectedConversationFields(workbench: WorkbenchSnapshot): WorkbenchSnapshot {
  const selectedConversationId = workbench.selectedConversationId ?? null;
  // liveParentConversationIds contains every active regular conversation allowed to own a navigation tree.
  // liveParentConversationIds 包含允许拥有导航树的全部活动常规会话。
  const liveParentConversationIds = new Set(workbench.conversations.map((conversation) => conversation.id));
  // subagentNavigationByConversation removes trees whose regular parent is no longer active.
  // subagentNavigationByConversation 删除常规父会话已不活动的导航树。
  const subagentNavigationByConversation = (workbench.subagentNavigationByConversation ?? [])
    .filter((navigation) => liveParentConversationIds.has(navigation.parentConversationId));
  const pendingQuestion = selectedConversationId
    ? (workbench.pendingQuestions ?? []).find((question) => question.sessionId === selectedConversationId) ?? null
    : null;
  const pendingPrompts = selectedConversationId
    ? workbench.pendingPromptsByConversation?.find((entry) => entry.conversationId === selectedConversationId)?.pendingPrompts ?? []
    : [];
  const sessionAddons = selectedConversationId
    ? workbench.sessionAddonsByConversation?.find((entry) => entry.conversationId === selectedConversationId) ?? null
    : null;
  const sessionVmmPolicyEntry = selectedConversationId
    ? workbench.sessionVmmPoliciesByConversation?.find((entry) => entry.conversationId === selectedConversationId) ?? null
    : null;
  const subagentNavigation = selectedConversationId
    ? subagentNavigationByConversation.find((navigation) => navigation.parentConversationId === selectedConversationId) ?? null
    : null;
  const contextInjectionEntry = selectedConversationId
    ? workbench.contextInjectionByConversation?.find((entry) => entry.conversationId === selectedConversationId) ?? null
    : null;
  return {
    ...workbench,
    pendingQuestion,
    pendingPrompts,
    sessionGoal: sessionAddons?.sessionGoal ?? null,
    sessionTodolist: sessionAddons?.sessionTodolist ?? null,
    sessionPlanMode: sessionAddons?.sessionPlanMode ?? null,
    sessionOutputStyle: sessionAddons?.sessionOutputStyle ?? null,
    sessionContextCompactionWindowNodeId:
      sessionAddons?.sessionContextCompactionWindowNodeId ?? null,
    sessionProviderUsage: sessionAddons?.sessionProviderUsage ?? null,
    sessionVmmPolicy: sessionVmmPolicyEntry?.sessionVmmPolicy ?? null,
    subagentNavigationByConversation,
    subagentNavigation,
    contextInjection: contextInjectionEntry?.contextInjection ?? null,
  };
}

// workbenchWithSelectedConversationProjection reprojects selected-session fields for a UI-local conversation choice.
// workbenchWithSelectedConversationProjection 根据 UI 本地会话选择重新投影当前会话字段。
//
// Parameters:
// 参数：
// workbench is the normalized snapshot to project.
// workbench 是需要投影的规范化快照。
// selectedConversationId is the UI-local selected conversation id.
// selectedConversationId 是 UI 本地选中的会话标识。
//
// Returns:
// 返回值：
// A workbench whose top-level selected-session fields match the visible selected conversation, or are cleared.
// 返回顶层当前会话字段匹配可见选中会话或已清空的工作台快照。
export function workbenchWithSelectedConversationProjection(
  workbench: WorkbenchSnapshot,
  selectedConversationId: string | null | undefined,
): WorkbenchSnapshot {
  const visibleSelectedConversationId = visibleConversationId(
    workbench.conversations,
    selectedConversationId,
  );
  return deriveSelectedConversationFields({
    ...workbench,
    selectedConversationId: visibleSelectedConversationId,
  });
}

// upsertPendingPromptsForConversation upserts or removes one conversation prompt queue.
// upsertPendingPromptsForConversation 写入或删除单个会话的待发送提示词队列。
function upsertPendingPromptsForConversation(
  current: PendingPromptsChangedDelta[],
  entry: PendingPromptsChangedDelta,
): PendingPromptsChangedDelta[] {
  const next = current.filter((candidate) => candidate.conversationId !== entry.conversationId);
  return entry.pendingPrompts.length > 0 ? [...next, entry] : next;
}

// upsertSessionAddonsForConversation upserts or removes one session addon projection.
// upsertSessionAddonsForConversation 写入或删除单个会话附属投影。
function upsertSessionAddonsForConversation(
  current: SessionAddonsChangedDelta[],
  entry: SessionAddonsChangedDelta,
): SessionAddonsChangedDelta[] {
  const next = current.filter((candidate) => candidate.conversationId !== entry.conversationId);
  const hasAddon = Boolean(
    entry.sessionGoal
      || entry.sessionTodolist
      || entry.sessionPlanMode
      || entry.sessionOutputStyle
      || entry.sessionContextCompactionWindowNodeId
      || entry.sessionProviderUsage,
  );
  return hasAddon ? [...next, entry] : next;
}

// upsertSessionVmmPolicyForConversation upserts or removes one conversation VMM policy projection.
// upsertSessionVmmPolicyForConversation 写入或删除单个会话的 VMM 策略投影。
function upsertSessionVmmPolicyForConversation(
  current: SessionVmmPolicyByConversation[],
  entry: SessionAddonsChangedDelta,
): SessionVmmPolicyByConversation[] {
  if (entry.sessionVmmPolicy === undefined) {
    return current;
  }
  const next = current.filter((candidate) => candidate.conversationId !== entry.conversationId);
  return entry.sessionVmmPolicy ? [...next, {
    conversationId: entry.conversationId,
    sessionVmmPolicy: entry.sessionVmmPolicy,
  }] : next;
}

// mergeContextInjectionByConversation merges memory-panel projections by conversation id.
// mergeContextInjectionByConversation 按会话标识合并记忆面板投影。
function mergeContextInjectionByConversation(
  current: ContextInjectionChangedDelta[],
  incoming: ContextInjectionChangedDelta[],
): ContextInjectionChangedDelta[] {
  const merged = new Map(current.map((entry) => [entry.conversationId, entry]));
  for (const entry of incoming) {
    merged.set(entry.conversationId, entry);
  }
  return [...merged.values()];
}

// upsertContextInjectionForConversation upserts or removes one memory-panel projection.
// upsertContextInjectionForConversation 写入或删除单个会话的记忆面板投影。
function upsertContextInjectionForConversation(
  current: ContextInjectionChangedDelta[],
  entry: ContextInjectionChangedDelta,
): ContextInjectionChangedDelta[] {
  const next = current.filter((candidate) => candidate.conversationId !== entry.conversationId);
  return entry.contextInjection ? [...next, entry] : next;
}

// deriveIncomingSubagentNavigations extracts full-snapshot subagent navigation cache entries.
// deriveIncomingSubagentNavigations 提取全量快照中的子代理导航缓存条目。
function deriveIncomingSubagentNavigations(workbench: WorkbenchSnapshot): SubagentNavigationSnapshot[] {
  const entries = workbench.subagentNavigationByConversation ?? [];
  return workbench.subagentNavigation ? [...entries, workbench.subagentNavigation] : entries;
}

// mergeSubagentNavigationsByConversation merges subagent navigation snapshots by parent conversation id and applies explicit clears first.
// mergeSubagentNavigationsByConversation 按父会话标识合并子代理导航快照，并先应用显式清理。
function mergeSubagentNavigationsByConversation(
  current: SubagentNavigationSnapshot[],
  incoming: SubagentNavigationSnapshot[],
  clearParentConversationIds: Set<string> = new Set(),
): SubagentNavigationSnapshot[] {
  const merged = new Map(current.map((navigation) => [navigation.parentConversationId, navigation]));
  for (const parentConversationId of clearParentConversationIds) {
    merged.delete(parentConversationId);
  }
  for (const navigation of incoming) {
    merged.set(navigation.parentConversationId, navigation);
  }
  return [...merged.values()];
}

// focusedSubagentNavigationClearIds returns the focused parent ids whose missing navigation is authoritative.
// focusedSubagentNavigationClearIds 返回缺失导航具有权威删除语义的聚焦父会话标识。
function focusedSubagentNavigationClearIds(
  incoming: SubagentNavigationSnapshot[],
  focusedConversationId: string | null,
): Set<string> {
  if (!focusedConversationId) {
    return new Set();
  }
  return incoming.some((navigation) => navigation.parentConversationId === focusedConversationId)
    ? new Set()
    : new Set([focusedConversationId]);
}

// upsertSubagentNavigationForConversation upserts or removes one parent conversation navigation snapshot.
// upsertSubagentNavigationForConversation 写入或删除单个父会话的子代理导航快照。
function upsertSubagentNavigationForConversation(
  current: SubagentNavigationSnapshot[],
  parentConversationId: string | null,
  navigation: SubagentNavigationSnapshot | null,
): SubagentNavigationSnapshot[] {
  if (!parentConversationId) {
    return current;
  }
  const next = current.filter((candidate) => candidate.parentConversationId !== parentConversationId);
  return navigation ? [...next, navigation] : next;
}

// applyPendingQuestionAttention updates sidebar ask markers from the normalized ask queue.
// applyPendingQuestionAttention 根据归一化 ask 队列更新侧栏提问标记。
function applyPendingQuestionAttention(
  conversations: ConversationSummary[],
  pendingQuestions: PendingQuestionSnapshot[],
): ConversationSummary[] {
  const pendingBySession = new Map(pendingQuestions.map((question) => [question.sessionId, question]));
  return conversations.map((conversation) => {
    const pendingQuestion = pendingBySession.get(conversation.id);
    if (pendingQuestion) {
      return {
        ...conversation,
        attention: {
          kind: "ask",
          priority: 90,
          requestId: pendingQuestion.id,
          label: "Ask question waiting",
        },
      };
    }
    if (conversation.attention?.kind !== "ask") {
      return conversation;
    }
    return {
      ...conversation,
      attention: conversation.unread
        ? {
          kind: "unread",
          priority: 10,
          label: "Unread activity",
        }
        : null,
    };
  });
}

// pendingQuestionsFromDelta returns the next normalized ask queue from a full or single-session delta.
// pendingQuestionsFromDelta 根据全量或单会话增量返回下一版归一化提问队列。
function pendingQuestionsFromDelta(
  current: PendingQuestionSnapshot[],
  conversationId: string | null | undefined,
  pendingQuestion: PendingQuestionSnapshot | null | undefined,
  pendingQuestions: PendingQuestionSnapshot[] | undefined,
): PendingQuestionSnapshot[] {
  if (pendingQuestions) {
    return pendingQuestions;
  }
  if (!conversationId) {
    return pendingQuestion ? [pendingQuestion] : [];
  }
  const nextQuestions = current.filter((question) => question.sessionId !== conversationId);
  if (pendingQuestion) {
    nextQuestions.push(pendingQuestion);
  }
  return nextQuestions.sort((left, right) => left.sequence - right.sequence);
}

// applySessionAddonAttention updates one sidebar Plan marker from session addon state.
// applySessionAddonAttention 根据会话附属状态更新单个侧栏计划标记。
function applySessionAddonAttention(
  conversations: ConversationSummary[],
  entry: SessionAddonsChangedDelta,
): ConversationSummary[] {
  return conversations.map((conversation) => {
    if (conversation.id !== entry.conversationId || conversation.attention?.kind === "ask") {
      return conversation;
    }
    if (entry.sessionPlanMode?.executionMode?.toLowerCase() === "plan") {
      return {
        ...conversation,
        attention: {
          kind: "plan_confirm",
          priority: 80,
          requestId: entry.sessionPlanMode.artifact?.path ?? null,
          label: "Plan mode waiting",
        },
      };
    }
    if (conversation.attention?.kind !== "plan_confirm") {
      return conversation;
    }
    return {
      ...conversation,
      attention: conversation.unread
        ? {
          kind: "unread",
          priority: 10,
          label: "Unread activity",
        }
        : null,
    };
  });
}

// applyNormalizedConversationAttention reapplies per-session ask and Plan markers after summary replacement.
// applyNormalizedConversationAttention 在摘要替换后重新应用按会话归一化的提问与计划标记。
function applyNormalizedConversationAttention(
  conversations: ConversationSummary[],
  workbench: WorkbenchSnapshot,
): ConversationSummary[] {
  let nextConversations = conversations;
  for (const entry of workbench.sessionAddonsByConversation ?? []) {
    nextConversations = applySessionAddonAttention(nextConversations, entry);
  }
  return applyPendingQuestionAttention(nextConversations, workbench.pendingQuestions ?? []);
}

// applyConversationArchivedDelta applies one authoritative conversation archive transition without transcript ownership.
// applyConversationArchivedDelta 应用一条权威会话归档转换，且不再持有转录数据。
// Parameters: workbench is the structural state and delta identifies the archived conversation projection.
// 参数：workbench 是结构状态，delta 标识已归档会话投影。
// Returns: the normalized structural state after removing session-scoped projections.
// 返回值：移除会话范围投影后的规范化结构状态。
function applyConversationArchivedDelta(
  workbench: WorkbenchSnapshot,
  delta: ConversationArchivedDelta,
): WorkbenchSnapshot {
  const conversationId = delta.conversationId;
  const conversations = workbench.conversations.filter(
    (conversation) => conversation.id !== conversationId,
  );
  const selectedConversationId = visibleConversationId(
    conversations,
    delta.selectedConversationId,
  ) ?? conversations[0]?.id ?? null;
  const retainedArchivedConversations = (workbench.archivedConversations ?? [])
    .filter((conversation) => conversation.id !== conversationId);
  const archivedConversations = delta.archivedConversation
    ? [delta.archivedConversation, ...retainedArchivedConversations]
    : retainedArchivedConversations;
  return deriveSelectedConversationFields({
    ...workbench,
    conversations,
    archivedConversations,
    selectedConversationId,
    pendingPromptsByConversation: (workbench.pendingPromptsByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId),
    pendingQuestions: (workbench.pendingQuestions ?? [])
      .filter((question) => question.sessionId !== conversationId),
    sessionAddonsByConversation: (workbench.sessionAddonsByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId),
    sessionVmmPoliciesByConversation: (workbench.sessionVmmPoliciesByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId),
    contextInjectionByConversation: (workbench.contextInjectionByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId),
    contextUsage: workbench.selectedConversationId === conversationId
      ? null
      : workbench.contextUsage,
  });
}

// applyConversationsArchivedDelta applies a committed archive batch in backend order.
// applyConversationsArchivedDelta 按后端顺序应用一批已提交的归档转换。
// Parameters: workbench is the structural state and deltas are authoritative archive transitions.
// 参数：workbench 是结构状态，deltas 是权威归档转换集合。
// Returns: the state after every transition has been applied idempotently.
// 返回值：幂等应用全部转换后的状态。
function applyConversationsArchivedDelta(
  workbench: WorkbenchSnapshot,
  deltas: ConversationArchivedDelta[],
): WorkbenchSnapshot {
  return deltas.reduce(
    (nextWorkbench, delta) => applyConversationArchivedDelta(nextWorkbench, delta),
    workbench,
  );
}

// applyProjectRemovedDelta applies project membership and its committed child archive transitions atomically.
// applyProjectRemovedDelta 原子应用项目成员关系及其已提交的子会话归档转换。
// Parameters: workbench is the structural state and delta is the committed project removal.
// 参数：workbench 是结构状态，delta 是已提交的项目移除转换。
// Returns: the structural state converged to the backend commit.
// 返回值：收敛到后端提交后的结构状态。
function applyProjectRemovedDelta(
  workbench: WorkbenchSnapshot,
  delta: ProjectRemovedDelta,
): WorkbenchSnapshot {
  const archivedWorkbench = applyConversationsArchivedDelta(
    workbench,
    delta.archivedConversations,
  );
  return {
    ...archivedWorkbench,
    projects: delta.projects,
    expandedProjectIds: delta.expandedProjectIds,
  };
}

// applyWorkbenchDelta applies one partial update to the exact workbench domains owned by the delta.
// applyWorkbenchDelta 将单条局部更新应用到该增量精确拥有的工作台领域。
export function applyWorkbenchDelta(
  workbench: WorkbenchSnapshot,
  envelope: WorkbenchDeltaEnvelope,
): WorkbenchSnapshot {
  // ownsIndependentLifecycleDomain identifies a delta whose domain revision is accepted separately from unrelated state.
  // ownsIndependentLifecycleDomain 标识其领域版本独立于无关状态接收的增量。
  const ownsIndependentLifecycleDomain = envelope.delta.kind === "conversationArchived"
    || envelope.delta.kind === "conversationsArchived"
    || envelope.delta.kind === "projectRemoved"
    || envelope.delta.kind === "agentSkillsCatalogChanged"
    || envelope.delta.kind === "luaSkillsCatalogChanged"
    || envelope.delta.kind === "subagentCatalogChanged"
    || envelope.delta.kind === "pluginCatalogChanged"
    || envelope.delta.kind === "projectSettingsChanged"
    || envelope.delta.kind === "luaSkillsManagedRuntimeProgressChanged";
  if (!ownsIndependentLifecycleDomain && envelope.generation <= (workbench.revision ?? 0)) {
    return workbench;
  }
  // nextWorkbench applies only fields explicitly owned by this delta kind.
  // nextWorkbench 仅应用该增量类型明确拥有的字段。
  const nextWorkbench = applyWorkbenchDeltaPayload(workbench, envelope);
  return {
    ...nextWorkbench,
    revision: Math.max(workbench.revision ?? 0, envelope.generation),
  };
}

// applyWorkbenchDeltaPayload applies one delta payload before revision stamping.
// applyWorkbenchDeltaPayload 在写入版本号前应用单条增量载荷。
function applyWorkbenchDeltaPayload(
  workbench: WorkbenchSnapshot,
  envelope: WorkbenchDeltaEnvelope,
): WorkbenchSnapshot {
  switch (envelope.delta.kind) {
    case "updateConversationRunPhase":
      return applyUpdateConversationRunPhaseDelta(workbench, envelope.delta.payload);
    case "pendingPromptsChanged":
      return deriveSelectedConversationFields({
        ...workbench,
        pendingPromptsByConversation: upsertPendingPromptsForConversation(
          workbench.pendingPromptsByConversation ?? [],
          envelope.delta.payload,
        ),
      });
    case "conversationSummaryChanged":
      return applyConversationSummaryChangedDelta(workbench, envelope.delta.payload);
    case "conversationArchived":
      return applyConversationArchivedDelta(workbench, envelope.delta.payload);
    case "conversationsArchived":
      return applyConversationsArchivedDelta(
        workbench,
        envelope.delta.payload.archivedConversations,
      );
    case "projectRemoved":
      return applyProjectRemovedDelta(workbench, envelope.delta.payload);
    case "pendingQuestionChanged": {
      const pendingQuestions = pendingQuestionsFromDelta(
        workbench.pendingQuestions ?? [],
        envelope.delta.payload.conversationId ?? null,
        envelope.delta.payload.pendingQuestion ?? null,
        envelope.delta.payload.pendingQuestions,
      );
      return deriveSelectedConversationFields({
        ...workbench,
        pendingQuestions,
        conversations: applyPendingQuestionAttention(workbench.conversations, pendingQuestions),
      });
    }
    case "commandSessionsChanged":
      return {
        ...workbench,
        commandSessions: envelope.delta.payload.commandSessions,
      };
    case "subagentNavigationChanged":
      return deriveSelectedConversationFields({
        ...workbench,
        subagentNavigationByConversation: upsertSubagentNavigationForConversation(
          workbench.subagentNavigationByConversation ?? [],
          envelope.delta.payload.parentConversationId
            ?? envelope.delta.payload.subagentNavigation?.parentConversationId
            ?? null,
          envelope.delta.payload.subagentNavigation ?? null,
        ),
      });
    case "contextUsageChanged":
      return {
        ...workbench,
        contextUsage: envelope.delta.payload.contextUsage ?? null,
      };
    case "contextInjectionChanged":
      return applyContextInjectionChangedDelta(workbench, envelope.delta.payload);
    case "sessionAddonsChanged":
      return deriveSelectedConversationFields({
        ...workbench,
        conversations: applySessionAddonAttention(workbench.conversations, envelope.delta.payload),
        sessionAddonsByConversation: upsertSessionAddonsForConversation(
          workbench.sessionAddonsByConversation ?? [],
          envelope.delta.payload,
        ),
        sessionVmmPoliciesByConversation: upsertSessionVmmPolicyForConversation(
          workbench.sessionVmmPoliciesByConversation ?? [],
          envelope.delta.payload,
        ),
      });
    case "agentSkillsCatalogChanged":
      return {
        ...workbench,
        agentSkills: envelope.delta.payload.agentSkills,
      };
    case "luaSkillsCatalogChanged":
      return {
        ...workbench,
        luaSkills: envelope.delta.payload.luaSkills,
        plugins: envelope.delta.payload.plugins,
      };
    case "subagentCatalogChanged":
      return {
        ...workbench,
        subagentCatalog: envelope.delta.payload.subagentCatalog,
      };
    case "pluginCatalogChanged":
      return {
        ...workbench,
        luaSkills: envelope.delta.payload.luaSkills,
        plugins: envelope.delta.payload.plugins,
        agentSkills: envelope.delta.payload.agentSkills,
        subagentCatalog: envelope.delta.payload.subagentCatalog,
      };
    case "projectSettingsChanged":
      // changedProjectId selects the exact project-owned revision map or the global revision pair.
      // changedProjectId 选择准确的项目级版本映射或全局版本对。
      const changedProjectId = envelope.delta.payload.projectId;
      // changedDomain identifies the only revision advanced by this focused delta.
      // changedDomain 标识当前聚焦增量唯一推进的版本。
      const changedDomain = envelope.delta.payload.domain;
      if (changedProjectId === null) {
        return {
          ...workbench,
          projectSettingsRevisions: {
            ...workbench.projectSettingsRevisions,
            global: {
              ...workbench.projectSettingsRevisions.global,
              [changedDomain]: envelope.generation,
            },
          },
        };
      }
      // currentProjectRevisions provides the complete initial pair for a project's first focused delta.
      // currentProjectRevisions 为项目首次聚焦增量提供完整的初始版本对。
      const currentProjectRevisions = workbench.projectSettingsRevisions.byProject[changedProjectId]
        ?? { vmm: 0, agentSkills: 0, codebaseMemory: 0 };
      return {
        ...workbench,
        projectSettingsRevisions: {
          ...workbench.projectSettingsRevisions,
          byProject: {
            ...workbench.projectSettingsRevisions.byProject,
            [changedProjectId]: {
              ...currentProjectRevisions,
              [changedDomain]: envelope.generation,
            },
          },
        },
      };
    case "luaSkillsManagedRuntimeProgressChanged":
      return {
        ...workbench,
        luaSkills: {
          ...workbench.luaSkills,
          managedRuntimeProgress: envelope.delta.payload.progress,
        },
      };
    case "mediaUploadProgressChanged":
      // Media progress is an ordered ephemeral event; its bytes live only in the overlay state.
      // 媒体进度是有序临时事件，字节数仅保留在浮层状态中。
      return workbench;
    case "toolRequestProgressChanged":
      // Tool request progress lives only in ConversationLiveStreamStore and never enters snapshots.
      // 工具请求进度仅存在于 ConversationLiveStreamStore，绝不进入快照。
      return workbench;
    case "transientReasoningStatusChanged":
      // Transient reasoning lives only in ConversationLiveStreamStore and never enters snapshots.
      // 瞬态推理仅存在于 ConversationLiveStreamStore，绝不进入快照。
      return workbench;
  }
}

// applyConversationSummaryChangedDelta upserts one sidebar summary and applies its owned selection without touching transcript rows.
// applyConversationSummaryChangedDelta 写入单个侧栏摘要及其自有选择，不触碰转录消息行。
function applyConversationSummaryChangedDelta(
  workbench: WorkbenchSnapshot,
  delta: { conversation: ConversationSummary; selectedConversationId?: string | null },
): WorkbenchSnapshot {
  let found = false;
  const conversations = workbench.conversations.map((conversation) => {
    if (conversation.id !== delta.conversation.id) {
      return conversation;
    }
    found = true;
    return delta.conversation;
  });
  if (!found) {
    conversations.unshift(delta.conversation);
  }
  const nextWorkbench = {
    ...workbench,
    conversations,
    selectedConversationId: delta.selectedConversationId === undefined
      ? workbench.selectedConversationId
      : visibleConversationId(conversations, delta.selectedConversationId),
  };
  return deriveSelectedConversationFields({
    ...nextWorkbench,
    conversations: applyNormalizedConversationAttention(conversations, nextWorkbench),
  });
}

// updateWorkbenchSubagentNavigationRunPhase mirrors child run phases into navigation projections.
// updateWorkbenchSubagentNavigationRunPhase 将子任务运行阶段同步到导航投影。
// Parameters: workbench owns the projections, conversationId identifies the child, and runPhase is authoritative.
// 参数：workbench 持有投影，conversationId 标识子任务，runPhase 是权威阶段。
// Returns: the exact selected and cached navigation projections after the update.
// 返回值：更新后的精确当前导航投影与缓存导航投影。
function updateWorkbenchSubagentNavigationRunPhase(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  runPhase?: string,
): Pick<WorkbenchSnapshot, "subagentNavigation" | "subagentNavigationByConversation"> {
  return {
    subagentNavigation: updateSubagentNavigationRunPhase(
      workbench.subagentNavigation,
      conversationId,
      runPhase,
    ),
    subagentNavigationByConversation: workbench.subagentNavigationByConversation?.map((navigation) => (
      updateSubagentNavigationRunPhase(navigation, conversationId, runPhase) ?? navigation
    )),
  };
}

// updateSubagentNavigationRunPhase updates one exact child task inside a navigation projection.
// updateSubagentNavigationRunPhase 更新导航投影中的一个精确子任务。
// Parameters: navigation is the owner projection, conversationId identifies the child, and runPhase is authoritative.
// 参数：navigation 是所属投影，conversationId 标识子任务，runPhase 是权威阶段。
// Returns: the original projection when unrelated, otherwise one immutable updated projection.
// 返回值：不相关时返回原投影，否则返回一份不可变更新投影。
function updateSubagentNavigationRunPhase(
  navigation: WorkbenchSnapshot["subagentNavigation"],
  conversationId: string,
  runPhase?: string,
): WorkbenchSnapshot["subagentNavigation"] {
  // Archive lifecycle is authoritative: late execution phases cannot reopen a closed child.
  // 归档生命周期具有权威性：迟到的执行阶段不能重新打开已关闭子任务。
  if (!navigation?.tasks?.some((task) => task.taskId === conversationId && task.runState !== "closed")) {
    return navigation;
  }
  const normalized = normalizedSessionRunPhase(runPhase);
  if (!normalized) {
    return navigation;
  }
  const running = !["completed", "failed", "idle", "ready", "stopped"].includes(normalized);
  const runState = running ? "running" : terminalSubagentRunStateFromPhase(normalized);
  return {
    ...navigation,
    tasks: navigation.tasks.map((task) => (
      task.taskId === conversationId && task.runState !== "closed"
        ? { ...task, isRunning: running, runState }
        : task
    )),
  };
}

// terminalSubagentRunStateFromPhase maps terminal session phases to navigation states.
// terminalSubagentRunStateFromPhase 将终态会话阶段映射为导航状态。
// Parameters: phase is one normalized terminal phase.
// 参数：phase 是一个规范化终态阶段。
// Returns: failed or stopped verbatim, otherwise completed.
// 返回值：failed 或 stopped 原样返回，其他情况返回 completed。
function terminalSubagentRunStateFromPhase(phase: string): string {
  if (phase === "failed" || phase === "stopped") {
    return phase;
  }
  return "completed";
}

// applyUpdateConversationRunPhaseDelta updates one conversation lifecycle row without touching transcript messages.
// applyUpdateConversationRunPhaseDelta 在不触碰转录消息的情况下更新单个会话生命周期行。
function applyUpdateConversationRunPhaseDelta(
  workbench: WorkbenchSnapshot,
  delta: UpdateConversationRunPhaseDelta,
): WorkbenchSnapshot {
  const subagentNavigationProjection = updateWorkbenchSubagentNavigationRunPhase(workbench, delta.conversationId, delta.runPhase);
  return deriveSelectedConversationFields({
    ...workbench,
    conversations: workbench.conversations.map((conversation) => (
      conversation.id === delta.conversationId
        ? {
            ...conversation,
            runPhase: delta.runPhase,
            status: delta.status ?? conversation.status,
            lastUpdatedLabel: delta.lastUpdatedLabel ?? conversation.lastUpdatedLabel,
          }
        : conversation
    )),
    ...subagentNavigationProjection,
  });
}

// applyContextInjectionChangedDelta replaces the selected conversation memory-panel projection.
// applyContextInjectionChangedDelta 替换当前选中会话的记忆面板投影。
function applyContextInjectionChangedDelta(
  workbench: WorkbenchSnapshot,
  delta: ContextInjectionChangedDelta,
): WorkbenchSnapshot {
  return deriveSelectedConversationFields({
    ...workbench,
    contextInjectionByConversation: upsertContextInjectionForConversation(
      workbench.contextInjectionByConversation ?? [],
      delta,
    ),
  });
}

// applySubagentPanelRouteResponse applies navigation only; child transcripts belong to independent V1 event windows.
// applySubagentPanelRouteResponse 仅应用导航；子转录归独立 V1 事件窗口所有。
// Parameters: workbench is the latest global snapshot and response is one route-local result.
// 参数：workbench 是最新全局快照，response 是单条路由局部结果。
// Returns: the accepted route projection, or the unchanged newer global projection.
// 返回值：已接受的路由投影，或保持不变的较新全局投影。
export function applySubagentPanelRouteResponse(
  workbench: WorkbenchSnapshot,
  response: SubagentPanelRouteResponse,
): WorkbenchSnapshot {
  // currentRevision is the authoritative global projection revision already visible to the client.
  // currentRevision 是客户端当前可见的权威全局投影版本。
  const currentRevision = workbench.revision ?? 0;
  // responseRevision is the global projection revision captured while this route response was built.
  // responseRevision 是构建该路由响应时捕获的全局投影版本。
  const responseRevision = response.revision ?? 0;
  if (currentRevision > 0 && responseRevision < currentRevision) {
    return workbench;
  }
  // nextNavigationByConversation changes only when the route response carries an authoritative navigation snapshot.
  // nextNavigationByConversation 仅在路由响应携带权威导航快照时发生变化。
  const nextNavigationByConversation = response.subagentNavigation
    ? upsertSubagentNavigationForConversation(
      workbench.subagentNavigationByConversation ?? [],
      response.parentConversationId ?? response.subagentNavigation.parentConversationId,
      response.subagentNavigation,
    )
    : workbench.subagentNavigationByConversation ?? [];
  return deriveSelectedConversationFields({
    ...workbench,
    revision: response.revision,
    subagentNavigationByConversation: nextNavigationByConversation,
  });
}

// applySessionAddonsChangedResponse applies one direct addon-state RPC response.
// applySessionAddonsChangedResponse 应用一次直接附属状态 RPC 响应。
export function applySessionAddonsChangedResponse(
  workbench: WorkbenchSnapshot,
  response: SessionAddonsChangedResponse,
): WorkbenchSnapshot {
  if (!shouldAcceptWorkbenchRevision(workbench.revision ?? 0, response.revision ?? 0)) {
    return workbench;
  }
  return deriveSelectedConversationFields({
    ...workbench,
    revision: response.revision,
    conversations: applySessionAddonAttention(workbench.conversations, response),
    sessionAddonsByConversation: upsertSessionAddonsForConversation(
      workbench.sessionAddonsByConversation ?? [],
      response,
    ),
    sessionVmmPoliciesByConversation: upsertSessionVmmPolicyForConversation(
      workbench.sessionVmmPoliciesByConversation ?? [],
      response,
    ),
  });
}

// mergeConversationSummaries preserves only optimistic local read state while trusting backend lifecycle state.
// mergeConversationSummaries 仅保留本地乐观已读状态，同时信任后端生命周期状态。
function mergeConversationSummaries(
  current: ConversationSummary[],
  incoming: ConversationSummary[],
  selectedConversationId: string | null | undefined,
): ConversationSummary[] {
  const currentById = new Map(current.map((conversation) => [conversation.id, conversation]));
  return incoming.map((conversation) => {
    const currentConversation = currentById.get(conversation.id);
    if (!currentConversation) {
      return conversation;
    }
    const keepUnread = conversation.id === selectedConversationId
      ? false
      : conversation.unread;
    return {
      ...conversation,
      unread: keepUnread,
    };
  });
}

// reorderPendingPromptSummaries applies an optimistic pending-prompt order by id.
// reorderPendingPromptSummaries 根据标识应用乐观待发送提示词排序。
export function reorderPendingPromptSummaries(entries: PendingPromptSummary[], entryIds: string[]): PendingPromptSummary[] {
  const entriesById = new Map(entries.map((entry) => [entry.id, entry]));
  const reordered = entryIds
    .map((entryId) => entriesById.get(entryId))
    .filter((entry): entry is PendingPromptSummary => Boolean(entry));
  const orderedIds = new Set(entryIds);
  return [
    ...reordered,
    ...entries.filter((entry) => !orderedIds.has(entry.id)),
  ];
}

// markConversationLocallyRead clears the unread dot and records the selected conversation locally.
// markConversationLocallyRead 清除未读圆点，并在本地记录当前选中的会话。
export function markConversationLocallyRead(workbench: WorkbenchSnapshot, conversationId: string): WorkbenchSnapshot {
  return deriveSelectedConversationFields({
    ...workbench,
    selectedConversationId: conversationId,
    conversations: workbench.conversations.map((conversation) => (
      conversation.id === conversationId
        ? {
          ...conversation,
          unread: false,
          attention: conversation.attention?.kind === "unread" ? null : conversation.attention,
        }
        : conversation
    )),
  });
}
