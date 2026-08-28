import type {
  CommandSessionSummary,
  ComposerAttachmentPreparationSummary,
  ComposerAttachmentSummary,
  PendingPromptSummary,
  ContextCompactionWindowNodeId,
  SessionAddonsChangedResponse,
  SessionGoalUserStatusUpdate,
  SubagentPanelRouteResponse,
  SubmitConversationPromptAck,
  WorkbenchSnapshot,
} from "@/types";
import type {
  ComposerModelSelectionSnapshot,
  ComposerSubmission,
  ComposerSessionSettingsSnapshot,
} from "@/components/composer/composer-model";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";
import { updateBrowserFallbackWorkbench } from "@/service/browser-fallback-workbench";

// submitConversationPrompt sends one ordinary new user turn to the desktop conversation runtime.
// submitConversationPrompt 将一个普通的新用户回合发送到桌面会话运行时。
export async function submitConversationPrompt(
  clientSubmissionId: string,
  projectId: string | null,
  conversationId: string | null,
  draftSlotKey: string,
  prompt: string,
  hiddenModelGuidance: string | undefined,
  attachments: ComposerAttachmentSummary[],
  executionMode: string,
  reasoningEffort: string | null,
  modelSelection: ComposerModelSelectionSnapshot | null,
  sessionSettings: ComposerSessionSettingsSnapshot | null,
): Promise<SubmitConversationPromptAck | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SubmitConversationPromptAck>("service_submit_conversation_prompt", {
    clientSubmissionId,
    projectId,
    conversationId,
    draftSlotKey,
    prompt,
    hiddenModelGuidance,
    attachments,
    executionMode,
    reasoningEffort,
    modelSelection,
    sessionSettings,
  });
}

// continueConversation resumes one terminated conversation without appending a user prompt.
// continueConversation 在不追加用户提示词的情况下继续单个已终止会话。
export async function continueConversation(
  conversationId: string,
  clientContinuationId: string,
  supplement: ComposerSubmission | null,
  signal?: AbortSignal,
): Promise<SubmitConversationPromptAck | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SubmitConversationPromptAck>(
    "service_continue_conversation",
    {
      conversationId,
      clientContinuationId,
      prompt: supplement?.prompt,
      hiddenModelGuidance: supplement?.hiddenModelGuidance,
      executionMode: supplement?.executionMode,
      reasoningEffort: supplement?.reasoningEffort,
      modelSelection: supplement?.modelSelection,
      attachments: supplement?.attachments,
    },
    { signal },
  );
}

/** Request for replacing one persisted root input through the V1 generation boundary.
 * 通过 V1 generation 边界替换一条已持久化根输入的请求。
 */
export interface ConversationEditGenerationV1Request extends Record<string, unknown> {
  /** Exact V1 session identity.
   * 精确的 V1 会话身份。
   */
  sessionId: string;
  /** Event-window generation observed before the edit.
   * 编辑前观察到的事件窗口代次。
   */
  expectedWindowGeneration: number;
  /** Persisted root-input seq being replaced.
   * 被替换的持久化根输入 seq。
   */
  targetSeq: number;
  /** Exact replacement text.
   * 精确替换正文。
   */
  replacementText: string;
}

/** Response for one fully published V1 edit generation.
 * 一次完整发布的 V1 编辑 generation 响应。
 */
export interface ConversationEditGenerationV1Response {
  /** Edited V1 session identity.
   * 已编辑的 V1 会话身份。
   */
  sessionId: string;
  /** Rewritten root-input seq.
   * 已重写的根输入 seq。
   */
  targetSeq: number;
  /** Newly published event-window generation.
   * 新发布的事件窗口代次。
   */
  windowGeneration: number;
  /** Complete document revision after publication.
   * 发布后的完整文档修订号。
   */
  documentRevision: number;
  /** Whether the exact rewritten Turn entered physical dispatch.
   * 精确重写回合是否已进入物理分发。
   */
  dispatchStarted: boolean;
}

/** Replaces one V1 root input and waits for durable physical admission.
 * 替换一条 V1 根输入并等待可靠物理准入。
 *
 * @param request exact generation compare-and-swap request.
 *                精确的 generation 比较交换请求。
 * @returns the published generation response, or null without the desktop runtime.
 *          已发布 generation 响应；桌面运行时不可用时返回 null。
 */
export async function editConversationGenerationV1(
  request: ConversationEditGenerationV1Request,
): Promise<ConversationEditGenerationV1Response | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<ConversationEditGenerationV1Response>(
    "service_edit_conversation_generation_v1",
    request,
  );
}

