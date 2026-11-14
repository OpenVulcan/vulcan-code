import React from "react";
import { completeClientOwnedRetryAction, createClientOwnedActionId, retainClientOwnedRetryActionId } from "@/notifications/client-identity";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type { ComposerSubmission } from "@/components/composer/composer-model";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { moveConversationToolCollapseOverride } from "@/components/conversation/tool-collapse-preferences";
import { moveConversationThinkingPopupOverride } from "@/components/conversation/thinking-display-preferences";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import { materializeAcknowledgedDraftSubmission } from "@/components/workspace/draft-submission-state";
import {
  cancelConversationSubmission,
  compactSelectedConversationContext,
  closeSubagentTask,
  insertPendingPrompt,
  openSubagentPanel,
  removePendingPrompt,
  reorderPendingPrompts,
  setSessionContextCompactionWindow,
  continueConversation,
  editConversationGenerationV1,
  forkConversationGenerationV1,
  stopCommandSession,
  stopConversationRun,
  stopSubagentTask,
  submitConversationPrompt,
  type StopConversationRunMode,
  type StopConversationRunResponse,
} from "@/service/conversation-actions-service";
import {
  createReasoningRecoverySession,
  listWorkbenchModelSecondaryRoutes,
  selectWorkbenchModel,
  selectWorkbenchModelSecondaryRoute,
  selectWorkbenchReasoningEffort,
  selectWorkbenchThinkingDisplayMode,
  saveToolCollapseDefault,
} from "@/service/settings-actions-service";
import { BrowserServiceRpcError } from "@/service/browserTransport";
import { loadWorkbenchSnapshot } from "@/service/workbench-snapshot-service";
import { modelContextWindowOffer, showModelContextWindowOffer } from "./model-context-window-offer";
import {
  applySessionAddonsChangedResponse,
  applySubagentPanelRouteResponse,
  reorderPendingPromptSummaries,
  type WorkbenchMergeOptions,
} from "@/state/workbench-state";
import type {
  ConversationSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  SubagentPanelRouteResponse,
  WorkbenchSnapshot,
} from "@/types";
import type { TranslationKey } from "@/i18n-messages";
import {
  conversationContinuationBlockedReasonText,
  conversationContinuationBlockedReasonTextFromError,
} from "@/components/conversation/continuation-blocked-reason";
import { toast } from "@/ui-kit/primitives/toast";
import {
  webuiClickTrace,
  type RequestStopConversationRunChoice,
  type StopConversationRunAgentPromptItem,
  type StopConversationRunChoice,
} from "./types";

