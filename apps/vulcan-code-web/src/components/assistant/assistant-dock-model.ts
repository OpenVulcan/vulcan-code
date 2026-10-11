/** One floating window holding one or more assistants; only the active one is shown.
 * 一个容纳一个或多个助手的浮动窗口；只显示当前激活的助手。 */
export interface AssistantWindow {
  /** Stable slot id; it also keys the saved window geometry, so freed slots are reused.
   * 稳定槽位标识；同时作为窗口几何的保存键，因此空出的槽位会被重用。 */
  id: string;
  /** Assistants in this window, in rail order.
   * 窗口内的助手，按侧栏顺序排列。 */
  assistantIds: string[];
  /** Assistant currently shown in this window.
   * 窗口当前显示的助手。 */
  activeId: string;
}

/** Visible assistant windows plus their most-recently-used order.
 * 可见的助手窗口及其最近使用顺序。 */
export interface AssistantDockState {
  windows: AssistantWindow[];
  /** Window ids, most recently used first; new assistants join the first one.
   * 窗口标识，最近使用者在前；新打开的助手加入第一个窗口。 */
  recent: string[];
}

export const EMPTY_ASSISTANT_DOCK: AssistantDockState = { windows: [], recent: [] };

/** Returns the window currently holding the assistant, or null when it is hidden.
 * 返回当前容纳该助手的窗口；助手隐藏时返回 null。 */
export function assistantWindowOf(state: AssistantDockState, assistantId: string): AssistantWindow | null {
  return state.windows.find((window) => window.assistantIds.includes(assistantId)) ?? null;
}

/** Shows a hidden assistant, or hides a visible one.
 * 显示隐藏的助手，或隐藏可见的助手。 */
export function toggleAssistant(state: AssistantDockState, assistantId: string): AssistantDockState {
  return assistantWindowOf(state, assistantId)
    ? hideAssistant(state, assistantId)
    : showAssistant(state, assistantId);
}

/** Shows the assistant: activates it where it already is, else adds it to the most recent window.
 * 显示助手：已在窗口中时激活它，否则加入最近使用的窗口。 */
export function showAssistant(state: AssistantDockState, assistantId: string): AssistantDockState {
  const current = assistantWindowOf(state, assistantId);
  if (current) return activateAssistant(state, current.id, assistantId);
  const target = state.windows.find((window) => window.id === state.recent[0]);
  if (!target) return openWindow(state, [assistantId]);
  return focusWindow(replaceWindow(state, {
    ...target,
    assistantIds: [...target.assistantIds, assistantId],
    activeId: assistantId,
  }), target.id);
}

/** Removes the assistant from its window; an emptied window closes.
 * 将助手移出其窗口；清空的窗口随之关闭。 */
export function hideAssistant(state: AssistantDockState, assistantId: string): AssistantDockState {
  const current = assistantWindowOf(state, assistantId);
  if (!current) return state;
  const assistantIds = current.assistantIds.filter((id) => id !== assistantId);
  if (assistantIds.length === 0) return closeAssistantWindow(state, current.id);
  const removedIndex = current.assistantIds.indexOf(assistantId);
  const activeId = current.activeId === assistantId
    ? assistantIds[Math.min(removedIndex, assistantIds.length - 1)]
    : current.activeId;
  return replaceWindow(state, { ...current, assistantIds, activeId });
}

/** Switches the window to one of its assistants and brings it forward.
 * 将窗口切换到其中一个助手，并置于最近使用。 */
export function activateAssistant(state: AssistantDockState, windowId: string, assistantId: string): AssistantDockState {
  const window = state.windows.find((candidate) => candidate.id === windowId);
  if (!window || !window.assistantIds.includes(assistantId)) return state;
  return focusWindow(replaceWindow(state, { ...window, activeId: assistantId }), windowId);
}

/** Moves the assistant out of a shared window into a window of its own.
 * 将助手从共享窗口移到独立窗口。 */
export function detachAssistant(state: AssistantDockState, windowId: string, assistantId: string): AssistantDockState {
  const window = state.windows.find((candidate) => candidate.id === windowId);
  if (!window || window.assistantIds.length < 2 || !window.assistantIds.includes(assistantId)) return state;
  return openWindow(hideAssistant(state, assistantId), [assistantId]);
}

/** Pulls every other window's assistants into this window, keeping its active assistant.
 * 将其他所有窗口的助手并入此窗口，并保留其当前激活的助手。 */
export function mergeAssistantWindows(state: AssistantDockState, windowId: string): AssistantDockState {
  const window = state.windows.find((candidate) => candidate.id === windowId);
  if (!window) return state;
  const others = state.windows.filter((candidate) => candidate.id !== windowId);
  const merged: AssistantWindow = {
    ...window,
    assistantIds: [...window.assistantIds, ...others.flatMap((candidate) => candidate.assistantIds)],
  };
  return { windows: [merged], recent: [windowId] };
}

/** Closes the window and hides every assistant in it.
 * 关闭窗口并隐藏其中所有助手。 */
export function closeAssistantWindow(state: AssistantDockState, windowId: string): AssistantDockState {
  return {
    windows: state.windows.filter((window) => window.id !== windowId),
    recent: state.recent.filter((id) => id !== windowId),
  };
}

/** Marks the window as most recently used.
 * 将窗口标记为最近使用。 */
export function focusWindow(state: AssistantDockState, windowId: string): AssistantDockState {
  if (state.recent[0] === windowId) return state;
  return { ...state, recent: [windowId, ...state.recent.filter((id) => id !== windowId)] };
}

/** Drops assistants that no longer exist, e.g. restored to sessions or deleted.
 * 移除已不存在的助手，例如已还原为会话或已删除。 */
export function retainAssistants(state: AssistantDockState, assistantIds: ReadonlySet<string>): AssistantDockState {
  let next = state;
  for (const window of state.windows) {
    for (const id of window.assistantIds) {
      if (!assistantIds.has(id)) next = hideAssistant(next, id);
    }
  }
  return next;
}

/** Opens a new window in the lowest free slot.
 * 在最小的空闲槽位打开新窗口。 */
function openWindow(state: AssistantDockState, assistantIds: string[]): AssistantDockState {
  const used = new Set(state.windows.map((window) => window.id));
  let slot = 1;
  while (used.has(`assistant-window-${slot}`)) slot += 1;
  const id = `assistant-window-${slot}`;
  return {
    windows: [...state.windows, { id, assistantIds, activeId: assistantIds[0] }],
    recent: [id, ...state.recent],
  };
}

/** Replaces one window by id.
 * 按标识替换一个窗口。 */
function replaceWindow(state: AssistantDockState, next: AssistantWindow): AssistantDockState {
  return { ...state, windows: state.windows.map((window) => (window.id === next.id ? next : window)) };
}
