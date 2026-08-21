import React from "react";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type { ComposerSubmission } from "@/components/composer/composer-model";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import { materializeAcknowledgedDraftSubmission } from "@/components/workspace/draft-submission-state";
import {
  cancelConversationSubmission,
  compactSelectedConversationContext,
  closeSubagentTask,
  forkConversation,
  getUserPromptEditDraft,
  insertPendingPrompt,
  openSubagentPanel,
  removePendingPrompt,
  reorderPendingPrompts,
  continueConversation,
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
  mergeConversationMessagesWithSubagentSurfaceMessageCache,
  reorderPendingPromptSummaries,
  type SubagentSurfaceMessageCache,
  type WorkbenchMergeOptions,
} from "@/state/workbench-state";
import type {
  ComposerAttachmentSummary,
  ConversationSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  SubagentPanelRouteResponse,
  UserPromptEditDraftSummary,
  WorkbenchSnapshot,
} from "@/types";
import type { TranslationKey } from "@/i18n-messages";
import {
  conversationContinuationBlockedReasonText,
  conversationContinuationBlockedReasonTextFromError,
} from "@/components/conversation/continuation-blocked-reason";
import {
  continuationClaimSnapshotHasConverged,
  reconcileContinuationClaimSnapshot,
  type ContinuationClaimExpectation,
} from "@/components/conversation/continuation-claim-reconciliation";
import { toast } from "sonner";
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
  // Conversation ids created in this frontend lifetime whose transcript is already live and needs no history hydration.
  // 当前前端生命周期内创建且转录已实时可用、无需历史水合的会话标识。
  locallyCreatedConversationIdsRef: React.MutableRefObject<Set<string>>;
  // Latest workbench snapshot for callbacks that should not close over stale state.
  // 供回调读取最新工作台快照，避免闭包过期。
  workbenchStateRef: React.MutableRefObject<WorkbenchSnapshot>;
  // Asks the user whether running child agents should be stopped together.
  // 询问用户是否要同时停止运行中的智能体。
  requestStopConversationRunChoice: RequestStopConversationRunChoice;
  // Cached subagent transcript surfaces for instant route switching.
  // 用于即时路由切换的子代理转录界面缓存。
  subagentSurfaceMessageCacheRef: React.MutableRefObject<SubagentSurfaceMessageCache>;
  // Invalidates hydration responses that began before a destructive prompt edit committed.
  // 作废在破坏性提示词编辑提交前启动的水合响应。
  invalidateConversationHydration: (conversationId: string) => void;
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
    locallyCreatedConversationIdsRef,
    workbenchStateRef,
    requestStopConversationRunChoice,
    subagentSurfaceMessageCacheRef,
    invalidateConversationHydration,
  } = options;
  // inflightSubagentOpenRequestsRef coalesces duplicate opens without serializing independent side-panel tabs.
  // inflightSubagentOpenRequestsRef 合并重复打开请求，但不串行化彼此独立的侧栏页签。
  const inflightSubagentOpenRequestsRef = React.useRef<SubagentOpenRequestMap>(new Map());
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
  // clientSubmissionId correlates the optimistic first message with the authoritative service acknowledgement.
  // clientSubmissionId 将乐观首条消息与权威服务确认关联起来。
  const clientSubmissionId = crypto.randomUUID();
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
      submission.prompt,
      submission.hiddenModelGuidance,
      submission.attachments,
      submission.executionMode,
      submission.reasoningEffort,
      submission.modelSelection,
      submission.sessionSettings,
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
    const nextConversationId = ack.conversationId || activeConversationId;
    if (nextConversationId) {
      // A successful draft submit creates the persisted id before its ordered sidebar delta is rendered.
      // 草稿提交成功后，持久化标识会先于有序侧栏增量完成渲染。
      if (selectedDraftConversation?.id === submittedSelectionId) {
        locallyCreatedConversationIdsRef.current.add(nextConversationId);
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
          materializedConversationAlreadyVisible,
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
      toast.error(errorMessage);
    }
    return false;
  } finally {
    if (pendingStageTimer !== null) {
      window.clearTimeout(pendingStageTimer);
    }
  }
}, [
  pendingMaterializedConversationIdRef,
  locallyCreatedConversationIdsRef,
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

// continuationRequestTimeoutMs bounds one Continue claim so the UI never stays silently locked.
// continuationRequestTimeoutMs 限制单次 Continue 领取请求，避免界面无提示地长期锁定。
const continuationRequestTimeoutMs = 15_000;

// continuationReconciliationTimeoutMs bounds one authoritative state read after a Continue claim.
// continuationReconciliationTimeoutMs 限制 Continue 领取后的单次权威状态回读。
const continuationReconciliationTimeoutMs = 5_000;

// loadContinuationReconciliationSnapshot reads one bounded authoritative snapshot for claim convergence.
// loadContinuationReconciliationSnapshot 为领取收敛读取一次有界权威快照。
//
// Returns: the authoritative snapshot, or null when the read fails or reaches its timeout.
// 返回值：权威快照；读取失败或达到超时上限时返回 null。
async function loadContinuationReconciliationSnapshot(): Promise<WorkbenchSnapshot | null> {
  // controller owns only this reconciliation request and never aborts the accepted claim.
  // controller 只控制本次对账请求，绝不会中止已经接受的领取。
  const controller = new AbortController();
  // timeoutId releases a reconciliation request that cannot produce an authoritative state.
  // timeoutId 解除无法产生权威状态的对账请求。
  const timeoutId = setTimeout(() => controller.abort(), continuationReconciliationTimeoutMs);
  try {
    return await loadWorkbenchSnapshot(controller.signal);
  } catch {
    return null;
  } finally {
    clearTimeout(timeoutId);
  }
}

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
  if (message.includes("ERR_CONVERSATION_CONTINUE_STATE_RECONCILIATION_FAILED")) {
    return translate("conversation.continuation.stateSyncFailed");
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

// handleContinueConversation continues a terminated conversation through the revision-bound retained state.
// handleContinueConversation 通过绑定修订号的保留状态继续已终止会话。
const handleContinueConversation = React.useCallback(async (
  conversationId: string,
  expectedGeneration: number,
  expectedLedgerRevision: number,
  clientContinuationId: string,
): Promise<boolean> => {
  // controller owns the claim request timeout before any post-ACK state reconciliation begins.
  // controller 在 ACK 后状态对账开始前控制领取请求超时。
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
      expectedGeneration,
      expectedLedgerRevision,
      clientContinuationId,
      controller.signal,
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
  } catch (nextError) {
    // normalizedError gives a timed-out transport one stable user-facing classification.
    // normalizedError 为超时传输提供一个稳定的用户可见分类。
    const normalizedError = timedOut
      ? new Error("ERR_CONVERSATION_CONTINUE_TIMEOUT")
      : nextError;
    // Reconcile every failed claim so stale eligibility cannot leave the same dead button visible.
    // 每次领取失败都回读权威状态，避免过期资格让同一个无效按钮继续显示。
    // snapshot is optional because the persistent failure status must remain visible during outages.
    // snapshot 是可选值，因为服务中断期间仍必须保留持久失败状态。
    const snapshot = await loadContinuationReconciliationSnapshot();
    if (snapshot) {
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
    }
    toast.error(continuationErrorMessage(normalizedError, translate));
    return false;
  } finally {
    clearTimeout(timeoutId);
  }
  if (!response) {
    return false;
  }
  // expectation binds the post-ACK snapshot to the exact token and durable revision just accepted.
  // expectation 将 ACK 后快照绑定到刚刚接受的精确令牌与持久修订。
  const expectation: ContinuationClaimExpectation = {
    conversationId,
    expectedGeneration,
    expectedLedgerRevision,
    acceptedRevision: response.revision,
  };
  // reconciliationSnapshot closes the success path even when the preceding broadcast delta was lost.
  // reconciliationSnapshot 即使前置广播增量丢失，也会闭合成功路径。
  const reconciliationSnapshot = await loadContinuationReconciliationSnapshot();
  if (
    !reconciliationSnapshot
    || !continuationClaimSnapshotHasConverged(reconciliationSnapshot, expectation)
  ) {
    toast.error(translate("conversation.continuation.stateSyncFailed"));
    return false;
  }
  setWorkbenchState((current) => {
    // merged applies the normal full-snapshot ordering rule before the target-only loss repair.
    // merged 在目标级丢失修复前先应用常规全量快照排序规则。
    const merged = mergeWorkbenchWithLocalCaches(current, reconciliationSnapshot);
    return reconcileContinuationClaimSnapshot(merged, reconciliationSnapshot, expectation);
  });
  return true;
}, [mergeWorkbenchWithLocalCaches, setWorkbenchState, translate]);

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
  setWorkbenchState((current) => injectCachedSubagentSurfaceMessages(current, subagentSurfaceMessageCacheRef.current));
  try {
    // Every route consumer waits for the shared command and applies its successful transcript independently.
    // 每个路由调用方都等待共享命令，并独立应用其成功返回的转录。
    const response = await requestPromise;
    if (response) {
      setWorkbenchState((current) => applySubagentPanelRouteResponse(
        current,
        response,
        subagentSurfaceMessageCacheRef.current,
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
    // conversationId scopes model persistence to an existing session; drafts update the global new-session default.
    // conversationId 将模型持久化限定到既有会话；草稿则更新新会话使用的全局默认值。
    const conversationId = selectedDraftConversation ? null : selectedConversationId;
    const snapshot = await selectWorkbenchModel(choice, conversationId);
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
}, [
  mergeWorkbenchWithLocalCaches,
  selectedConversationId,
  selectedDraftConversation,
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

// handleSubmitUserPromptEdit submits one edit and closes the destructive transaction with an authoritative snapshot.
// handleSubmitUserPromptEdit 提交单次编辑，并用权威快照闭合破坏性事务。
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
    if (ack) {
      // The first-open hydration request may still carry the pre-edit transcript and must never cross this commit boundary.
      // 首次打开时的水合请求可能仍携带编辑前转录，绝不能跨越本次提交边界。
      invalidateConversationHydration(request.conversationId);
      try {
        // authoritativeSnapshot closes the destructive-edit transaction even if an intermediate
        // rollback patch was delayed behind the replacement prompt's live stream.
        // authoritativeSnapshot 在中间回滚补丁晚于替代提示词实时流时，仍会闭合破坏性编辑事务。
        const authoritativeSnapshot = await loadWorkbenchSnapshot();
        // authoritativeConversationMessages is the exact post-edit window and must not inherit the removed streaming tail.
        // authoritativeConversationMessages 是编辑后的精确窗口，不得继承已删除的流式尾部。
        const authoritativeConversationMessages = authoritativeSnapshot.messages.filter(
          (message) => message.conversationId === request.conversationId,
        );
        setWorkbenchState((current) => {
          const mergedWorkbench = mergeWorkbenchWithLocalCaches(current, authoritativeSnapshot);
          return mergeConversationMessagesWithSubagentSurfaceMessageCache(
            mergedWorkbench,
            request.conversationId,
            authoritativeConversationMessages,
            subagentSurfaceMessageCacheRef.current,
          );
        });
      } catch (refreshError) {
        // The edit already committed; live deltas and gap recovery remain authoritative fallbacks.
        // 编辑已经提交；实时增量与缺口恢复仍是权威回退路径。
        console.warn("Failed to refresh the workbench after a committed prompt edit.", refreshError);
      }
    }
    return true;
  } catch (nextError) {
    toast.error(translate("conversation.action.editPromptFailed"));
    return false;
  }
}, [
  mergeWorkbenchWithLocalCaches,
  invalidateConversationHydration,
  selectedConversationIdRef,
  setSelectedConversationId,
  setWorkbenchState,
  subagentSurfaceMessageCacheRef,
  translate,
]);

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
        ? `${translate("conversation.action.forkFailed")}${translate("global.labelSeparator")}${errorMessage}`
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
  // conversationId freezes the exact owner in this callback instance. A queued write keeps this
  // closure even if the user navigates before the former write settles.
  // conversationId 在本回调实例中冻结精确归属。即使用户在前一写入结束前切换会话，排队写入
  // 仍会保留该闭包。
  const conversationId = selectedDraftConversation ? null : selectedConversationId;
  if (!conversationId) {
    toast.error("Reasoning effort can only be persisted for a materialized conversation.");
    return false;
  }
  try {
    const snapshot = await selectWorkbenchReasoningEffort(effort, conversationId);
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
    handleContinueConversation,
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
