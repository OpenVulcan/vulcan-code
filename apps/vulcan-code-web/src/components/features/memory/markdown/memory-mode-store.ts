import * as React from "react";
import { loadMemorySettings } from "@/service/markdown-memory-service";
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
  if (section === MEMORY_MODE_SECTION || mode === null) return section;
  if (mode === "off") return MEMORY_MODE_SECTION;
  if (mode === "markdown") return isMarkdownMemorySection(section) ? section : "md_user";
  return isMarkdownMemorySection(section) ? "overview" : section;
}

// current is the last settings read or saved in this window; listeners re-render on change.
// current 是本窗口最近读取或保存的设置；listeners 在变化时重新渲染。
let current: MemorySettingsView | null = null;
let pending: Promise<void> | null = null;
const listeners = new Set<() => void>();

// publishMemorySettings records settings returned by the service and notifies every reader.
// publishMemorySettings 记录服务返回的设置并通知全部读取方。
export function publishMemorySettings(settings: MemorySettingsView): void {
  current = settings;
  for (const listener of listeners) listener();
}

// ensureMemorySettingsLoaded starts one read; a failed read stays null and is retried by the next reader.
// ensureMemorySettingsLoaded 发起一次读取；失败时保持 null，由下一个读取方重试。
function ensureMemorySettingsLoaded(): void {
  if (current !== null || pending !== null) return;
  pending = loadMemorySettings()
    .then(publishMemorySettings, () => undefined)
    .finally(() => {
      pending = null;
    });
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
