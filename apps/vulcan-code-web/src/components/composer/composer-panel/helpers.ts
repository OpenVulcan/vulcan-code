import { isComposerAiCapabilityKind, type AiCapabilityKind } from "@/components/composer/composer-model";
import type { ComposerAttachmentSummary } from "@/types";
export interface ComposerSessionMemoryDraft {
  // input is the raw textarea value, including user-authored whitespace.
  // input 是原始输入框内容，保留用户输入的空白字符。
  input: string;
  // attachments are staged files that should travel with the draft input.
  // attachments 是应随草稿输入一起保留的已暂存文件。
  attachments: ComposerAttachmentSummary[];
}

// composerSessionDraftMemory keeps per-session composer drafts for the current WebUI process.
// composerSessionDraftMemory 在当前 WebUI 进程内保存按会话分槽的输入区草稿。
const composerSessionDraftMemory = new Map<string, ComposerSessionMemoryDraft>();

// composerShortcutHint returns a platform-specific keyboard hint for the composer placeholder.
// composerShortcutHint 返回输入区 placeholder 使用的平台化快捷键提示。
export function composerShortcutHint(baseHint: string): string {
  const platformText = typeof navigator === "undefined"
    ? ""
    : `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
  if (platformText.includes("mac")) {
    return baseHint;
  }
  return baseHint;
}

// composerSessionDraftKey returns the in-memory slot for one selected or draft conversation.
// composerSessionDraftKey 返回当前选中会话或新建会话对应的内存草稿槽。
export function composerSessionDraftKey(projectId: string | null, conversationId: string | null): string {
  if (conversationId) {
    return `conversation:${conversationId}`;
  }
  return `new:${projectId?.trim() || "workspace"}`;
}

// cloneComposerSessionMemoryDraft returns a detached draft copy for safe React state use.
// cloneComposerSessionMemoryDraft 返回脱离原对象的草稿副本，供 React 状态安全使用。
function cloneComposerSessionMemoryDraft(draft: ComposerSessionMemoryDraft): ComposerSessionMemoryDraft {
  return {
    input: draft.input,
    attachments: draft.attachments.map((attachment) => ({ ...attachment })),
  };
}

// emptyComposerSessionMemoryDraft creates an empty draft object.
// emptyComposerSessionMemoryDraft 创建空输入区草稿对象。
function emptyComposerSessionMemoryDraft(): ComposerSessionMemoryDraft {
  return {
    input: "",
    attachments: [],
  };
}

// readComposerSessionMemoryDraft reads one draft slot without sharing mutable arrays.
// readComposerSessionMemoryDraft 读取单个草稿槽，并避免共享可变数组。
export function readComposerSessionMemoryDraft(key: string): ComposerSessionMemoryDraft {
  const draft = composerSessionDraftMemory.get(key);
  return draft ? cloneComposerSessionMemoryDraft(draft) : emptyComposerSessionMemoryDraft();
}

// writeComposerSessionMemoryDraft saves or clears one draft slot.
// writeComposerSessionMemoryDraft 保存或清理单个草稿槽。
export function writeComposerSessionMemoryDraft(
  key: string,
  input: string,
  attachments: ComposerAttachmentSummary[],
): void {
  if (!input && attachments.length === 0) {
    composerSessionDraftMemory.delete(key);
    return;
  }
  composerSessionDraftMemory.set(key, {
    input,
    attachments: attachments.map((attachment) => ({ ...attachment })),
  });
}

// defaultAiCapabilityTargetOrderFromRecord restores a typed route order map from a saved request payload.
// defaultAiCapabilityTargetOrderFromRecord 从已保存请求载荷恢复类型化路由顺序映射。
export function defaultAiCapabilityTargetOrderFromRecord(record: Record<string, string[]>): Map<AiCapabilityKind, string[]> {
  const order = new Map<AiCapabilityKind, string[]>();
  for (const [kind, selectors] of Object.entries(record)) {
    if (isComposerAiCapabilityKind(kind)) {
      order.set(kind, [...selectors]);
    }
  }
  return order;
}

