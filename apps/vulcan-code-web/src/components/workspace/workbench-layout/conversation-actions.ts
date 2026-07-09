import React from "react";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type { ComposerSubmission } from "@/components/composer/composer-model";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import {
  compactSelectedConversationContext,
  closeSubagentTask,
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
  type StopConversationRunMode,
  type StopConversationRunResponse,
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
  canOpenCachedSubagentSurface,
  applySubagentPanelRouteResponse,
  injectCachedSubagentSurfaceMessages,
  reorderPendingPromptSummaries,
  type InflightSubagentOpenRequest,
  type SubagentSurfaceMessageCache,
  type WorkbenchMergeOptions,
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
import {
  webuiClickTrace,
  type RequestStopConversationRunChoice,
  type StopConversationRunAgentPromptItem,
  type StopConversationRunChoice,
} from "./types";

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
  mergeWorkbenchWithLocalCaches: (
    current: WorkbenchSnapshot,
    incoming: WorkbenchSnapshot,
    options?: WorkbenchMergeOptions,
  ) => WorkbenchSnapshot;
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
  // Asks the user whether running child agents should be stopped together.
  // 询问用户是否要同时停止运行中的智能体。
  requestStopConversationRunChoice: RequestStopConversationRunChoice;
  // Monotonic subagent route request sequence.
  // 单调递增的子代理路由请求序号。
  subagentNavigationRequestSeqRef: React.MutableRefObject<number>;
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
    requestStopConversationRunChoice,
    subagentNavigationRequestSeqRef,
    inflightSubagentOpenRequestRef,
    subagentSurfaceMessageCacheRef,
  } = options;
  // conversationForkInFlightRef synchronously gates duplicate fork submissions before React state commits.
  // conversationForkInFlightRef 在 React 状态提交前同步拦截重复分叉提交。
  const conversationForkInFlightRef = React.useRef(false);
  // conversationForkInProgress drives the workspace-level blocking progress overlay.
  // conversationForkInProgress 驱动工作台级阻塞进度遮罩。
  const [conversationForkInProgress, setConversationForkInProgress] = React.useState(false);