/** Request for creating or exactly retrying one immutable V1 child generation.
 * 创建或精确重试一个不可变 V1 子 generation 的请求。
 */
export interface ConversationForkGenerationV1Request extends Record<string, unknown> {
  /** Exact parent V1 session identity.
   * 精确的父 V1 会话身份。
   */
  sourceSessionId: string;
  /** Parent event-window generation observed before the fork.
   * 分叉前观察到的父事件窗口代次。
   */
  expectedWindowGeneration: number;
  /** Parent root-input seq retained by the child.
   * 子会话保留的父根输入 seq。
   */
  targetSeq: number;
  /** Stable child identity reused after response loss.
   * 响应丢失后复用的稳定子会话身份。
   */
  childSessionId: string;
  /** Exact child title.
   * 精确的子会话标题。
   */
  title: string;
}

/** Response for one created or exactly replayed V1 child generation.
 * 一个已创建或精确重放的 V1 子 generation 响应。
 */
export interface ConversationForkGenerationV1Response {
  /** Exact parent V1 session identity.
   * 精确的父 V1 会话身份。
   */
  sourceSessionId: string;
  /** Stable child V1 session identity.
   * 稳定的子 V1 会话身份。
   */
  childSessionId: string;
  /** Whether this request physically created the child.
   * 当前请求是否物理创建了子会话。
   */
  created: boolean;
  /** Parent root-input seq retained by the child.
   * 子会话保留的父根输入 seq。
   */
  sourceTargetSeq: number;
  /** Child event-window generation.
   * 子事件窗口代次。
   */
  windowGeneration: number;
  /** Child complete document revision.
   * 子会话完整文档修订号。
   */
  documentRevision: number;
  /** Whether the exact child Turn entered physical dispatch.
   * 精确子回合是否已进入物理分发。
   */
  dispatchStarted: boolean;
}

/** Creates or exactly retries one V1 conversation fork.
 * 创建或精确重试一个 V1 会话分叉。
 *
 * @param request immutable parent boundary and stable child identity.
 *                不可变父边界与稳定子会话身份。
 * @returns the child publication response, or null without the desktop runtime.
 *          子会话发布响应；桌面运行时不可用时返回 null。
 */
export async function forkConversationGenerationV1(
  request: ConversationForkGenerationV1Request,
): Promise<ConversationForkGenerationV1Response | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<ConversationForkGenerationV1Response>(
    "service_fork_conversation_generation_v1",
    request,
  );
}

// cancelConversationSubmission cancels pre-dispatch media preparation by client submission id.
// cancelConversationSubmission 按客户端提交标识取消分派前媒体准备。
export async function cancelConversationSubmission(clientSubmissionId: string): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_cancel_conversation_submission", {
    clientSubmissionId,
  });
}

// StopConversationRunMode selects how the backend handles live child agents.
// StopConversationRunMode 选择后端如何处理仍在运行的智能体。
export type StopConversationRunMode = "ask_if_running_subagents" | "main_only" | "main_and_subagents";

// StopConversationRunAgentSummary describes one running child agent reported by the backend.
// StopConversationRunAgentSummary 描述后端报告的一条运行中智能体。
export interface StopConversationRunAgentSummary {
  // Direct parent conversation id for this child agent.
  // 该智能体的直接父会话标识。
  parent_conversation_id: string;
  // Stable child task session id.
  // 稳定的子任务会话标识。
  task_id: string;
  // Human-facing child agent label.
  // 面向用户的智能体标签。
  label: string;
}

// StopConversationRunResponse is the tagged response for a stop request.
// StopConversationRunResponse 是停止请求的带标记响应。
export type StopConversationRunResponse =
  | {
    // The conversation stop completed.
    // 会话停止已完成。
    status: "stopped";
    // Full rebuilt workbench snapshot.
    // 完整重建后的工作台快照。
    workbench: WorkbenchSnapshot;
  }
  | {
    // The backend found running agents and needs user confirmation.
    // 后端发现运行中智能体，需要用户确认。
    status: "needs_confirmation";
    // Running child agents that need a stop decision.
    // 需要停止决策的运行中智能体。
    running_subagents: StopConversationRunAgentSummary[];
  };

