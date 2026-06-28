import React from "react";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type { ComposerSubmission } from "@/components/composer/composer-model";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import {
  compactSelectedConversationContext,
  closeSubagentPanel,
  forkConversation,
  getUserPromptEditDraft,
  insertPendingPrompt,
  openSubagentPanel,
  removePendingPrompt,
  reorderPendingPrompts,
  stopCommandSession,
  stopConversationRun,
  stopSubagentTask,
  submitConversationPrompt,
  submitUserPromptEdit,
} from "@/service/conversation-actions-service";
import {
  listWorkbenchModelSecondaryRoutes,
  selectWorkbenchModel,
  selectWorkbenchModelSecondaryRoute,
  selectWorkbenchReasoningEffort,
  selectWorkbenchThinkingDisplayMode,
} from "@/service/settings-actions-service";
import { loadWorkbenchSnapshot } from "@/service/workbench-snapshot-service";
import {
  applySubagentRoutePreference,
  canOpenCachedSubagentSurface,
  closeSubagentSurfaceForConversation,
  injectCachedSubagentSurfaceMessages,
  openSubagentSurfaceForConversation,
  reorderPendingPromptSummaries,
  type InflightSubagentOpenRequest,
  type SubagentRoutePreference,
  type SubagentSurfaceMessageCache,
} from "@/state/workbench-state";
import type {
  ComposerAttachmentSummary,
  ConversationSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  UserPromptEditDraftSummary,
  WorkbenchSnapshot,
} from "@/types";
import type { TranslationKey } from "@/i18n-messages";
import { toast } from "sonner";
import { webuiClickTrace } from "./types";

