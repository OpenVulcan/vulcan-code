import * as React from "react";
import { loadMemorySettings, saveMemorySettings } from "@/service/markdown-memory-service";
import { errorMessage } from "@/lib/display-format";
import type { MemoryMode, MemorySettingsView } from "@/types";

// MARKDOWN_MEMORY_SECTIONS are the memory page sections shown in Markdown mode, one per scope.
// MARKDOWN_MEMORY_SECTIONS 是 Markdown 模式下记忆页面的分区，每层一个。
export const MARKDOWN_MEMORY_SECTIONS = ["md_user", "md_space", "md_project"] as const;

// MEMORY_MODE_SECTION is the settings section that chooses the memory mode in every mode.
// MEMORY_MODE_SECTION 是在任何模式下选择记忆模式的设置分区。
export const MEMORY_MODE_SECTION = "memory_mode";

// MarkdownMemorySectionId identifies one Markdown scope section.
// MarkdownMemorySectionId 标识一个 Markdown 层分区。
export type MarkdownMemorySectionId = (typeof MARKDOWN_MEMORY_SECTIONS)[number];

// isMarkdownMemorySection reports whether a section id belongs to the Markdown scope pages.
// isMarkdownMemorySection 判断分区标识是否属于 Markdown 层页面。
export function isMarkdownMemorySection(section: string): section is MarkdownMemorySectionId {
  return (MARKDOWN_MEMORY_SECTIONS as readonly string[]).includes(section);
}

// memorySectionForMode returns the section the page can show under a mode; null keeps the request while loading.
// memorySectionForMode 返回某模式下页面可显示的分区；模式加载中（null）时保留请求的分区。
export function memorySectionForMode(section: string, mode: MemoryMode | null): string {
  if (section === MEMORY_MODE_SECTION) return "inference";
  if (section === "inference" || section === "overview" || section === "advanced" || mode === null) return section;
  if (mode === "off") return "inference";
  if (mode === "markdown") return isMarkdownMemorySection(section) ? section : "md_user";
  return isMarkdownMemorySection(section) ? "overview" : section;
}

// current is the last settings read or saved in this window; listeners re-render on change.
// current 是本窗口最近读取或保存的设置；listeners 在变化时重新渲染。
let current: MemorySettingsView | null = null;
// loadError is the last failed read while no settings are known; it clears on the next attempt.
// loadError 是尚无设置时最近一次读取失败的原因；下一次尝试时清除。
let loadError: string | null = null;
let pending: Promise<void> | null = null;
const listeners = new Set<() => void>();

// publishMemorySettings records settings returned by the service and notifies every reader.
// publishMemorySettings 记录服务返回的设置并通知全部读取方。
export function publishMemorySettings(settings: MemorySettingsView): void {
  current = settings;
  loadError = null;
  notify();
}

// Saves an explicit mode selection with the current persisted budget and republishes the returned settings.
// 使用当前已保存的预算写入用户明确选择的模式，并发布服务返回的设置。
export async function applyMemoryMode(mode: MemoryMode): Promise<void> {
  // Use the persisted budget so a shortcut does not apply another form's draft.
  // 使用已保存的预算，避免快捷操作应用其他表单的草稿。
  const currentSettings = await loadMemorySettings();
  publishMemorySettings(currentSettings.mode === mode
    ? currentSettings
    : await saveMemorySettings({ mode, budget: currentSettings.budget }));
}

// notify re-renders every reader.
// notify 重新渲染全部读取方。
function notify(): void {
  for (const listener of listeners) listener();
}

// ensureMemorySettingsLoaded starts one read; a failed read keeps the settings null, records the reason,
// and is retried by the next reader or by retryMemorySettings.
// ensureMemorySettingsLoaded 发起一次读取；失败时设置保持 null 并记录原因，由下一个读取方或 retryMemorySettings 重试。
function ensureMemorySettingsLoaded(): void {
  if (current !== null || pending !== null) return;
  if (loadError !== null) {
    loadError = null;
    notify();
  }
  pending = loadMemorySettings()
    .then(publishMemorySettings, (error: unknown) => {
      loadError = errorMessage(error);
      notify();
    })
    .finally(() => {
      pending = null;
    });
}

// retryMemorySettings reads the settings again after a failed read.
// retryMemorySettings 在读取失败后重新读取设置。
export function retryMemorySettings(): void {
  ensureMemorySettingsLoaded();
}

// useMemorySettingsLoadError returns why the settings could not be read, or null while none failed.
// useMemorySettingsLoadError 返回设置无法读取的原因；没有失败时为 null。
export function useMemorySettingsLoadError(): string | null {
  return React.useSyncExternalStore(subscribe, () => loadError, () => loadError);
}

// subscribe registers a listener for the external store.
// subscribe 为外部存储注册监听器。
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

// useMemorySettings returns the saved memory settings, or null until the first read succeeds.
// useMemorySettings 返回已保存的记忆设置；首次读取成功前为 null。
export function useMemorySettings(): MemorySettingsView | null {
  const settings = React.useSyncExternalStore(subscribe, () => current, () => current);
  React.useEffect(() => {
    ensureMemorySettingsLoaded();
  }, []);
  return settings;
}

// useMemorySettingsSnapshot follows the settings without starting a read, for surfaces outside the memory page.
// useMemorySettingsSnapshot 跟随设置但不发起读取，供记忆页面之外的界面使用。
export function useMemorySettingsSnapshot(): MemorySettingsView | null {
  return React.useSyncExternalStore(subscribe, () => current, () => current);
}