// stopConversationRun stops one exact backend conversation run.
// stopConversationRun 停止一个精确后端会话运行。
// Parameters: conversationId is the immutable stop target; mode selects child-agent handling.
// 参数：conversationId 是不可变停止目标；mode 选择子代理处理方式。
// Returns: the tagged stop response, or null when no service runtime is available.
// 返回值：带标记的停止响应；无服务运行时时返回 null。
export async function stopConversationRun(
  conversationId: string,
  mode: StopConversationRunMode,
): Promise<StopConversationRunResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<StopConversationRunResponse>("service_stop_conversation_run", {
    conversationId,
    mode,
  });
}

// stopCommandSession force-stops one live backend command session.
// stopCommandSession 强制停止单条实时后端命令会话。
export async function stopCommandSession(commandSessionId: string, ownerSessionId?: string | null): Promise<CommandSessionSummary[]> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.commandSessions = snapshot.commandSessions.map((session) => session.id === commandSessionId
        ? { ...session, ownerSessionId: ownerSessionId ?? session.ownerSessionId ?? null, status: "stopped", preview: "Browser preview marked this session as stopped." }
        : session);
    }).commandSessions;
  }
  return serviceClient.request<CommandSessionSummary[]>("service_stop_command_session", {
    commandSessionId,
    ownerSessionId: ownerSessionId ?? null,
  });
}

// getCommandSessionSnapshot reads one live command-session snapshot without refreshing the full workbench.
// getCommandSessionSnapshot 读取单条实时命令会话快照，但不刷新完整工作台。
export async function getCommandSessionSnapshot(
  commandSessionId: string,
  ownerSessionId?: string | null,
): Promise<CommandSessionSummary | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<CommandSessionSummary>("service_get_command_session_snapshot", {
    commandSessionId,
    ownerSessionId: ownerSessionId ?? null,
  });
}

// TerminalInputEvent describes one structured terminal input sent through the service runtime.
// TerminalInputEvent 描述一条通过服务运行时发送的结构化终端输入。
export type TerminalInputEvent =
  | { kind: "text"; text: string }
  | { kind: "paste"; text: string }
  | { kind: "key"; key: string }
  | { kind: "chord"; key: string; ctrl?: boolean; alt?: boolean; shift?: boolean; meta?: boolean };

// sendCommandSessionInput sends structured input events to one live terminal session.
// sendCommandSessionInput 向单条实时终端会话发送结构化输入事件。
export async function sendCommandSessionInput(
  commandSessionId: string,
  ownerSessionId: string | null | undefined,
  inputs: TerminalInputEvent[],
): Promise<CommandSessionSummary[]> {
  if (!hasServiceRuntime()) {
    return [];
  }
  return serviceClient.request<CommandSessionSummary[]>("service_send_command_session_input", {
    request: {
      commandSessionId,
      ownerSessionId: ownerSessionId ?? null,
      inputs,
    },
  });
}

// GitCommitSummaryGenerateResponse describes backend-generated commit message text and accounting metadata.
// GitCommitSummaryGenerateResponse 描述后端生成的提交信息文本与统计元数据。
export interface GitCommitSummaryGenerateResponse {
  // Generated commit message text.
  // 生成的提交信息文本。
  message: string;
  // Provider id used for the internal generation request.
  // 内部生成请求使用的 Provider 标识。
  providerId: string;
  // Model id used for the internal generation request.
  // 内部生成请求使用的模型标识。
  modelId: string;
  // Number of real provider requests used for generation.
  // 生成过程使用的真实 Provider 请求次数。
  providerRequestCount: number;
  // Whether the full diff was replaced by compact metadata.
  // 完整 diff 是否被紧凑元数据替换。
  inputWasTruncated: boolean;
  // Estimated input tokens used by the generation prompt.
  // 生成提示词使用的输入 Token 估算值。
  estimatedInputTokens: number;
  // Provider-reported usage from the successful generation request when available.
  // 成功生成请求返回的 Provider 用量；不可用时为空。
  providerUsage?: {
    inputTokens?: number | null;
    outputTokens?: number | null;
    reasoningTokens?: number | null;
    totalTokens?: number | null;
    cachedInputTokens?: number | null;
    cacheCreationInputTokens?: number | null;
  } | null;
}

// GitCommitActionTarget describes the commit target selected by the WebUI.
// GitCommitActionTarget 描述 WebUI 选择的提交目标。
export type GitCommitActionTarget =
  | { mode: "current" }
  | { mode: "new"; branchName: string };