// SubagentOpenRequestMap owns one shared backend transcript request per parent/task route.
// SubagentOpenRequestMap 为每条父级/任务路由持有一个共享后端转录请求。
type SubagentOpenRequestMap = Map<string, Promise<SubagentPanelRouteResponse | null>>;

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
  // Persisted conversation id whose draft-to-session sidebar projection is still converging.
  // 草稿转为持久化会话后仍在等待侧栏投影收敛的会话标识。
  pendingMaterializedConversationIdRef: React.MutableRefObject<string | null>;
  // Latest workbench snapshot for callbacks that should not close over stale state.
  // 供回调读取最新工作台快照，避免闭包过期。
  workbenchStateRef: React.MutableRefObject<WorkbenchSnapshot>;
  // Asks the user whether running child agents should be stopped together.
  // 询问用户是否要同时停止运行中的智能体。
  requestStopConversationRunChoice: RequestStopConversationRunChoice;
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
    pendingMaterializedConversationIdRef,
    workbenchStateRef,
    requestStopConversationRunChoice,
  } = options;
  // inflightSubagentOpenRequestsRef coalesces duplicate opens without serializing independent side-panel tabs.
  // inflightSubagentOpenRequestsRef 合并重复打开请求，但不串行化彼此独立的侧栏页签。
  const inflightSubagentOpenRequestsRef = React.useRef<SubagentOpenRequestMap>(new Map());
  // Pending recovery offers retain their action identities when the user reselects the same route.
  // 未完成的恢复提示在用户重新选择同一路由时保留操作身份。
  const recoveryActionsRef = React.useRef(new Map<string, { actionId: string; running: boolean }>());
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
  // clientSubmissionId correlates the optimistic first message with the authoritative service acknowledgement.
  // clientSubmissionId 将乐观首条消息与权威服务确认关联起来。
  const clientSubmissionId = createClientOwnedActionId();
  // pendingStageTimer advances the bounded user-facing status when acknowledgement is not immediate.
  // pendingStageTimer 在确认未即时返回时推进有界的用户可见状态。
  const pendingStageTimer = selectedDraftConversation
    ? window.setTimeout(() => {
        setDraftConversation((current) => current?.id === selectedDraftConversation.id
          && current.pendingSubmission?.clientSubmissionId === clientSubmissionId
          ? {
              ...current,
              pendingSubmission: {
                ...current.pendingSubmission,
                stage: "preparing",
              },
            }
          : current);
      }, 500)
    : null;
  if (selectedDraftConversation) {
    setDraftConversation((current) => current?.id === selectedDraftConversation.id
      ? {
          ...current,
          pendingSubmission: {
            clientSubmissionId,
            prompt: submission.prompt,
            attachments: submission.attachments,
            startedAt: Date.now(),
            stage: "creating",
          },
        }
      : current);
  }
  try {
    const ack = await submitConversationPrompt(
      clientSubmissionId,
      activeProjectId,
      activeConversationId,
      submission.draftSlotKey,
      submission.prompt,
      submission.hiddenModelGuidance,
      submission.attachments,
      submission.executionMode,
      submission.reasoningEffort,
      submission.modelSelection,
      submission.sessionSettings,
      submission.droppedPlanId,
    );
    if (!ack) {
      if (selectedDraftConversation) {
        setDraftConversation((current) => current?.id === selectedDraftConversation.id
          && current.pendingSubmission?.clientSubmissionId === clientSubmissionId
          ? { ...current, pendingSubmission: null }
          : current);
      }
      toast.info(translate("conversationActions.previewCannotSend"));
      return false;
    }
    if (ack.clientSubmissionId !== clientSubmissionId) {
      throw new Error("Service acknowledgement did not match the active submission.");
    }
    if (ack.queuedTurnDispatch === "waiting") {
      toast.info(translate("conversationActions.acceptedAwaitingDispatch"));
    } else if (ack.queuedTurnDispatch === "failed") {
      toast.error(translate("conversationActions.acceptedDispatchFailed"));
    }
    if (submission.insertIntoCurrentTurn && ack.queued) {
      // Use the acknowledged owner and action even if the user has since changed the selected conversation.
      // 即使用户后来切换了会话，也使用服务已确认的会话和消息标识。
      try {
        if (!await insertPendingPrompt(ack.clientSubmissionId, ack.conversationId)) {
          toast.error(translate("conversationActions.insertUnconfirmed"));
        }
      } catch (error) {
        // Admission already succeeded. Reporting an insertion failure must never restore or duplicate the draft.
        // 提交已经成功。报告插队失败时不能恢复或重复发送草稿。
        toast.error(translate("conversationActions.insertUnconfirmed"), {
          description: error instanceof Error ? error.message : String(error),
        });
      }
    }
    const nextConversationId = ack.conversationId || activeConversationId;
    if (nextConversationId) {
      // A successful draft submit creates the persisted id before its ordered sidebar delta is rendered.
      // 草稿提交成功后，持久化标识会先于有序侧栏增量完成渲染。
      if (selectedDraftConversation?.id === submittedSelectionId) {
        moveConversationToolCollapseOverride(submittedSelectionId, nextConversationId);
        moveConversationThinkingPopupOverride(submittedSelectionId, nextConversationId);
        // materializedConversationAlreadyVisible closes the transition immediately when the delta won the race.
        // materializedConversationAlreadyVisible 在增量先到达时立即结束转正过渡。
        const materializedConversationAlreadyVisible = workbenchStateRef.current.conversations
          .some((conversation) => conversation.id === nextConversationId);
        pendingMaterializedConversationIdRef.current = materializedConversationAlreadyVisible
          ? null
          : nextConversationId;
        setDraftConversation((current) => materializeAcknowledgedDraftSubmission(
          current,
          submittedSelectionId,
          clientSubmissionId,
          nextConversationId,
        ));
      }
      setSelectedConversationId((current) => (
        current === submittedSelectionId ? nextConversationId : current
      ));
    }
    return true;
  } catch (nextError) {
    if (selectedDraftConversation) {
      setDraftConversation((current) => current?.id === selectedDraftConversation.id
        && current.pendingSubmission?.clientSubmissionId === clientSubmissionId
        ? { ...current, pendingSubmission: null }
        : current);
    }
    const errorMessage = nextError instanceof Error ? nextError.message : String(nextError);
    if (!errorMessage.startsWith("submission_cancelled:")) {
      // blockedSettlementReason localizes a durable terminal settlement that still blocks admission;
      // an unreadable or unsettled boundary must never reach the user as a raw machine diagnostic.
      // blockedSettlementReason 本地化仍然阻塞准入的可靠终态结算；
      // 无法读取或未完成的边界绝不能以原始机器诊断的形式暴露给用户。
      const blockedSettlementReason = conversationContinuationBlockedReasonTextFromError(
        errorMessage,
        translate,
      );
      toast.error(
        blockedSettlementReason
          ? `${translate("conversation.continuation.blocked")}${translate("global.labelSeparator")}${blockedSettlementReason}`
          : errorMessage,
      );
    }
    return false;
  } finally {
    if (pendingStageTimer !== null) {
      window.clearTimeout(pendingStageTimer);
    }
  }
}, [
  pendingMaterializedConversationIdRef,
  selectedConversation,
  selectedConversationId,
  selectedDraftConversation,
  setDraftConversation,
  setSelectedConversationId,
  translate,
]);

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
  // conversationId freezes the stop target across preflight, user confirmation, and final mutation.
  // conversationId 在预检、用户确认与最终变更之间固定停止目标。
  const conversationId = selectedConversation?.id ?? null;
  if (!conversationId) {
    return false;
  }
  try {
    let response = await stopConversationRun(conversationId, "ask_if_running_subagents");
    if (!response) {
      return false;
    }
    if (response.status === "needs_confirmation") {
      const choice = await requestStopConversationRunChoice(
        conversationId,
        stopConversationRunPromptItems(response),
      );
      if (!choice) {
        return false;
      }
      response = await stopConversationRun(conversationId, stopConversationRunModeFromChoice(choice));
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
}, [mergeWorkbenchWithLocalCaches, requestStopConversationRunChoice, selectedConversation?.id, setWorkbenchState]);

