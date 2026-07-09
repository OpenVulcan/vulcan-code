import type { AiCapabilityKind } from "@/components/composer/composer-model";
import type {
  CommandSessionSummary,
  ComposerAttachmentSummary,
  PendingPromptSummary,
  SessionAddonsChangedResponse,
  SubagentPanelRouteResponse,
  SubmitConversationPromptAck,
  UserPromptEditDraftSummary,
  WorkbenchSnapshot,
} from "@/types";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";
import { updateBrowserFallbackWorkbench } from "@/service/browser-fallback-workbench";

// submitConversationPrompt sends one composer prompt to the desktop conversation runtime.
// submitConversationPrompt 将单条输入区提示词发送到桌面会话运行时。
export async function submitConversationPrompt(
  projectId: string | null,
  conversationId: string | null,
  prompt: string,
  hiddenModelGuidance: string | undefined,
  attachments: ComposerAttachmentSummary[],
  executionMode: string,
  reasoningEffort: string | null,
  disabledAiCapabilityKinds: AiCapabilityKind[],
  disabledAiCapabilityTargetSelectors: string[],
  aiCapabilityTargetOrderOverrides: Record<string, string[]>,
): Promise<SubmitConversationPromptAck | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SubmitConversationPromptAck>("service_submit_conversation_prompt", {
    projectId,
    conversationId,
    prompt,
    hiddenModelGuidance,
    attachments,
    executionMode,
    reasoningEffort,
    disabledAiCapabilityKinds,
    disabledAiCapabilityTargetSelectors,
    aiCapabilityTargetOrderOverrides,
  });
}

// saveSessionAiCapabilityConfiguration persists the composer capability dialog draft for one session.
// saveSessionAiCapabilityConfiguration 将输入区能力弹窗草稿持久化到单个会话。
export async function saveSessionAiCapabilityConfiguration(
  projectId: string | null,
  conversationId: string | null,
  disabledAiCapabilityKinds: AiCapabilityKind[],
  disabledAiCapabilityTargetSelectors: string[],
  aiCapabilityTargetOrderOverrides: Record<string, string[]>,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_session_ai_capability_configuration", {
    request: {
      projectId,
      conversationId,
      disabledAiCapabilityKinds,
      disabledAiCapabilityTargetSelectors,
      aiCapabilityTargetOrderOverrides,
    },
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

// stopConversationRun stops the selected backend conversation run.
// stopConversationRun 停止当前选中的后端会话运行。
export async function stopConversationRun(mode: StopConversationRunMode): Promise<StopConversationRunResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<StopConversationRunResponse>("service_stop_conversation_run", { mode });
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

// getUserPromptEditDraft loads a backend-validated editable prompt draft.
// getUserPromptEditDraft 加载一份后端校验过的可编辑提示词草稿。
export async function getUserPromptEditDraft(
  conversationId: string,
  messageId: string,
): Promise<UserPromptEditDraftSummary | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<UserPromptEditDraftSummary>("service_get_user_prompt_edit_draft", {
    conversationId,
    messageId,
  });
}

// submitUserPromptEdit replaces one prompt and immediately replays the edited turn.
// submitUserPromptEdit 替换单条提示词并立即重新执行编辑后的回合。
export async function submitUserPromptEdit(request: {
  conversationId: string;
  messageId: string;
  prompt: string;
  attachments: ComposerAttachmentSummary[];
  stopRunning?: boolean;
}): Promise<SubmitConversationPromptAck | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<SubmitConversationPromptAck>("service_submit_user_prompt_edit", { request });
}

// forkConversation copies one persisted conversation into a new session and opens it.
// forkConversation 将一个持久化会话复制为新会话并打开。
export async function forkConversation(request: {
  conversationId: string;
  title: string;
  anchorMessageId?: string;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  const forkRequestStartedAt = performance.now();
  console.info("[fork-trace] frontend.service.request.start", {
    conversationId: request.conversationId,
    anchorMessageId: request.anchorMessageId ?? null,
  });
  try {
    const snapshot = await serviceClient.request<WorkbenchSnapshot>("service_fork_conversation", { request });
    console.info("[fork-trace] frontend.service.request.success", {
      conversationId: request.conversationId,
      selectedConversationId: snapshot.selectedConversationId,
      elapsedMs: Math.round(performance.now() - forkRequestStartedAt),
      conversations: snapshot.conversations.length,
      messages: snapshot.messages.length,
    });
    return snapshot;
  } catch (error) {
    console.info("[fork-trace] frontend.service.request.error", {
      conversationId: request.conversationId,
      elapsedMs: Math.round(performance.now() - forkRequestStartedAt),
      error: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
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
  status: string,
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