// GitCommitActionResponse describes a completed backend Git commit/push action.
// GitCommitActionResponse 描述已完成的后端 Git 提交/推送操作。
export interface GitCommitActionResponse {
  // Human-readable operation summary.
  // 面向用户的操作摘要。
  summary: string;
  // Whether a commit was created.
  // 是否创建了提交。
  committed: boolean;
  // Whether a push command was executed.
  // 是否执行了推送。
  pushed: boolean;
  // Current branch after the action.
  // 操作后的当前分支。
  branchName?: string | null;
  // Short hash for the commit affected by the action.
  // 本次操作涉及的提交短哈希。
  commitHash?: string | null;
  // Commit message subject for the commit affected by the action.
  // 本次操作涉及的提交信息标题。
  commitMessage?: string | null;
}

// generateGitCommitMessage asks the backend to generate a commit message for the selected session repository.
// generateGitCommitMessage 请求后端为当前会话仓库生成提交信息。
export async function generateGitCommitMessage(request: {
  conversationId: string;
  workspacePath: string;
  repositoryKey?: string | null;
  includeUnstaged: boolean;
}): Promise<GitCommitSummaryGenerateResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<GitCommitSummaryGenerateResponse>("service_generate_git_commit_message", { request });
}

// runGitCommitAction asks the backend to create a commit and optionally push it.
// runGitCommitAction 请求后端创建提交并可选推送。
export async function runGitCommitAction(request: {
  conversationId: string;
  workspacePath: string;
  repositoryKey?: string | null;
  target: GitCommitActionTarget;
  includeUnstaged: boolean;
  message: string;
  push: boolean;
}): Promise<GitCommitActionResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<GitCommitActionResponse>("service_run_git_commit_action", { request });
}

// resizeCommandSession resizes one live terminal session.
// resizeCommandSession 调整单条实时终端会话尺寸。
export async function resizeCommandSession(
  commandSessionId: string,
  ownerSessionId: string | null | undefined,
  rows: number,
  cols: number,
): Promise<CommandSessionSummary[]> {
  if (!hasServiceRuntime()) {
    return [];
  }
  return serviceClient.request<CommandSessionSummary[]>("service_resize_command_session", {
    request: {
      commandSessionId,
      ownerSessionId: ownerSessionId ?? null,
      rows,
      cols,
    },
  });
}

// saveSessionGoal persists a user-authored goal for the current conversation.
// saveSessionGoal 为当前会话持久化用户编写的目标。
export async function saveSessionGoal(
  conversationId: string | null,
  objective: string,
  tokenBudget?: number | null,
): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_save_session_goal", {
    request: {
      conversationId,
      objective,
      tokenBudget: tokenBudget ?? null,
    },
  });
}

// resumeSessionGoal resumes a stopped goal as a fresh budget stage.
// resumeSessionGoal 将已停止目标恢复为新的预算阶段。
export async function resumeSessionGoal(
  conversationId: string | null,
  objective: string,
  tokenBudget?: number | null,
): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_resume_session_goal", {
    request: {
      conversationId,
      objective,
      tokenBudget: tokenBudget ?? null,
    },
  });
}

// clearSessionGoal removes the persisted goal for the current conversation.
// clearSessionGoal 移除当前会话的持久化目标。
export async function clearSessionGoal(conversationId: string | null): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_clear_session_goal", {
    request: {
      conversationId,
    },
  });
}

// updateSessionGoalStatus updates the persisted lifecycle status for the current conversation goal.
// updateSessionGoalStatus 更新当前会话目标的持久化生命周期状态。
export async function updateSessionGoalStatus(
  conversationId: string | null,
  status: SessionGoalUserStatusUpdate,
): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_update_session_goal_status", {
    request: {
      conversationId,
      status,
    },
  });
}

// setSessionPlanMode persists the selected conversation Plan mode state.
// setSessionPlanMode 持久化当前选中会话的计划模式状态。
export async function setSessionPlanMode(
  conversationId: string | null,
  enabled: boolean,
): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_set_session_plan_mode", {
    request: {
      conversationId,
      enabled,
    },
  });
}

// setSessionVmmPolicy persists one selected conversation VMM memory policy mode.
// setSessionVmmPolicy 持久化当前选中会话的单个 VMM 记忆策略模式。
export async function setSessionVmmPolicy(
  conversationId: string | null,
  policy: string,
  mode: string,
): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_set_session_vmm_policy", {
    request: {
      conversationId,
      policy,
      mode,
    },
  });
}

// setSessionOutputStyle persists a per-session style override for the next safe prompt boundary.
// setSessionOutputStyle 持久化会话级风格覆盖，并仅在下一个安全提示词边界生效。
export async function setSessionOutputStyle(
  conversationId: string | null,
  styleId: string | null,
): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime() || !conversationId) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_set_session_output_style", {
    request: {
      conversationId,
      styleId,
    },
  });
}

