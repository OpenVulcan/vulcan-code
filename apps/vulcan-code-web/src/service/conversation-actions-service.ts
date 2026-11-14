import type {
  CommandSessionSummary,
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentPreparationSummary,
  ComposerAttachmentSummary,
  PendingPromptEditOutcome,
  ContextWindowSelection,
  EnvironmentValuesSubmitRequest,
  SessionAddonsChangedResponse,
  SessionGoalUserStatusUpdate,
  SessionRequestOptions,
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
  queuedInputActionId?: string,
): Promise<SubmitConversationPromptAck | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SubmitConversationPromptAck>(
    "service_continue_conversation",
    {
      conversationId,
      clientContinuationId,
      queuedInputActionId,
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
  /** Durable action id carrying the exact WebUI notification owner.
   * 携带精确 WebUI 通知所有者的可靠动作标识。
   */
  clientActionId: string;
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
  /** Parent terminal TurnStats seq retained as the child's inclusive history boundary.
   * 作为子会话包含式历史边界保留的父级终态 TurnStats seq。
   */
  sourceTurnEndSeq: number;
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
  /** Parent terminal TurnStats seq retained by the child.
   * 子会话保留的父级终态 TurnStats seq。
   */
  sourceTurnEndSeq: number;
  /** Child event-window generation.
   * 子事件窗口代次。
   */
  windowGeneration: number;
  /** Child complete document revision.
   * 子会话完整文档修订号。
   */
  documentRevision: number;
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

// cancelConversationSubmission requests cancellation by client submission id and acknowledges only after the exact upload owner settles.
// cancelConversationSubmission 按客户端提交标识请求取消，并仅在精确上传所有者结算后确认。
export async function cancelConversationSubmission(clientSubmissionId: string): Promise<boolean> {
  if (!hasServiceRuntime()) {
    // No transport cannot prove that the provider operation has settled; only an actual RPC may report absence.
    // 缺少传输不能证明供应商操作已结算；只有真实 RPC 才能报告操作不存在。
    throw new Error("Submission cancellation requires an available service runtime.");
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

/** Reprojects the exact retained command-session runtime after a durable terminal mutation.
 * 在可靠终端变更后重新投影精确保留的命令会话运行时。
 *
 * Parameters: none.
 * 参数：无。
 * Returns: the authoritative current command-session summaries.
 * 返回值：权威的当前命令会话摘要。
 */
export async function refreshCommandSessions(): Promise<CommandSessionSummary[]> {
  if (!hasServiceRuntime()) {
    return [];
  }
  return serviceClient.request<CommandSessionSummary[]>("service_refresh_command_sessions");
}

// TerminalInputEvent describes one structured terminal input sent through the service runtime.
// TerminalInputEvent 描述一条通过服务运行时发送的结构化终端输入。
export type TerminalInputEvent =
  | { kind: "text"; text: string }
  | { kind: "paste"; text: string }
  | { kind: "key"; key: string }
  | { kind: "chord"; key: string; ctrl?: boolean; alt?: boolean; shift?: boolean; meta?: boolean };

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

// GitPullActionResponse describes a completed fast-forward pull of the current branch.
// GitPullActionResponse 描述当前分支一次已完成的快进拉取。
export interface GitPullActionResponse {
  // Human-readable operation summary.
  // 面向用户的操作摘要。
  summary: string;
  // Whether HEAD moved; false means the branch was already up to date.
  // HEAD 是否移动；为 false 表示分支已是最新。
  updated: boolean;
  // Current branch after the pull.
  // 拉取后的当前分支。
  branchName?: string | null;
  // Short hash of HEAD after the pull.
  // 拉取后 HEAD 的短哈希。
  commitHash?: string | null;
  // Commit message subject of HEAD after the pull.
  // 拉取后 HEAD 的提交信息标题。
  commitMessage?: string | null;
}

// runGitPullAction asks the backend to fast-forward the current branch from its upstream.
// runGitPullAction 请求后端从上游快进当前分支。
export async function runGitPullAction(request: {
  conversationId: string;
  workspacePath: string;
  repositoryKey?: string | null;
}): Promise<GitPullActionResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<GitPullActionResponse>("service_run_git_pull_action", { request });
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

// getSessionRequestOptions reads the cache lifetime and service tier stored for one session.
// getSessionRequestOptions 读取一个会话已保存的缓存时长与服务档位。
export async function getSessionRequestOptions(conversationId: string): Promise<SessionRequestOptions | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SessionRequestOptions>("service_get_session_request_options", {
    request: { conversationId },
  });
}

// setSessionRequestOptions replaces the cache lifetime and service tier of one session; null means default.
// setSessionRequestOptions 替换一个会话的缓存时长与服务档位；null 表示默认值。
export async function setSessionRequestOptions(options: SessionRequestOptions): Promise<SessionRequestOptions | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SessionRequestOptions>("service_set_session_request_options", {
    request: options,
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
  draftSlotKey: string,
  generation: number,
  modelSelection: ComposerModelSelectionSnapshot,
  attachments: ComposerAttachmentSummary[],
  // Whether new images are downscaled and re-encoded before planning; unsupported formats are converted either way.
  // 是否在规划前缩小并重新编码新图片；不受支持的格式无论如何都会转换。
  compressImages: boolean,
  // Explicit user retry is the only way to restart an original upload without a live plan or operation.
  // 对没有活动计划或操作的原上传，只有用户显式重试才允许再次启动。
  retryUploads = false,
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
    { projectId, conversationId, draftSlotKey, generation, modelSelection, attachments, retryUploads, compressImages },
  );
}

// discardComposerAttachmentPlans removes unused view plans without cancelling any original upload.
// discardComposerAttachmentPlans 移除未使用视图计划，不取消任何原上传。
// Parameter: transferIds contains service-issued view identities; returns the number actually discarded.
// 参数：transferIds 包含服务签发视图身份；返回实际丢弃数量。
export async function discardComposerAttachmentPlans(transferIds: string[]): Promise<number> {
  if (!hasServiceRuntime()) throw new Error("Discarding attachment plans requires an available service runtime.");
  return serviceClient.request<number>("service_discard_composer_attachment_plans", { transferIds });
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
    // Only a real service response may acknowledge an unchanged set; unavailable transport must remain an error.
    // 只有真实服务响应可以确认集合未变；传输不可用必须保持错误。
    throw new Error("Attachment draft reconciliation requires an available service runtime.");
  }
  return serviceClient.request<boolean>("service_reconcile_composer_attachment_draft", {
    projectId,
    conversationId,
    draftSlotKey,
    attachments,
  });
}

// setSessionContextCompactionWindow persists one window selection (fixed node or custom value) or clears the session lock.
// setSessionContextCompactionWindow 持久化一个窗口选择（固定节点或自定义值），或清除会话锁定。
export async function setSessionContextCompactionWindow(
  conversationId: string | null,
  selection: ContextWindowSelection | null,
): Promise<SessionAddonsChangedResponse | null> {
  if (!hasServiceRuntime() || !conversationId) {
    return null;
  }
  return serviceClient.request<SessionAddonsChangedResponse>("service_set_session_context_compaction_window", {
    request: {
      conversationId,
      selection,
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

// submitEnvironmentValues stores the values entered for one value request, or declines it.
// submitEnvironmentValues 保存为一条取值请求输入的值，或拒绝该请求。
export async function submitEnvironmentValues(request: EnvironmentValuesSubmitRequest): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_submit_environment_values", { request });
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
export async function editPendingPrompt(entryId: string): Promise<PendingPromptEditOutcome | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<PendingPromptEditOutcome>("service_edit_pending_prompt", { entryId });
}

// insertPendingPrompt requests insertion of entryId in conversationId, defaulting to the selected conversation.
// insertPendingPrompt 请求在 conversationId 中插入 entryId，省略时使用当前选中的会话。
// Returns whether the service accepted the insertion request.
// 返回服务是否接收了插队请求。
export async function insertPendingPrompt(entryId: string, conversationId?: string): Promise<boolean> {
  if (!hasServiceRuntime()) {
    return false;
  }
  return serviceClient.request<boolean>("service_insert_pending_prompt", { entryId, conversationId });
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
  const dataBase64 = await blobToBase64(file);
  return serviceClient.request<ComposerAttachmentSummary>("service_stage_composer_attachment_bytes", {
    projectId,
    conversationId,
    fileName: file.name || "clipboard-attachment",
    mimeType: file.type || "",
    dataBase64,
  });
}

// Return only the file facts that determine delivery eligibility.
// 仅返回用于确定交付资格的文件信息。
function droppedComposerFileFacts(file: File) {
  return { fileName: file.name, mimeType: file.type, byteLen: file.size };
}

// Ask Core to classify dropped files in their current order without transferring their contents.
// 请求 Core 按当前顺序判定拖入文件，不传递文件内容。
export function classifyDroppedComposerFiles(
  conversationId: string | null, modelSelection: ComposerModelSelectionSnapshot | null, files: File[],
): Promise<boolean[]> {
  return serviceClient.request("service_classify_dropped_composer_files", {
    conversationId, modelSelection, files: files.map(droppedComposerFileFacts),
  });
}

// Stage an eligible dropped file under its draft owner and recheck the route at the write boundary.
// 以草稿所有者暂存符合条件的拖入文件，并在写入边界重新检查路线。
export async function stageDroppedComposerFile(
  projectId: string | null, conversationId: string | null, draftSlotKey: string,
  modelSelection: ComposerModelSelectionSnapshot | null, file: File,
): Promise<ComposerAttachmentSummary | null> {
  return serviceClient.request("service_stage_dropped_composer_file", {
    projectId, conversationId, draftSlotKey, modelSelection,
    file: droppedComposerFileFacts(file), dataBase64: await blobToBase64(file),
  });
}

// fetchModelAttachmentCapabilities reads the attachment gates of one exact model route; `bridged` is always false.
// fetchModelAttachmentCapabilities 读取一条精确模型路线的附件门控；`bridged` 恒为 false。
export async function fetchModelAttachmentCapabilities(
  providerId: string,
  connectionId: string,
  modelId: string,
): Promise<ComposerAttachmentCapabilitySummary[]> {
  if (!hasServiceRuntime()) {
    return [];
  }
  return serviceClient.request<ComposerAttachmentCapabilitySummary[]>("service_model_attachment_capabilities", {
    providerId,
    connectionId,
    modelId,
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
  const dataBase64 = await blobToBase64(file);
  return serviceClient.request<string>("service_stage_composer_attachment_preview_bytes", {
    projectId,
    conversationId,
    attachmentId,
    dataBase64,
  });
}

// blobToBase64 converts a browser Blob or File into a standard base64 payload.
// blobToBase64 将浏览器 Blob 或 File 转换为标准 base64 载荷。
export async function blobToBase64(file: Blob): Promise<string> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunkSize));
  }
  return btoa(binary);
}
