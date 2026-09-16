import React from "react";
import { createConversationDisplayOverrides } from "./conversation-display-overrides";
import { normalizeThinkingDisplayMode, type ThinkingDisplayMode } from "./thinking-display";

// Popup overrides are local display choices, separate from the existing service-owned global preference.
// 弹窗覆盖值是本地展示选择，与既有服务持有的全局偏好分离。
const popupOverrides = createConversationDisplayOverrides("vulcan-thinking-popup-overrides-v1");
// Null resets inheritance; draft transfer never changes another session.
// null 恢复继承；草稿转移不改变其他会话。
export const setConversationThinkingPopupOverride = popupOverrides.set;
export const moveConversationThinkingPopupOverride = popupOverrides.move;

/** Supplies the authoritative system preference and the existing service save operation to its settings page.
 * 向设置页面提供权威系统偏好及既有服务保存操作。
 */
export interface ThinkingDisplaySystemPreference {
  // Existing service-owned global mode; old values are normalized by the UI.
  // 既有服务持有的全局模式；旧值由界面规范化。
  mode: string | null | undefined;
  // Resolves true only when the system default was persisted.
  // 仅在系统默认值已持久化时返回 true。
  save?: (mode: ThinkingDisplayMode) => Promise<boolean>;
}

// Standalone settings surfaces cannot save without an explicitly provided service action.
// 独立设置界面在没有明确提供服务操作时不可保存。
export const ThinkingDisplaySystemContext = React.createContext<ThinkingDisplaySystemPreference>({ mode: "popup_only" });

/** Resolves sessionId against the authoritative globalMode and returns its effective mode and explicit override.
 * 将 sessionId 与权威 globalMode 组合解析，返回生效模式及明确覆盖值。
 */
export function useThinkingDisplayPreference(sessionId: string | null, globalMode: string | null | undefined) {
  // Primitive snapshots isolate subscriptions without mirroring global values into session storage.
  // 原始值快照隔离订阅，不把全局值镜像到会话存储。
  const getSnapshot = React.useCallback(() => popupOverrides.get(sessionId), [sessionId]);
  const override = React.useSyncExternalStore(popupOverrides.subscribe, getSnapshot, getSnapshot);
  // mode preserves the popup/inline distinction and normalizes legacy global preferences.
  // mode 保留弹窗／内联区别，同时规范化旧版全局偏好。
  const mode: ThinkingDisplayMode = override === undefined
    ? normalizeThinkingDisplayMode(globalMode)
    : override ? "popup_only" : "inline_output";
  /** Applies only this session's explicit display mode; returns without writing when no session is selected.
   * 仅应用此会话的明确展示模式；没有选中会话时不写入直接返回。
   */
  const selectMode = React.useCallback((nextMode: ThinkingDisplayMode): void => {
    if (sessionId !== null) popupOverrides.set(sessionId, normalizeThinkingDisplayMode(nextMode) === "popup_only");
  }, [sessionId]);
  return { mode, override, selectMode };
}