// continuationRequestTimeoutMs exceeds the backend durable transport-observation deadline.
// continuationRequestTimeoutMs 长于后端可靠传输观察等待期限。
const continuationRequestTimeoutMs = 130_000;

// continuationErrorMessage maps stable Continue failures to persistent user-facing feedback.
// continuationErrorMessage 将稳定 Continue 错误映射为用户可见反馈。
//
// Parameters: error is the service or transport failure and translate resolves the active locale.
// 参数：error 是服务或传输错误，translate 解析当前语言。
// Returns: localized user-facing text that never relies on a raw machine code alone.
// 返回值：不只依赖原始机器错误码的本地化用户文案。
function continuationErrorMessage(error: unknown, translate: (key: TranslationKey) => string): string {
  // message is the stable diagnostic text used only for exact error classification.
  // message 是仅用于精确错误分类的稳定诊断文本。
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("ERR_CONVERSATION_CONTINUE_TIMEOUT")) {
    return translate("conversation.continuation.timeout");
  }
  if (message.includes("ERR_CONVERSATION_CONTINUE_ACK_MISMATCH")) {
    return translate("conversation.continuation.ackMismatch");
  }
  if (
    message.includes("ERR_CONTROLLER_RUNTIME_NOT_READY")
    || message.includes("ERR_WORKSPACE_SNAPSHOT_NOT_READY")
  ) {
    return conversationContinuationBlockedReasonText(
      "ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING",
      translate,
    );
  }
  // blockedReason is the concrete localized safety explanation carried by a stable backend reason.
  // blockedReason 是稳定后端原因对应的具体本地化安全说明。
  const blockedReason = conversationContinuationBlockedReasonTextFromError(message, translate);
  if (blockedReason) {
    return `${translate("conversation.continuation.blocked")}${translate("global.labelSeparator")}${blockedReason}`;
  }
  return `${translate("conversation.continuation.claimFailed")} (${message})`;
}