// WorkbenchConversationActionsOptions groups selected conversation state and async service bridges.
// WorkbenchConversationActionsOptions 归组选中会话状态与异步服务桥接。
interface WorkbenchConversationActionsOptions {
  // Current translation lookup function.
  // 当前翻译查询函数。
  translate: (key: TranslationKey) => string;
  // Currently selected persisted conversation.
  // 当前选中的持久化会话。
  selectedConversation: ConversationSummary | undefined;
  // Current selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Currently selected draft conversation.
  // 当前选中的草稿会话。
  selectedDraftConversation: DraftConversationSelection | null;
  // Updates the local workbench snapshot.
  // 更新本地工作台快照。
  setWorkbenchState: React.Dispatch<React.SetStateAction<WorkbenchSnapshot>>;
  // Merges a service snapshot with local caches.
  // 将服务快照与本地缓存合并。
  mergeWorkbenchWithLocalCaches: (current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => WorkbenchSnapshot;
  // Clears the active local draft after a persisted send succeeds.
  // 持久化发送成功后清理当前本地草稿。
  setDraftConversation: React.Dispatch<React.SetStateAction<DraftConversationSelection | null>>;
  // Updates the selected conversation id after service actions.
  // 服务动作完成后更新当前会话标识。
  setSelectedConversationId: React.Dispatch<React.SetStateAction<string | null>>;
  // Latest selected conversation id for async stale-result guards.
  // 用于异步过期结果防护的最新会话标识。
  selectedConversationIdRef: React.MutableRefObject<string | null>;
  // Latest workbench snapshot for callbacks that should not close over stale state.
  // 供回调读取最新工作台快照，避免闭包过期。
  workbenchStateRef: React.MutableRefObject<WorkbenchSnapshot>;
  // Monotonic subagent route request sequence.
  // 单调递增的子代理路由请求序号。
  subagentNavigationRequestSeqRef: React.MutableRefObject<number>;
  // Last user-selected subagent route preference.
  // 用户最后选择的子代理路由偏好。
  subagentRoutePreferenceRef: React.MutableRefObject<SubagentRoutePreference | null>;
  // In-flight backend open request for subagent surfaces.
  // 正在进行的子代理界面后端打开请求。
  inflightSubagentOpenRequestRef: React.MutableRefObject<InflightSubagentOpenRequest | null>;
  // Cached subagent transcript surfaces for instant route switching.
  // 用于即时路由切换的子代理转录界面缓存。
  subagentSurfaceMessageCacheRef: React.MutableRefObject<SubagentSurfaceMessageCache>;
}

// useWorkbenchConversationActions owns conversation, subagent, pending-prompt, model, edit, and fork callbacks.
// useWorkbenchConversationActions 持有会话、子代理、待发送、模型、编辑与 fork 回调。
export function useWorkbenchConversationActions(options: WorkbenchConversationActionsOptions) {
  const {
    translate,
    selectedConversation,
    selectedConversationId,
    selectedDraftConversation,
    setWorkbenchState,
    mergeWorkbenchWithLocalCaches,
    setDraftConversation,
    setSelectedConversationId,
    selectedConversationIdRef,
    workbenchStateRef,
    subagentNavigationRequestSeqRef,
    subagentRoutePreferenceRef,
    inflightSubagentOpenRequestRef,
    subagentSurfaceMessageCacheRef,
  } = options;
// handleSubmitConversationPrompt sends the composer text through the shared backend runtime.
// handleSubmitConversationPrompt 通过共享后端运行时发送输入区文本。
const handleSubmitConversationPrompt = React.useCallback(async (submission: ComposerSubmission): Promise<boolean> => {
  // submittedSelectionId captures the sidebar choice that owned this submission.
  // submittedSelectionId 捕获发起本次提交时所属的侧栏选择。
  const submittedSelectionId = selectedConversationId;
  const activeProjectId =
    selectedDraftConversation?.projectId
    ?? selectedConversation?.projectId
    ?? null;
  if (!activeProjectId) {
    toast.error("当前会话没有明确关联项目，无法发送。");
    return false;
  }
  const activeConversationId = selectedDraftConversation
    ? null
    : selectedConversation?.id ?? null;
  try {
    const snapshot = await submitConversationPrompt(
      activeProjectId,
      activeConversationId,
      submission.prompt,
      submission.hiddenModelGuidance,
      submission.attachments,
      submission.executionMode,
      submission.reasoningEffort,
      submission.disabledAiCapabilityKinds,
      submission.disabledAiCapabilityTargetSelectors,
      submission.aiCapabilityTargetOrderOverrides,
    );
    if (!snapshot) {
      toast.info("浏览器预览模式下无法发送真实会话。");
      return false;
    }
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    setDraftConversation(null);
    const nextConversationId = snapshot.selectedConversationId ?? activeConversationId;
    if (nextConversationId) {
      if (selectedConversationIdRef.current === submittedSelectionId) {
        setSelectedConversationId(nextConversationId);
      }
    }
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [selectedConversation, selectedConversationId, selectedDraftConversation]);

// handleInsertPendingPrompt routes one queued prompt through the shared backend queue router.
// handleInsertPendingPrompt 通过共享后端队列路由引导单条排队提示词。
const handleInsertPendingPrompt = React.useCallback(async (entryId: string): Promise<void> => {
  webuiClickTrace("pending_prompt.insert.invoke", { entryId });
  try {
    const snapshot = await insertPendingPrompt(entryId);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
  }
}, []);

// handleStopConversationRun stops the active backend conversation stream.
// handleStopConversationRun 停止当前活跃的后端会话流。
const handleStopConversationRun = React.useCallback(async (): Promise<boolean> => {
  webuiClickTrace("conversation.stop.invoke");
  try {
    const snapshot = await stopConversationRun();
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
    return Boolean(snapshot);
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, []);

// handleCompactSelectedConversationContext starts manual compaction from slash commands.
// handleCompactSelectedConversationContext 从斜杠指令启动手动上下文压缩。
const handleCompactSelectedConversationContext = React.useCallback(async (): Promise<boolean> => {
  webuiClickTrace("conversation.compact.invoke");
  try {
    const snapshot = await compactSelectedConversationContext();
    if (!snapshot) {
      toast.info("浏览器预览模式下无法执行真实上下文压缩。");
      return false;
    }
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, []);

// handleOpenSubagentPanel asks the backend to display one child task surface and reports whether it succeeded.
// handleOpenSubagentPanel 请求后端显示单个子任务界面，并返回是否打开成功。
const handleOpenSubagentPanel = React.useCallback(async (parentConversationId: string, taskId: string): Promise<boolean> => {
  const routeKey = `${parentConversationId}:${taskId}`;
  const existingRequest = inflightSubagentOpenRequestRef.current;
  if (canOpenCachedSubagentSurface(
    workbenchStateRef.current,
    subagentSurfaceMessageCacheRef.current,
    parentConversationId,
    taskId,
  )) {
    subagentRoutePreferenceRef.current = { parentConversationId, taskId };
    webuiClickTrace("subagent.open.local_cache", { parentConversationId, taskId, routeKey });
    setWorkbenchState((current) => openSubagentSurfaceForConversation(
      injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
      parentConversationId,
      taskId,
    ));
    return true;
  }
  if (existingRequest?.key === routeKey) {
    subagentRoutePreferenceRef.current = { parentConversationId, taskId };
    webuiClickTrace("subagent.open.coalesced", { parentConversationId, taskId, routeKey });
    setWorkbenchState((current) => openSubagentSurfaceForConversation(
      injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
      parentConversationId,
      taskId,
    ));
    return true;
  }
  const requestSeq = subagentNavigationRequestSeqRef.current + 1;
  subagentNavigationRequestSeqRef.current = requestSeq;
  subagentRoutePreferenceRef.current = { parentConversationId, taskId };
  webuiClickTrace("subagent.open.invoke", { parentConversationId, taskId, requestSeq, routeKey });
  setWorkbenchState((current) => openSubagentSurfaceForConversation(
    injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current),
    parentConversationId,
    taskId,
  ));
  const requestPromise = openSubagentPanel(parentConversationId, taskId);
  inflightSubagentOpenRequestRef.current = { key: routeKey, promise: requestPromise };
  try {
    const snapshot = await requestPromise;
    if (inflightSubagentOpenRequestRef.current?.promise === requestPromise) {
      inflightSubagentOpenRequestRef.current = null;
    }
    if (snapshot) {
      if (subagentNavigationRequestSeqRef.current !== requestSeq) {
        webuiClickTrace("subagent.open.stale_result", {
          parentConversationId,
          taskId,
          requestSeq,
          latestRequestSeq: subagentNavigationRequestSeqRef.current,
        });
        return true;
      }
      setWorkbenchState((current) => applySubagentRoutePreference(
        mergeWorkbenchWithLocalCaches(current, snapshot),
        subagentRoutePreferenceRef.current,
      ));
    }
    webuiClickTrace("subagent.open.result", {
      parentConversationId,
      taskId,
      requestSeq,
      opened: Boolean(snapshot),
      activeConversationId: snapshot?.subagentNavigation?.activeConversationId ?? null,
      activeTaskId: snapshot?.subagentNavigation?.activeTaskId ?? null,
    });
    return Boolean(snapshot);
  } catch (nextError) {
    if (inflightSubagentOpenRequestRef.current?.promise === requestPromise) {
      inflightSubagentOpenRequestRef.current = null;
    }
    const message = nextError instanceof Error ? nextError.message : String(nextError);
    webuiClickTrace("subagent.open.error", { parentConversationId, taskId, message });
    if (message.includes("不可恢复") || message.includes("没有找到可打开的子代理任务")) {
      return false;
    }
    toast.error(message);
    return false;
  }
}, []);

// handleCloseSubagentPanel asks the backend to return to the parent conversation surface.
// handleCloseSubagentPanel 请求后端返回父会话界面。
const handleCloseSubagentPanel = React.useCallback(async (): Promise<void> => {
  const parentConversationId = selectedConversationIdRef.current;
  inflightSubagentOpenRequestRef.current = null;
  const requestSeq = subagentNavigationRequestSeqRef.current + 1;
  subagentNavigationRequestSeqRef.current = requestSeq;
  if (parentConversationId) {
    subagentRoutePreferenceRef.current = { parentConversationId, taskId: null };
    setWorkbenchState((current) => closeSubagentSurfaceForConversation(current, parentConversationId));
  }
  webuiClickTrace("subagent.close.invoke", { parentConversationId, requestSeq });
  try {
    const snapshot = await closeSubagentPanel();
    if (snapshot) {
      if (subagentNavigationRequestSeqRef.current !== requestSeq) {
        webuiClickTrace("subagent.close.stale_result", {
          parentConversationId,
          requestSeq,
          latestRequestSeq: subagentNavigationRequestSeqRef.current,
        });
        return;
      }
      setWorkbenchState((current) => applySubagentRoutePreference(
        mergeWorkbenchWithLocalCaches(current, snapshot),
        subagentRoutePreferenceRef.current,
      ));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
  }
}, []);

// handleStopSubagentTask asks the backend to stop one running child task.
// handleStopSubagentTask 请求后端停止单个运行中的子任务。
const handleStopSubagentTask = React.useCallback(async (parentConversationId: string, taskId: string): Promise<void> => {
  webuiClickTrace("subagent.stop.invoke", { parentConversationId, taskId });
  try {
    const snapshot = await stopSubagentTask(parentConversationId, taskId);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
  }
}, []);

// handleRemovePendingPrompt removes one queued prompt through the shared service queue.
// handleRemovePendingPrompt 通过共享服务队列移除单条排队提示词。
const handleRemovePendingPrompt = React.useCallback(async (entryId: string): Promise<void> => {
  try {
    const snapshot = await removePendingPrompt(entryId);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
  }
}, []);

// handleReorderPendingPrompts persists one visible pending prompt order update.
// handleReorderPendingPrompts 持久化一次可见待发送提示词排序更新。
const handleReorderPendingPrompts = React.useCallback(async (
  entryIds: string[],
  draggedEntryId: string,
  targetEntryId: string | null,
): Promise<void> => {
  setWorkbenchState((current) => ({ ...current, pendingPrompts: reorderPendingPromptSummaries(current.pendingPrompts ?? [], entryIds) }));
  try {
    const snapshot = await reorderPendingPrompts(selectedConversationIdRef.current, draggedEntryId, targetEntryId);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    const snapshot = await loadWorkbenchSnapshot();
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
  }
}, []);

// handleSelectWorkbenchModel persists one model picker choice through the shared service.
// handleSelectWorkbenchModel 通过共享服务持久化单个模型筛选器选择。
const handleSelectWorkbenchModel = React.useCallback(async (choice: ModelPickerChoice): Promise<boolean> => {
  try {
    const snapshot = await selectWorkbenchModel(choice);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success(`当前模型已切换为 ${choice.modelName}（${choice.providerName}）。`);
    }
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, []);

// handleStopCommandSession force-stops one live terminal session from context panels.
// handleStopCommandSession 从上下文面板强制停止单条实时终端会话。
const handleStopCommandSession = React.useCallback(async (commandSessionId: string, ownerSessionId?: string | null): Promise<void> => {
  webuiClickTrace("command_session.stop.invoke", { commandSessionId });
  try {
    const snapshot = await stopCommandSession(commandSessionId, ownerSessionId ?? null);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
  }
}, []);

// handleListWorkbenchModelSecondaryRoutes loads backend route choices for the active model.
// handleListWorkbenchModelSecondaryRoutes 为当前模型加载后端路由选项。
const handleListWorkbenchModelSecondaryRoutes = React.useCallback(async (
  providerId: string,
  modelId: string,
): Promise<ProviderModelSecondaryRouteSummary[]> => {
  try {
    return await listWorkbenchModelSecondaryRoutes(providerId, modelId);
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return [];
  }
}, []);

// handleSelectWorkbenchModelSecondaryRoute persists one backend route choice.
// handleSelectWorkbenchModelSecondaryRoute 持久化单个后端路由选择。
const handleSelectWorkbenchModelSecondaryRoute = React.useCallback(async (
  providerId: string,
  modelId: string,
  route: ProviderModelSecondaryRouteSelection | null,
): Promise<boolean> => {
  try {
    const snapshot = await selectWorkbenchModelSecondaryRoute(providerId, modelId, route);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success(route ? "二级供应商已切换。" : "二级供应商已恢复 Auto。");
    }
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, []);

// handleGetUserPromptEditDraft loads one backend-validated edit draft.
// handleGetUserPromptEditDraft 加载单条后端校验过的编辑草稿。
const handleGetUserPromptEditDraft = React.useCallback(async (
  conversationId: string,
  messageId: string,
): Promise<UserPromptEditDraftSummary | null> => {
  try {
    return await getUserPromptEditDraft(conversationId, messageId);
  } catch (nextError) {
    toast.error(translate("conversation.action.editPromptFailed"));
    return null;
  }
}, [translate]);

// handleSubmitUserPromptEdit submits one edit and merges the replay snapshot.
// handleSubmitUserPromptEdit 提交单次编辑并合并重放后的快照。
const handleSubmitUserPromptEdit = React.useCallback(async (request: {
  conversationId: string;
  messageId: string;
  prompt: string;
  attachments: ComposerAttachmentSummary[];
}): Promise<boolean> => {
  try {
    const snapshot = await submitUserPromptEdit(request);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
    return true;
  } catch (nextError) {
    toast.error(translate("conversation.action.editPromptFailed"));
    return false;
  }
}, [translate]);

// handleForkConversation creates a persisted copy of the current conversation and opens it.
// handleForkConversation 创建当前会话的持久化副本并打开。
const handleForkConversation = React.useCallback(async (conversationId: string, beforeMessageId?: string | null): Promise<boolean> => {
  const sourceConversation = workbenchStateRef.current.conversations.find((conversation) => conversation.id === conversationId);
  const sourceTitle = sourceConversation?.title.trim() || translate("console.newConversation");
  try {
    const snapshot = await forkConversation({
      conversationId,
      title: `${sourceTitle}${translate("conversation.action.forkTitleSuffix")}`,
      beforeMessageId: beforeMessageId ?? null,
    });
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
      toast.success(translate("conversation.action.forkSucceeded"));
    }
    return true;
  } catch (nextError) {
    toast.error(translate("conversation.action.forkFailed"));
    return false;
  }
}, [translate]);

// handleSelectWorkbenchReasoningEffort persists one reasoning-effort choice through the shared service router.
// handleSelectWorkbenchReasoningEffort 通过共享服务路由持久化单个思考档位选择。
const handleSelectWorkbenchReasoningEffort = React.useCallback(async (effort: string): Promise<boolean> => {
  try {
    const snapshot = await selectWorkbenchReasoningEffort(effort);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [mergeWorkbenchWithLocalCaches, setWorkbenchState]);

// handleSelectWorkbenchThinkingDisplayMode persists one thinking display preference through the shared service router.
// handleSelectWorkbenchThinkingDisplayMode 通过共享服务路由持久化单个思考展示偏好。
const handleSelectWorkbenchThinkingDisplayMode = React.useCallback(async (mode: ThinkingDisplayMode): Promise<boolean> => {
  try {
    const snapshot = await selectWorkbenchThinkingDisplayMode(mode);
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [mergeWorkbenchWithLocalCaches, setWorkbenchState]);


  return {
    handleSubmitConversationPrompt,
    handleInsertPendingPrompt,
    handleStopConversationRun,
    handleCompactSelectedConversationContext,
    handleOpenSubagentPanel,
    handleCloseSubagentPanel,
    handleStopSubagentTask,
    handleRemovePendingPrompt,
    handleReorderPendingPrompts,
    handleSelectWorkbenchModel,
    handleStopCommandSession,
    handleListWorkbenchModelSecondaryRoutes,
    handleSelectWorkbenchModelSecondaryRoute,
    handleGetUserPromptEditDraft,
    handleSubmitUserPromptEdit,
    handleForkConversation,
    handleSelectWorkbenchReasoningEffort,
    handleSelectWorkbenchThinkingDisplayMode,
  };
}
