import React from "react";

// FileWindow is one file open in its own floating window of a session.
// FileWindow 是会话中在独立浮动窗口内打开的一个文件。
export interface FileWindow {
  // Slot id, unique within the session; it also keys the saved window geometry, so freed slots are reused.
  // 槽位标识，在会话内唯一；同时作为窗口几何的保存键，因此空出的槽位会被重用。
  id: string;
  // Absolute path of the file shown.
  // 所显示文件的绝对路径。
  path: string;
  // Whether the window is collapsed into its taskbar chip.
  // 窗口是否已收入其任务栏条目。
  minimized: boolean;
  // Bumped whenever the window should come to the front; never persisted.
  // 每次窗口需要置于最前时递增；不持久化。
  raise: number;
}

// FILE_WINDOWS_STORAGE_KEY holds every session's open file windows. They are UI layout of this browser, not session
// data, so they live in localStorage and never reach the service.
// FILE_WINDOWS_STORAGE_KEY 保存每个会话打开的文件窗口。它们属于本浏览器的界面布局而非会话数据，因此存放在
// localStorage 中，不会发往服务。
const FILE_WINDOWS_STORAGE_KEY = "vulcan.file-windows";

// sessions maps a session id to its open file windows, in taskbar order; listeners re-render their readers.
// sessions 将会话标识映射到其打开的文件窗口（按任务栏顺序）；listeners 使读取方重新渲染。
let sessions: Record<string, FileWindow[]> = loadSessions();
const listeners = new Set<() => void>();

// NO_WINDOWS is the shared empty list, so sessions without windows keep a stable snapshot.
// NO_WINDOWS 是共享的空列表，使没有窗口的会话保持稳定快照。
const NO_WINDOWS: FileWindow[] = [];

// useFileWindows returns the open file windows of a session.
// useFileWindows 返回会话打开的文件窗口。
export function useFileWindows(sessionId: string): FileWindow[] {
  return React.useSyncExternalStore(subscribe, () => sessions[sessionId] ?? NO_WINDOWS, () => NO_WINDOWS);
}

// openFileWindow shows `path` in a window of the session: an existing window of that file is restored and raised,
// otherwise a new one opens in the lowest free slot.
// openFileWindow 在会话的窗口中显示 `path`：该文件已有窗口时还原并置前，否则在最小的空闲槽位打开新窗口。
export function openFileWindow(sessionId: string, path: string): void {
  const windows = sessions[sessionId] ?? NO_WINDOWS;
  const existing = windows.find((entry) => entry.path === path);
  if (existing) return restoreFileWindow(sessionId, existing.id);
  const used = new Set(windows.map((entry) => entry.id));
  let slot = 1;
  while (used.has(`file-window-${slot}`)) slot += 1;
  update(sessionId, [...windows, { id: `file-window-${slot}`, path, minimized: false, raise: 0 }]);
}

// restoreFileWindow brings a window back from its chip, or to the front when it is already shown.
// restoreFileWindow 将窗口从任务栏条目还原；已显示时将其置于最前。
export function restoreFileWindow(sessionId: string, windowId: string): void {
  updateWindow(sessionId, windowId, (entry) => ({ ...entry, minimized: false, raise: entry.raise + 1 }));
}

// minimizeFileWindow collapses a window into its taskbar chip.
// minimizeFileWindow 将窗口收入其任务栏条目。
export function minimizeFileWindow(sessionId: string, windowId: string): void {
  updateWindow(sessionId, windowId, (entry) => ({ ...entry, minimized: true }));
}

// closeFileWindow closes a window and removes its chip.
// closeFileWindow 关闭窗口并移除其任务栏条目。
export function closeFileWindow(sessionId: string, windowId: string): void {
  update(sessionId, (sessions[sessionId] ?? NO_WINDOWS).filter((entry) => entry.id !== windowId));
}

// chips maps "<session> <window>" to the mounted taskbar chip each window minimizes into.
// chips 将“<会话> <窗口>”映射到各窗口最小化时收入的已挂载任务栏条目。
const chips = new Map<string, HTMLElement>();

// registerFileWindowChip records the chip element of a window while it is mounted; null forgets it.
// registerFileWindowChip 在任务栏条目挂载期间记录其元素；传入 null 时遗忘它。
export function registerFileWindowChip(sessionId: string, windowId: string, element: HTMLElement | null): void {
  if (element) chips.set(`${sessionId} ${windowId}`, element);
  else chips.delete(`${sessionId} ${windowId}`);
}

// fileWindowChip returns the mounted chip of a window, the target of its minimize animation.
// fileWindowChip 返回窗口已挂载的任务栏条目，即其最小化动画的目标。
export function fileWindowChip(sessionId: string, windowId: string): HTMLElement | null {
  return chips.get(`${sessionId} ${windowId}`) ?? null;
}

// subscribe registers a reader for changes.
// subscribe 为变化注册读取方。
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

// updateWindow replaces one window of a session through `change`.
// updateWindow 通过 `change` 替换会话中的一个窗口。
function updateWindow(sessionId: string, windowId: string, change: (entry: FileWindow) => FileWindow): void {
  update(sessionId, (sessions[sessionId] ?? NO_WINDOWS).map((entry) => (entry.id === windowId ? change(entry) : entry)));
}

// update stores a session's windows, saves every session and notifies readers.
// update 保存会话的窗口，持久化全部会话并通知读取方。
function update(sessionId: string, windows: FileWindow[]): void {
  const { [sessionId]: _previous, ...others } = sessions;
  sessions = windows.length > 0 ? { ...others, [sessionId]: windows } : others;
  try {
    window.localStorage.setItem(FILE_WINDOWS_STORAGE_KEY, JSON.stringify({
      version: 1,
      sessions: Object.fromEntries(Object.entries(sessions).map(([id, list]) => [
        id,
        list.map((entry) => ({ id: entry.id, path: entry.path, minimized: entry.minimized })),
      ])),
    }));
  } catch {
    // Storage may be full or blocked; the windows still work for this page.
    // 存储可能已满或被阻止；窗口在本页面中仍可使用。
  }
  listeners.forEach((listener) => listener());
}

// loadSessions reads the saved windows; anything malformed is dropped rather than guessed at.
// loadSessions 读取已保存的窗口；任何格式错误的内容都被丢弃而非猜测。
function loadSessions(): Record<string, FileWindow[]> {
  try {
    const saved = JSON.parse(window.localStorage.getItem(FILE_WINDOWS_STORAGE_KEY) ?? "null") as unknown;
    if (!isRecord(saved) || saved.version !== 1 || !isRecord(saved.sessions)) return {};
    const loaded: Record<string, FileWindow[]> = {};
    for (const [sessionId, list] of Object.entries(saved.sessions)) {
      if (!Array.isArray(list)) continue;
      const windows = list.filter((entry): entry is Omit<FileWindow, "raise"> => isRecord(entry)
        && typeof entry.id === "string" && typeof entry.path === "string" && typeof entry.minimized === "boolean");
      if (windows.length > 0) loaded[sessionId] = windows.map((entry) => ({ id: entry.id, path: entry.path, minimized: entry.minimized, raise: 0 }));
    }
    return loaded;
  } catch {
    return {};
  }
}

// isRecord tells plain objects apart from other JSON values.
// isRecord 区分普通对象与其他 JSON 值。
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