// handleContinueConversation continues the exact V1 open Turn and waits for durable transport observation.
// handleContinueConversation 继续精确的 V1 开放 Turn，并等待可靠传输观察事实。
const handleContinueConversation = React.useCallback(async (
  conversationId: string,
  clientContinuationId: string,
  supplement: ComposerSubmission | null,
  queuedInputActionId?: string,
): Promise<boolean> => {
  // controller owns only the bounded RPC wait for the durable transport observation.
  // controller 只控制等待可靠传输观察事实的有界 RPC。
  const controller = new AbortController();
  // timedOut distinguishes a local timeout from an immediate service rejection.
  // timedOut 区分本地超时与服务立即拒绝。
  let timedOut = false;
  // timeoutId guarantees the transport claim cannot lock the Continue button indefinitely.
  // timeoutId 保证传输领取不会无限期锁定 Continue 按钮。
  const timeoutId = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, continuationRequestTimeoutMs);
  // response remains null until an identity-validated service ACK is received.
  // response 在收到通过身份校验的服务 ACK 前保持为空。
  let response: Awaited<ReturnType<typeof continueConversation>> = null;
  try {
    response = await continueConversation(
      conversationId,
      clientContinuationId,
      supplement,
      controller.signal,
      queuedInputActionId,
    );
    if (!response) {
      toast.info(translate("conversationActions.previewCannotContinue"));
      return false;
    }
    if (
      response.clientSubmissionId !== clientContinuationId
      || response.conversationId !== conversationId
    ) {
      throw new Error("ERR_CONVERSATION_CONTINUE_ACK_MISMATCH");
    }
    if (response.queuedTurnDispatch === "waiting") {
      toast.info(translate("conversationActions.acceptedAwaitingDispatch"));
    } else if (response.queuedTurnDispatch === "failed") {
      toast.error(translate("conversationActions.acceptedDispatchFailed"));
      return false;
    }
  } catch (nextError) {
    // normalizedError gives a timed-out transport one stable user-facing classification.
    // normalizedError 为超时传输提供一个稳定的用户可见分类。
    const normalizedError = timedOut
      ? new Error("ERR_CONVERSATION_CONTINUE_TIMEOUT")
      : nextError;
    toast.error(continuationErrorMessage(normalizedError, translate));
    return false;
  } finally {
    clearTimeout(timeoutId);
  }
  return response !== null;
}, [translate]);

// handleEditConversationGeneration replaces one exact root input through the V1 generation service.
// handleEditConversationGeneration 通过 V1 generation 服务替换一条精确根输入。
const handleEditConversationGeneration = React.useCallback(async (
  sessionId: string,
  expectedWindowGeneration: number,
  targetSeq: number,
  replacementText: string,
): Promise<boolean> => {
  try {
    // response is accepted only when every immutable request identity is echoed exactly.
    // 仅当全部不可变请求身份都被精确回显时才接受响应。
    const response = await editConversationGenerationV1({
      sessionId,
      expectedWindowGeneration,
      targetSeq,
      replacementText,
      clientActionId: createClientOwnedActionId(),
    });
    if (!response) {
      toast.info(translate("conversationActions.previewCannotSend"));
      return false;
    }
    if (
      response.sessionId !== sessionId
      || response.targetSeq !== targetSeq
      || !response.dispatchStarted
    ) {
      throw new Error("ERR_CONVERSATION_EDIT_GENERATION_ACK_MISMATCH");
    }
    return true;
  } catch (nextError) {
    const message = nextError instanceof Error ? nextError.message : String(nextError);
    toast.error(`${translate("conversation.action.editPromptFailed")}: ${message}`);
    return false;
  }
}, [translate]);

