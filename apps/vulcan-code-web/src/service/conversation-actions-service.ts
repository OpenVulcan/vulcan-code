import type { AiCapabilityKind } from "@/components/composer/composer-model";
import type {
  ComposerAttachmentSummary,
  PendingPromptSummary,
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
  reasoningEffort: string,
  disabledAiCapabilityKinds: AiCapabilityKind[],
  disabledAiCapabilityTargetSelectors: string[],
  aiCapabilityTargetOrderOverrides: Record<string, string[]>,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_submit_conversation_prompt", {
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

// stopConversationRun stops the selected backend conversation run.
// stopConversationRun 停止当前选中的后端会话运行。
export async function stopConversationRun(): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_stop_conversation_run");
}

// stopCommandSession force-stops one live backend command session.
// stopCommandSession 强制停止单条实时后端命令会话。
export async function stopCommandSession(commandSessionId: string, ownerSessionId?: string | null): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return updateBrowserFallbackWorkbench((snapshot) => {
      snapshot.commandSessions = snapshot.commandSessions.map((session) => session.id === commandSessionId
        ? { ...session, ownerSessionId: ownerSessionId ?? session.ownerSessionId ?? null, status: "stopped", preview: "浏览器预览已标记停止。" }
        : session);
    });
  }
  return serviceClient.request<WorkbenchSnapshot>("service_stop_command_session", {
    commandSessionId,
    ownerSessionId: ownerSessionId ?? null,
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
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_submit_user_prompt_edit", { request });
}

// forkConversation copies one persisted conversation into a new session and opens it.
// forkConversation 将一个持久化会话复制为新会话并打开。
export async function forkConversation(request: {
  conversationId: string;
  title: string;
  beforeMessageId?: string | null;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_fork_conversation", { request });
}

// saveSessionGoal persists a user-authored goal for the current conversation.
// saveSessionGoal 为当前会话持久化用户编写的目标。
export async function saveSessionGoal(
  conversationId: string | null,
  objective: string,
  tokenBudget?: number | null,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_session_goal", {
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
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_resume_session_goal", {
    request: {
      conversationId,
      objective,
      tokenBudget: tokenBudget ?? null,
    },
  });
}

// clearSessionGoal removes the persisted goal for the current conversation.
// clearSessionGoal 移除当前会话的持久化目标。
export async function clearSessionGoal(conversationId: string | null): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_clear_session_goal", {
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
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_update_session_goal_status", {
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
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_session_plan_mode", {
    request: {
      conversationId,
      enabled,
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
export async function replyPendingQuestion(requestId: string, answers: string[][]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reply_pending_question", { requestId, answers });
}

// rejectPendingQuestion cancels one pending ask-tool request after explicit confirmation.
// rejectPendingQuestion 在明确确认后取消一个待处理 ask 工具请求。
export async function rejectPendingQuestion(requestId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reject_pending_question", { requestId });
}

// openSubagentPanel asks the backend to switch the active transcript surface to a child task.
// openSubagentPanel 请求后端把活动转录界面切换到子任务。
export async function openSubagentPanel(parentConversationId: string, taskId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_open_subagent_panel", { parentConversationId, taskId });
}

// closeSubagentPanel asks the backend to return the active transcript surface to the parent conversation.
// closeSubagentPanel 请求后端将活动转录界面返回父会话。
export async function closeSubagentPanel(): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_close_subagent_panel");
}

// stopSubagentTask asks the backend to stop one child task through the shared task runtime.
// stopSubagentTask 请求后端通过共享任务运行时停止单个子任务。
export async function stopSubagentTask(parentConversationId: string, taskId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_stop_subagent_task", { parentConversationId, taskId });
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
export async function insertPendingPrompt(entryId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_insert_pending_prompt", { entryId });
}

// removePendingPrompt removes one queued prompt from the selected conversation queue.
// removePendingPrompt 从当前选中会话队列移除单条排队提示词。
export async function removePendingPrompt(entryId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_pending_prompt", { entryId });
}

// reorderPendingPrompts persists one service-compatible pending prompt drop target.
// reorderPendingPrompts 持久化一次与服务兼容的待发送提示词拖放落点。
export async function reorderPendingPrompts(
  conversationId: string | null,
  draggedEntryId: string,
  targetEntryId: string | null,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reorder_pending_prompts", { conversationId, draggedEntryId, targetEntryId });
}

// stageComposerAttachmentBytes stages one browser/drop/paste provided file through the service.
// stageComposerAttachmentBytes 通过服务暂存一个由浏览器、拖拽或粘贴提供的文件。
export async function stageComposerAttachmentBytes(
  projectId: string | null,
  conversationId: string | null,
  file: File,
): Promise<ComposerAttachmentSummary | null> {
  if (!hasServiceRuntime()) {
    throw new Error("浏览器服务不可用，无法暂存附件。");
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