// planComposerAttachments resolves immutable draft-delivery decisions for the current route.
// planComposerAttachments 为当前路线解析不可变草稿传输决策。
// Parameters: generation rejects late plans, modelSelection freezes the route, and attachments are canonical staged assets.
// 参数：generation 用于拒绝迟到规划，modelSelection 冻结路线，attachments 是规范已暂存资源。
// Returns: one preparation state per attachment in input order.
// 返回值：按输入顺序返回每个附件的一条准备状态。
export async function planComposerAttachments(
  projectId: string | null,
  conversationId: string | null,
  generation: number,
  modelSelection: ComposerModelSelectionSnapshot,
  attachments: ComposerAttachmentSummary[],
): Promise<ComposerAttachmentPreparationSummary[]> {
  if (!hasServiceRuntime()) {
    return attachments.map((attachment) => ({
      attachmentId: attachment.assetId ?? attachment.resourcePath ?? attachment.localPath,
      generation,
      transferId: `browser-draft-media:${generation}:${attachment.assetId ?? attachment.fileName}`,
      deliveryPlan: "metadata_only",
      phase: "metadata_only",
      blocksSubmission: false,
      routeRevision: null,
      reason: "desktop_service_runtime_unavailable",
      error: null,
    }));
  }
  return serviceClient.request<ComposerAttachmentPreparationSummary[]>(
    "service_plan_composer_attachments",
    { projectId, conversationId, generation, modelSelection, attachments },
  );
}

// prepareComposerAttachment executes one planned provider upload without submitting the prompt.
// prepareComposerAttachment 在不提交提示词的情况下执行一条已规划供应商上传。
// Parameters: conversationId scopes reusable references and preparation freezes the transfer identity.
// 参数：conversationId 限定可复用引用作用域，preparation 冻结传输身份。
// Returns: a terminal draft state that is either ready, degraded, failed, or cancelled.
// 返回值：返回就绪、降级、失败或取消之一的终态草稿状态。
export async function prepareComposerAttachment(
  projectId: string | null,
  conversationId: string | null,
  modelSelection: ComposerModelSelectionSnapshot,
  attachment: ComposerAttachmentSummary,
  preparation: ComposerAttachmentPreparationSummary,
): Promise<ComposerAttachmentPreparationSummary> {
  if (!hasServiceRuntime()) {
    return {
      ...preparation,
      phase: "metadata_only",
      blocksSubmission: false,
      reason: "desktop_service_runtime_unavailable",
    };
  }
  return serviceClient.request<ComposerAttachmentPreparationSummary>(
    "service_prepare_composer_attachment",
    { projectId, conversationId, modelSelection, attachment, preparation },
  );
}

// reconcileComposerAttachmentDraftLease replaces one persistent draft owner with its complete canonical attachment set.
// reconcileComposerAttachmentDraftLease 使用完整规范附件集合替换一条持久草稿所有者。
// Parameters: draftSlotKey is the stable composer slot and attachments is its complete service-issued set.
// 参数：draftSlotKey 是稳定输入区草稿槽，attachments 是其完整服务签发集合。
// Returns: whether the backend attachment index changed.
// 返回值：后端附件索引是否发生变化。
export async function reconcileComposerAttachmentDraftLease(
  projectId: string | null,
  conversationId: string | null,
  draftSlotKey: string,
  attachments: ComposerAttachmentSummary[],
): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_reconcile_composer_attachment_draft", {
    projectId,
    conversationId,
    draftSlotKey,
    attachments,
  });
}

// setSessionContextCompactionWindow persists one stable node id or clears the session lock.
// setSessionContextCompactionWindow 持久化一个稳定节点标识，或清除会话锁定。
export async function setSessionContextCompactionWindow(
  conversationId: string | null,
  nodeId: ContextCompactionWindowNodeId | null,
): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime() || !conversationId) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_set_session_context_compaction_window", {
    request: {
      conversationId,
      nodeId,
    },
  });
}

// compactSelectedConversationContext starts backend context compaction for the selected conversation.
// compactSelectedConversationContext 为当前选中会话启动后端上下文压缩。
export async function compactSelectedConversationContext(): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_compact_selected_conversation_context");
}

// replyPendingQuestion submits ask-tool answers to the blocked backend runtime.
// replyPendingQuestion 将 ask 工具答案提交给正在阻塞的后端运行时。
export async function replyPendingQuestion(requestId: string, answers: string[][]): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_reply_pending_question", { requestId, answers });
}