// handleForkConversationGeneration creates one immutable V1 child and selects it after index refresh.
// handleForkConversationGeneration 创建一个不可变 V1 子会话，并在索引刷新后选中它。
const handleForkConversationGeneration = React.useCallback(async (
  sourceSessionId: string,
  expectedWindowGeneration: number,
  sourceTurnEndSeq: number,
  childSessionId: string,
  title: string,
): Promise<string | null> => {
  try {
    // response must preserve the exact parent, child, and completed-Turn identities.
    // 响应必须保留精确的父会话、子会话与完整回合身份。
    const response = await forkConversationGenerationV1({
      sourceSessionId,
      expectedWindowGeneration,
      sourceTurnEndSeq,
      childSessionId,
      title,
    });
    if (!response) {
      toast.info(translate("conversationActions.previewCannotSend"));
      return null;
    }
    if (
      response.sourceSessionId !== sourceSessionId
      || response.childSessionId !== childSessionId
      || response.sourceTurnEndSeq !== sourceTurnEndSeq
    ) {
      throw new Error("ERR_CONVERSATION_FORK_GENERATION_ACK_MISMATCH");
    }
    // snapshot refresh makes the SQLite projection visible before sidebar selection changes.
    // 快照刷新会在侧栏选择变化前让 SQLite 投影可见。
    const snapshot = await loadWorkbenchSnapshot();
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    if (selectedConversationIdRef.current === sourceSessionId) {
      setSelectedConversationId(childSessionId);
    }
    toast.success(translate("conversation.action.forkSucceeded"));
    return childSessionId;
  } catch (nextError) {
    const message = nextError instanceof Error ? nextError.message : String(nextError);
    toast.error(`${translate("conversation.action.forkFailed")}: ${message}`);
    return null;
  }
}, [
  mergeWorkbenchWithLocalCaches,
  selectedConversationIdRef,
  setSelectedConversationId,
  setWorkbenchState,
  translate,
]);