// handleSubmitConversationPrompt sends the composer text through the shared backend runtime.
// handleSubmitConversationPrompt 通过共享后端运行时发送输入区文本。
const handleSubmitConversationPrompt = React.useCallback(async (submission: ComposerSubmission): Promise<boolean> => {
  // submittedSelectionId captures the sidebar choice that owned this submission.
  // submittedSelectionId 捕获发起本次提交时所属的侧栏选择。
  const submittedSelectionId = selectedConversationId;
  const fallbackProject = workbenchStateRef.current.projects[0] ?? null;
  const activeProjectId =
    selectedDraftConversation?.projectId
    ?? selectedConversation?.projectId
    ?? fallbackProject?.id
    ?? null;
  if (!activeProjectId) {
    toast.error(translate("conversationActions.noProjectForSend"));
    return false;
  }
  const activeConversationId = selectedDraftConversation
    ? null
    : selectedConversation?.id ?? null;
  try {
    const ack = await submitConversationPrompt(
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
    if (!ack) {
      toast.info(translate("conversationActions.previewCannotSend"));
      return false;
    }
    setDraftConversation(null);
    const nextConversationId = ack.conversationId || activeConversationId;
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
}, [selectedConversation, selectedConversationId, selectedDraftConversation, setDraftConversation, setSelectedConversationId, translate]);

// handleInsertPendingPrompt routes one queued prompt through the shared backend queue router.
// handleInsertPendingPrompt 通过共享后端队列路由引导单条排队提示词。
const handleInsertPendingPrompt = React.useCallback(async (entryId: string): Promise<void> => {
  webuiClickTrace("pending_prompt.insert.invoke", { entryId });
  try {
    await insertPendingPrompt(entryId);
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
  }
}, []);

// handleStopConversationRun stops the active backend conversation stream.
// handleStopConversationRun 停止当前活跃的后端会话流。
const handleStopConversationRun = React.useCallback(async (): Promise<boolean> => {
  webuiClickTrace("conversation.stop.invoke");
  try {
    let response = await stopConversationRun("ask_if_running_subagents");
    if (!response) {
      return false;
    }
    if (response.status === "needs_confirmation") {
      const choice = await requestStopConversationRunChoice(
        stopConversationRunPromptItems(response),
      );
      if (!choice) {
        return false;
      }
      response = await stopConversationRun(stopConversationRunModeFromChoice(choice));
      if (!response) {
        return false;
      }
    }
    if (response.status === "stopped") {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, response.workbench));
      return true;
    }
    return false;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [mergeWorkbenchWithLocalCaches, requestStopConversationRunChoice, setWorkbenchState]);

// handleCompactSelectedConversationContext starts manual compaction from slash commands.
// handleCompactSelectedConversationContext 从斜杠指令启动手动上下文压缩。
const handleCompactSelectedConversationContext = React.useCallback(async (): Promise<boolean> => {
  webuiClickTrace("conversation.compact.invoke");
  try {
    const snapshot = await compactSelectedConversationContext();
    if (!snapshot) {
      toast.info(translate("conversationActions.previewCannotCompact"));
      return false;
    }
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [translate]);

// handleOpenSubagentPanel loads one child transcript without replacing the parent conversation surface.
// handleOpenSubagentPanel 加载单个子任务转录，但不替换父会话主界面。
const handleOpenSubagentPanel = React.useCallback(async (parentConversationId: string, taskId: string): Promise<boolean> => {
  const routeKey = `${parentConversationId}:${taskId}`;
  const existingRequest = inflightSubagentOpenRequestRef.current;
  if (canOpenCachedSubagentSurface(
    workbenchStateRef.current,
    subagentSurfaceMessageCacheRef.current,
    parentConversationId,
    taskId,
  )) {
    webuiClickTrace("subagent.open.local_cache", { parentConversationId, taskId, routeKey });
    setWorkbenchState((current) => injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current));
    return true;
  }
  if (existingRequest?.key === routeKey) {
    webuiClickTrace("subagent.open.coalesced", { parentConversationId, taskId, routeKey });
    setWorkbenchState((current) => injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current));
    try {
      // Coalesced opens must wait for the original route load so empty child surfaces are not shown as loaded.
      // 合并打开必须等待原始路由加载完成，避免空子界面被误显示为已载入。
      const response = await existingRequest.promise;
      if (response) {
        if (subagentNavigationRequestSeqRef.current !== existingRequest.requestSeq) {
          webuiClickTrace("subagent.open.coalesced_stale_result", {
            parentConversationId,
            taskId,
            requestSeq: existingRequest.requestSeq,
            latestRequestSeq: subagentNavigationRequestSeqRef.current,
          });
          return false;
        }
        setWorkbenchState((current) => applySubagentPanelRouteResponse(
          current,
          response,
          subagentSurfaceMessageCacheRef.current,
        ));
      }
      webuiClickTrace("subagent.open.coalesced_result", {
        parentConversationId,
        taskId,
        routeKey,
        opened: Boolean(response),
        activeConversationId: response?.activeConversationId ?? null,
        activeTaskId: response?.subagentNavigation?.activeTaskId ?? null,
      });
      return Boolean(response);
    } catch (nextError) {
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      webuiClickTrace("subagent.open.coalesced_error", { parentConversationId, taskId, message });
      return false;
    }
  }
  const requestSeq = subagentNavigationRequestSeqRef.current + 1;
  subagentNavigationRequestSeqRef.current = requestSeq;
  webuiClickTrace("subagent.open.invoke", { parentConversationId, taskId, requestSeq, routeKey });
  setWorkbenchState((current) => injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current));
  const requestPromise = openSubagentPanel(parentConversationId, taskId);
  inflightSubagentOpenRequestRef.current = { key: routeKey, requestSeq, promise: requestPromise };
  try {
    const response = await requestPromise;
    if (inflightSubagentOpenRequestRef.current?.promise === requestPromise) {
      inflightSubagentOpenRequestRef.current = null;
    }
    if (response) {
      if (subagentNavigationRequestSeqRef.current !== requestSeq) {
        webuiClickTrace("subagent.open.stale_result", {
          parentConversationId,
          taskId,
          requestSeq,
          latestRequestSeq: subagentNavigationRequestSeqRef.current,
        });
        return false;
      }
      setWorkbenchState((current) => applySubagentPanelRouteResponse(
        current,
        response,
        subagentSurfaceMessageCacheRef.current,
      ));
    }
    webuiClickTrace("subagent.open.result", {
      parentConversationId,
      taskId,
      requestSeq,
      opened: Boolean(response),
      activeConversationId: response?.activeConversationId ?? null,
      activeTaskId: response?.subagentNavigation?.activeTaskId ?? null,
    });
    return Boolean(response);
  } catch (nextError) {
    if (inflightSubagentOpenRequestRef.current?.promise === requestPromise) {
      inflightSubagentOpenRequestRef.current = null;
    }
    const message = nextError instanceof Error ? nextError.message : String(nextError);
    webuiClickTrace("subagent.open.error", { parentConversationId, taskId, message });
    if (message.includes("不可恢复") || message.includes("没有找到可打开的智能体任务")) {
      return false;
    }
    toast.error(message);
    return false;
  }
}, []);

// handleStopSubagentTask asks the backend to stop one running child task.
// handleStopSubagentTask 请求后端停止单个运行中的子任务。
const handleStopSubagentTask = React.useCallback(async (parentConversationId: string, taskId: string): Promise<void> => {
  webuiClickTrace("subagent.stop.invoke", { parentConversationId, taskId });
  try {
    const response = await stopSubagentTask(parentConversationId, taskId);
    if (response) {
      setWorkbenchState((current) => applySubagentPanelRouteResponse(
        current,
        response,
        subagentSurfaceMessageCacheRef.current,
      ));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
  }
}, []);

// handleCloseSubagentTask asks the backend to close one open child agent lifecycle.
// handleCloseSubagentTask 请求后端关闭单个未关闭子智能体生命周期。
const handleCloseSubagentTask = React.useCallback(async (parentConversationId: string, taskId: string): Promise<void> => {
  webuiClickTrace("subagent.close.invoke", { parentConversationId, taskId });
  try {
    const response = await closeSubagentTask(parentConversationId, taskId);
    if (response) {
      setWorkbenchState((current) => applySubagentPanelRouteResponse(
        current,
        response,
        subagentSurfaceMessageCacheRef.current,
      ));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    throw nextError;
  }
}, []);

// handleRemovePendingPrompt removes one queued prompt through the shared service queue.
// handleRemovePendingPrompt 通过共享服务队列移除单条排队提示词。
const handleRemovePendingPrompt = React.useCallback(async (entryId: string): Promise<void> => {
  try {
    await removePendingPrompt(entryId);
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
  const conversationId = selectedConversationIdRef.current;
  setWorkbenchState((current) => {
    const pendingPrompts = reorderPendingPromptSummaries(current.pendingPrompts ?? [], entryIds);
    const pendingPromptsByConversation = (current.pendingPromptsByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId);
    if (conversationId && pendingPrompts.length > 0) {
      pendingPromptsByConversation.push({ conversationId, pendingPrompts });
    }
    return { ...current, pendingPrompts, pendingPromptsByConversation };
  });
  try {
    await reorderPendingPrompts(conversationId, draggedEntryId, targetEntryId);
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
      toast.success(formatConversationActionTranslation(translate, "conversationActions.modelSwitched", {
        model: choice.modelName,
        provider: choice.providerName,
      }));
    }
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [mergeWorkbenchWithLocalCaches, setWorkbenchState, translate]);

// handleStopCommandSession force-stops one live terminal session from context panels.
// handleStopCommandSession 从上下文面板强制停止单条实时终端会话。
const handleStopCommandSession = React.useCallback(async (commandSessionId: string, ownerSessionId?: string | null): Promise<void> => {
  webuiClickTrace("command_session.stop.invoke", { commandSessionId });
  try {
    await stopCommandSession(commandSessionId, ownerSessionId ?? null);
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
      toast.success(translate(route ? "composer.secondaryRoute.switched" : "composer.secondaryRoute.restoredAuto"));
    }
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [mergeWorkbenchWithLocalCaches, setWorkbenchState, translate]);

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

// handleSubmitUserPromptEdit submits one edit and lets backend deltas update the replayed turn.
// handleSubmitUserPromptEdit 提交单次编辑，并由后端增量更新重放后的回合。
const handleSubmitUserPromptEdit = React.useCallback(async (request: {
  conversationId: string;
  messageId: string;
  prompt: string;
  attachments: ComposerAttachmentSummary[];
}): Promise<boolean> => {
  try {
    const ack = await submitUserPromptEdit(request);
    if (ack && selectedConversationIdRef.current === request.conversationId) {
      setSelectedConversationId(ack.conversationId);
    }
    return true;
  } catch (nextError) {
    toast.error(translate("conversation.action.editPromptFailed"));
    return false;
  }
}, [selectedConversationIdRef, setSelectedConversationId, translate]);

// handleForkConversation creates a persisted copy of the current conversation and opens it.
// handleForkConversation 创建当前会话的持久化副本并打开。
const handleForkConversation = React.useCallback(async (conversationId: string, anchorMessageId?: string): Promise<boolean> => {
  if (conversationForkInFlightRef.current) {
    return false;
  }
  conversationForkInFlightRef.current = true;
  setConversationForkInProgress(true);
  const sourceConversation = workbenchStateRef.current.conversations.find((conversation) => conversation.id === conversationId);
  const sourceTitle = sourceConversation?.title.trim() || translate("console.newConversation");
  try {
    const snapshot = await forkConversation({
      conversationId,
      title: `${sourceTitle}${translate("conversation.action.forkTitleSuffix")}`,
      anchorMessageId,
    });
    if (snapshot) {
      // Validate the service-selected fork before mirroring it into local sidebar state.
      // 写入本地侧栏状态前校验服务端选中的分叉会话仍在会话列表中。
      const forkedConversationId = snapshot.conversations.some((conversation) => (
        conversation.id === snapshot.selectedConversationId
      ))
        ? snapshot.selectedConversationId
        : null;
      if (forkedConversationId) {
        setSelectedConversationId(forkedConversationId);
      }
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(
        current,
        snapshot,
        { selectedConversationSource: "incoming" },
      ));
      toast.success(translate("conversation.action.forkSucceeded"));
    }
    return true;
  } catch (nextError) {
    const errorMessage = nextError instanceof Error ? nextError.message.trim() : "";
    toast.error(
      errorMessage
        ? `${translate("conversation.action.forkFailed")}：${errorMessage}`
        : translate("conversation.action.forkFailed"),
    );
    return false;
  } finally {
    conversationForkInFlightRef.current = false;
    setConversationForkInProgress(false);
  }
}, [mergeWorkbenchWithLocalCaches, setSelectedConversationId, setWorkbenchState, translate, workbenchStateRef]);

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
    handleStopSubagentTask,
    handleCloseSubagentTask,
    handleRemovePendingPrompt,
    handleReorderPendingPrompts,
    handleSelectWorkbenchModel,
    handleStopCommandSession,
    handleListWorkbenchModelSecondaryRoutes,
    handleSelectWorkbenchModelSecondaryRoute,
    handleGetUserPromptEditDraft,
    handleSubmitUserPromptEdit,
    handleForkConversation,
    conversationForkInProgress,
    handleSelectWorkbenchReasoningEffort,
    handleSelectWorkbenchThinkingDisplayMode,
  };
}

// stopConversationRunPromptItems normalizes backend running-agent payloads for the dialog.
// stopConversationRunPromptItems 将后端运行中智能体载荷归一化给弹窗使用。
function stopConversationRunPromptItems(
  response: Extract<StopConversationRunResponse, { status: "needs_confirmation" }>,
): StopConversationRunAgentPromptItem[] {
  return response.running_subagents.map((agent) => ({
    taskId: agent.task_id,
    label: agent.label || agent.task_id,
  }));
}

// stopConversationRunModeFromChoice converts a UI decision into the backend stop mode.
// stopConversationRunModeFromChoice 将界面决策转换为后端停止模式。
function stopConversationRunModeFromChoice(choice: StopConversationRunChoice): StopConversationRunMode {
  return choice === "main_and_subagents" ? "main_and_subagents" : "main_only";
}

// formatConversationActionTranslation replaces named placeholders in localized workbench action messages.
// formatConversationActionTranslation 替换工作台动作本地化消息中的具名占位符。
function formatConversationActionTranslation(
  translate: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    translate(key),
  );
}