// rejectPendingQuestion cancels one pending ask-tool request after explicit confirmation.
// rejectPendingQuestion 在明确确认后取消一个待处理 ask 工具请求。
export async function rejectPendingQuestion(requestId: string): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_reject_pending_question", { requestId });
}

// openSubagentPanel asks the backend for one child-task transcript route response.
// openSubagentPanel 请求后端返回单个子任务转录路由响应。
export async function openSubagentPanel(parentConversationId: string, taskId: string): Promise<SubagentPanelRouteResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SubagentPanelRouteResponse>("service_open_subagent_panel", { parentConversationId, taskId });
}

// stopSubagentTask asks the backend to stop one child task through the direct service runtime path.
// stopSubagentTask 请求后端通过独立服务运行时路径停止单个子任务。
export async function stopSubagentTask(parentConversationId: string, taskId: string): Promise<SubagentPanelRouteResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SubagentPanelRouteResponse>("service_stop_subagent_task", { parentConversationId, taskId });
}

// closeSubagentTask asks the backend to close one child agent lifecycle through the direct service runtime path.
// closeSubagentTask 请求后端通过独立服务运行时路径关闭单个子智能体生命周期。
export async function closeSubagentTask(parentConversationId: string, taskId: string): Promise<SubagentPanelRouteResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SubagentPanelRouteResponse>("service_close_subagent_task", { parentConversationId, taskId });
}

// editPendingPrompt restores one queued prompt from the selected conversation queue.
// editPendingPrompt 从当前选中会话队列恢复单条排队提示词。
export async function editPendingPrompt(entryId: string): Promise<PendingPromptSummary | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<PendingPromptSummary>("service_edit_pending_prompt", { entryId });
}

// insertPendingPrompt routes one visible queue item through the shared pending-prompt router.
// insertPendingPrompt 通过共享待发送提示词路由器插入单条可见队列项。
export async function insertPendingPrompt(entryId: string): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_insert_pending_prompt", { entryId });
}

// removePendingPrompt removes one queued prompt from the selected conversation queue.
// removePendingPrompt 从当前选中会话队列移除单条排队提示词。
export async function removePendingPrompt(entryId: string): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_remove_pending_prompt", { entryId });
}

// reorderPendingPrompts persists one service-compatible pending prompt drop target.
// reorderPendingPrompts 持久化一次与服务兼容的待发送提示词拖放落点。
export async function reorderPendingPrompts(
  conversationId: string | null,
  draggedEntryId: string,
  targetEntryId: string | null,
): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_reorder_pending_prompts", { conversationId, draggedEntryId, targetEntryId });
}

// stageComposerAttachmentBytes stages one browser/drop/paste provided file through the service.
// stageComposerAttachmentBytes 通过服务暂存一个由浏览器、拖拽或粘贴提供的文件。
export async function stageComposerAttachmentBytes(
  projectId: string | null,
  conversationId: string | null,
  file: File,
): Promise<ComposerAttachmentSummary | null> {
  if (!hasServiceRuntime()) {
    throw new Error("Browser service is unavailable, so the attachment cannot be staged.");
  }
  const dataBase64 = await fileToBase64(file);
  return serviceClient.request<ComposerAttachmentSummary>("service_stage_composer_attachment_bytes", {
    projectId,
    conversationId,
    fileName: file.name || "clipboard-attachment",
    mimeType: file.type || "",
    dataBase64,
  });
}

// stageComposerAttachmentPreviewBytes binds one derived JPEG preview to its existing managed image asset.
// stageComposerAttachmentPreviewBytes 将一份派生 JPEG 预览绑定到既有托管图片资产。
export async function stageComposerAttachmentPreviewBytes(
  projectId: string | null,
  conversationId: string | null,
  attachmentId: string,
  file: File,
): Promise<string> {
  if (!hasServiceRuntime()) {
    throw new Error("Browser service is unavailable, so the attachment preview cannot be staged.");
  }
  const dataBase64 = await fileToBase64(file);
  return serviceClient.request<string>("service_stage_composer_attachment_preview_bytes", {
    projectId,
    conversationId,
    attachmentId,
    dataBase64,
  });
}

// fileToBase64 converts a browser File into a standard base64 payload.
// fileToBase64 将浏览器 File 转换为标准 base64 载荷。
async function fileToBase64(file: File): Promise<string> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunkSize));
  }
  return btoa(binary);
}