// handleCancelConversationSubmission aborts a pending upload before the backend materializes the prompt.
// handleCancelConversationSubmission 在后端物化提示词前中止待处理上传。
const handleCancelConversationSubmission = React.useCallback(async (clientSubmissionId: string): Promise<boolean> => {
  try {
    return await cancelConversationSubmission(clientSubmissionId);
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

// handleOpenSubagentPanel loads one child transcript and coalesces only callers targeting the same tab route.
// handleOpenSubagentPanel 加载单个子任务转录，并且只合并指向同一页签路由的调用方。
const handleOpenSubagentPanel = React.useCallback(async (parentConversationId: string, taskId: string): Promise<boolean> => {
  // routeKey is the stable concurrency boundary for one parent/task transcript.
  // routeKey 是单个父级/任务转录的稳定并发边界。
  const routeKey = `${parentConversationId}:${taskId}`;
  // inflightRequests stores all independent route loads instead of one global last-click request.
  // inflightRequests 保存全部独立路由加载，而不是只保存全局最后一次点击请求。
  const inflightRequests = inflightSubagentOpenRequestsRef.current;
  // existingRequest is shared only when another caller is already loading this exact tab route.
  // existingRequest 仅在另一调用方已加载完全相同的页签路由时共享。
  const existingRequest = inflightRequests.get(routeKey);
  // coalesced records whether this caller owns the backend command or waits on an existing one.
  // coalesced 记录当前调用方是后端命令拥有者还是复用既有请求。
  const coalesced = existingRequest !== undefined;
  // requestPromise remains route-local, so successful sibling tab loads never invalidate each other.
  // requestPromise 保持路由局部性，因此成功的同级页签加载不会相互判定为过期。
  const requestPromise = existingRequest ?? openSubagentPanel(parentConversationId, taskId);
  if (!existingRequest) {
    inflightRequests.set(routeKey, requestPromise);
  }
  webuiClickTrace(coalesced ? "subagent.open.coalesced" : "subagent.open.invoke", {
    parentConversationId,
    taskId,
    routeKey,
  });
  try {
    // Every route consumer waits for the shared command and applies navigation independently.
    // 每个路由调用方都等待共享命令，并独立应用导航。
    const response = await requestPromise;
    if (response) {
      setWorkbenchState((current) => applySubagentPanelRouteResponse(
        current,
        response,
      ));
    }
    webuiClickTrace(coalesced ? "subagent.open.coalesced_result" : "subagent.open.result", {
      parentConversationId,
      taskId,
      routeKey,
      opened: Boolean(response),
      activeConversationId: response?.activeConversationId ?? null,
      activeTaskId: response?.subagentNavigation?.activeTaskId ?? null,
    });
    return Boolean(response);
  } catch (nextError) {
    // message normalizes backend failures for tracing and the single command owner toast.
    // message 归一化后端失败，供追踪与唯一命令拥有者提示使用。
    const message = nextError instanceof Error ? nextError.message : String(nextError);
    webuiClickTrace(coalesced ? "subagent.open.coalesced_error" : "subagent.open.error", {
      parentConversationId,
      taskId,
      routeKey,
      message,
    });
    if (message.includes("不可恢复") || message.includes("没有找到可打开的智能体任务")) {
      return false;
    }
    if (!coalesced) {
      toast.error(message);
    }
    return false;
  } finally {
    // Delete only the settled promise still registered for this route; a newer retry must remain untouched.
    // 仅删除该路由中仍登记的已完成 Promise；更晚发起的重试必须保持不变。
    if (inflightRequests.get(routeKey) === requestPromise) {
      inflightRequests.delete(routeKey);
    }
  }
}, []);

// handleStopSubagentTask asks the backend to stop one running child task.
// handleStopSubagentTask 请求后端停止单个运行中的子任务。
// Parameters: parentConversationId owns the exact taskId. Returns after authoritative state application; rejects after failure feedback.
// 参数：parentConversationId 持有精确 taskId。应用权威状态后返回；失败反馈后继续拒绝。
const handleStopSubagentTask = React.useCallback(async (parentConversationId: string, taskId: string): Promise<void> => {
  webuiClickTrace("subagent.stop.invoke", { parentConversationId, taskId });
  try {
    const response = await stopSubagentTask(parentConversationId, taskId);
    if (response) {
      setWorkbenchState((current) => applySubagentPanelRouteResponse(
        current,
        response,
      ));
    }
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    // Preserve rejection so the confirmation remains available for retry.
    // 保留拒绝状态，让确认界面继续可供重试。
    throw nextError;
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
  // conversationId binds both the attempted switch and its recovery action to the original session.
  // conversationId 将本次切换及恢复操作绑定到原会话。
  const conversationId = selectedDraftConversation ? null : selectedConversationId;
  try {
    const selected = await selectWorkbenchModel(choice, conversationId);
    if (selected) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, selected.workbench));
      toast.success(formatConversationActionTranslation(translate, "conversationActions.modelSwitched", {
        model: choice.modelName,
        provider: choice.providerName,
      }));
      // A session switching to a model with its own window default is asked whether to adopt it; drafts already
      // receive the model default when the session is created.
      // 会话切换到带有窗口默认值的模型时询问是否采用；草稿在创建会话时已自动获得模型默认值。
      const offer = conversationId
        ? modelContextWindowOffer(selected.settings, selected.workbench, choice.providerId, choice.modelId)
        : null;
      if (conversationId && offer) {
        showModelContextWindowOffer({
          offer,
          modelName: choice.modelName,
          nodes: selected.settings.contextCompactionWindowNodes.nodes,
          translate,
          onSwitch: (selection) => {
            void setSessionContextCompactionWindow(conversationId, selection).then((response) => {
              if (response) setWorkbenchState((current) => applySessionAddonsChangedResponse(current, response));
            }).catch((reason: unknown) => {
              toast.error(reason instanceof Error ? reason.message : String(reason));
            });
          },
        });
      }
    }
    return true;
  } catch (nextError) {
    if (conversationId && nextError instanceof BrowserServiceRpcError && nextError.code === "unreplayable_reasoning_history") {
      // recoveryKey scopes retries to the exact source session and target route.
      // recoveryKey 将重试限定到精确的来源会话与目标路由。
      const recoveryKey = JSON.stringify([conversationId, choice.providerId, choice.connectionId, choice.modelId]);
      // toastId scopes both the visible offer and the retained action to reasoning recovery.
      // toastId 将可见提示和保留操作限定在推理恢复范围内。
      const toastId = `reasoning-recovery:${recoveryKey}`;
      toast(translate("conversationActions.reasoningReplayBlocked"), {
        id: toastId,
        description: translate("conversationActions.reasoningRecoveryDescription"),
        duration: Infinity,
        action: {
          label: translate("conversationActions.reasoningRecoveryCreate"),
          onClick: (event) => {
            event.preventDefault();
            // Resolve the ID only when the user starts recovery, before the first RPC.
            // 仅当用户开始恢复时解析 ID，并在第一次 RPC 前完成。
            const recovery = recoveryActionsRef.current.get(recoveryKey)
              ?? { actionId: retainClientOwnedRetryActionId(toastId), running: false };
            if (recovery.running) return;
            recoveryActionsRef.current.set(recoveryKey, recovery);
            recovery.running = true;
            void createReasoningRecoverySession({
              actionId: recovery.actionId,
              conversationId,
              providerId: choice.providerId,
              modelId: choice.modelId,
              connectionId: choice.connectionId,
            }).then(async (sessionId) => {
              // Refresh the indexed conversation before selecting it so the composer loads its saved draft.
              // 先刷新索引会话再选中它，以便输入区加载已保存的草稿。
              const snapshot = await loadWorkbenchSnapshot();
              setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
              if (selectedConversationIdRef.current === conversationId) {
                setDraftConversation(null);
                setSelectedConversationId(sessionId);
              }
              completeClientOwnedRetryAction(toastId, recovery.actionId);
              recoveryActionsRef.current.delete(recoveryKey);
              toast.dismiss(toastId);
            }).catch((error: unknown) => {
              toast.error(error instanceof Error ? error.message : String(error));
            }).finally(() => {
              recovery.running = false;
            });
          },
        },
      });
      return false;
    }
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [
  mergeWorkbenchWithLocalCaches,
  selectedConversationId,
  selectedConversationIdRef,
  selectedDraftConversation,
  setDraftConversation,
  setSelectedConversationId,
  setWorkbenchState,
  translate,
]);

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
  forceRefresh: boolean,
): Promise<ProviderModelSecondaryRouteSummary[]> => {
  return listWorkbenchModelSecondaryRoutes(providerId, modelId, forceRefresh);
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

// handleSelectWorkbenchReasoningEffort persists effort for the exact choice and returns whether it was saved.
// handleSelectWorkbenchReasoningEffort 为精确 choice 持久化 effort，返回是否保存成功。
const handleSelectWorkbenchReasoningEffort = React.useCallback(async (effort: string, choice: ModelPickerChoice): Promise<boolean> => {
  // conversationId freezes the exact owner in this callback instance. A queued write keeps this
  // closure even if the user navigates before the former write settles.
  // conversationId 在本回调实例中冻结精确归属。即使用户在前一写入结束前切换会话，排队写入
  // 仍会保留该闭包。
  const conversationId = selectedDraftConversation ? null : selectedConversationId;
  try {
    const snapshot = await selectWorkbenchReasoningEffort(effort, conversationId, {
      providerId: choice.providerId,
      modelId: choice.modelId,
      connectionId: choice.connectionId,
    });
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [
  mergeWorkbenchWithLocalCaches,
  selectedConversationId,
  selectedDraftConversation,
  setWorkbenchState,
]);

// handleSelectWorkbenchThinkingDisplayMode persists one thinking display preference through the shared service router.
// handleSelectWorkbenchThinkingDisplayMode 通过共享服务路由持久化单个思考展示偏好。
const handleSelectWorkbenchThinkingDisplayMode = React.useCallback(async (mode: ThinkingDisplayMode): Promise<boolean> => {
  try {
    const snapshot = await selectWorkbenchThinkingDisplayMode(mode);
    if (!snapshot) return false;
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    return true;
  } catch (nextError) {
    toast.error(nextError instanceof Error ? nextError.message : String(nextError));
    return false;
  }
}, [mergeWorkbenchWithLocalCaches, setWorkbenchState]);


  // Saves the system default and merges the authoritative snapshot; failures remain visible to the settings form.
  // 保存系统默认值并合并权威快照；失败交由设置表单明确展示。
  // Parameter enabled selects the default; resolves only after persistence succeeds.
  // 参数 enabled 选择默认值；仅在持久化成功后完成。
  const handleSaveToolCollapseDefault = React.useCallback(async (enabled: boolean): Promise<void> => {
    const snapshot = await saveToolCollapseDefault(enabled);
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
  }, [mergeWorkbenchWithLocalCaches, setWorkbenchState]);

  return {
    handleSaveToolCollapseDefault,
    handleSubmitConversationPrompt,
    handleInsertPendingPrompt,
    handleStopConversationRun,
    handleContinueConversation,
    handleEditConversationGeneration,
    handleForkConversationGeneration,
    handleCancelConversationSubmission,
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
