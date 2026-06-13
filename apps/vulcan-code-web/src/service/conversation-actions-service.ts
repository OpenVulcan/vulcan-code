import type { AiCapabilityKind } from "@/components/composer/composer-model";
import type {
  ComposerAttachmentSummary,
  PendingPromptSummary,
  WorkbenchSnapshot,
} from "@/types";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// submitConversationPrompt sends one composer prompt to the desktop conversation runtime.
// submitConversationPrompt 将单条输入区提示词发送到桌面会话运行时。
export async function submitConversationPrompt(
  projectId: string | null,
  conversationId: string | null,
  prompt: string,
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
    attachments,
    executionMode,
    reasoningEffort,
    disabledAiCapabilityKinds,
    disabledAiCapabilityTargetSelectors,
    aiCapabilityTargetOrderOverrides,
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

// reorderPendingPrompts persists one iced-compatible pending prompt drop target.
// reorderPendingPrompts 持久化一次与 iced 兼容的待发送提示词拖放落点。
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
