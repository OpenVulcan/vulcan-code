import React from "react";
import { createConversationDisplayOverrides } from "./conversation-display-overrides";

// Existing storage identity keeps all previously selected session choices intact.
// 既有存储标识确保之前选择的会话偏好全部保留。
const toolCollapseOverrides = createConversationDisplayOverrides("vulcan-tool-collapse-overrides-v1");

// Explicit session writes and acknowledged draft transfers share the same validated store.
// 明确会话写入和已确认草稿转移共用同一个经过验证的存储。
export const setConversationToolCollapseOverride = toolCollapseOverrides.set;
export const moveConversationToolCollapseOverride = toolCollapseOverrides.move;

/** Shared default and active footer scope; saveDefault persists only an explicit system-settings action.
 * 共享默认值及底栏当前作用域；saveDefault 仅持久化明确的系统设置操作。
 */
export interface ToolCollapsePreferences {
  // Authoritative system default; absent legacy configuration resolves to true.
  // 权威系统默认值；旧配置缺省时解析为 true。
  defaultEnabled: boolean;
  // Active session or local draft; null means no editable conversation.
  // 当前会话或本地草稿；null 表示没有可编辑会话。
  activeConversationId: string | null;
  // Persists a system default and rejects on failure.
  // 持久化系统默认值，失败时拒绝。
  saveDefault?: (enabled: boolean) => Promise<void>;
}

// Standalone transcript renderers preserve the historical enabled default.
// 独立转录渲染器保留原有默认开启行为。
export const ToolCollapsePreferencesContext = React.createContext<ToolCollapsePreferences>({
  defaultEnabled: true,
  activeConversationId: null,
});

/** Resolves one exact session (or the active footer scope); returns effective and explicit choices.
 * 解析一个精确会话（或底栏当前作用域）；返回生效值与明确覆盖值。
 */
export function useToolCollapsePreference(sessionId?: string | null) {
  // preferences supplies the service default independently of local display choices.
  // preferences 提供独立于本地展示选择的服务默认值。
  const preferences = React.useContext(ToolCollapsePreferencesContext);
  // scope uses the active ID only when the caller intentionally omits an explicit surface ID.
  // scope 仅在调用方有意省略明确界面标识时使用当前标识。
  const scope = sessionId === undefined ? preferences.activeConversationId : sessionId;
  // getSnapshot returns a primitive so unrelated sessions never trigger a rendering loop.
  // getSnapshot 返回原始值，避免无关会话导致渲染循环。
  const getSnapshot = React.useCallback(() => toolCollapseOverrides.get(scope), [scope]);
  // override distinguishes explicit false from inherited defaults.
  // override 区分明确关闭与继承默认值。
  const override = React.useSyncExternalStore(toolCollapseOverrides.subscribe, getSnapshot, getSnapshot);
  return { ...preferences, scope, override, enabled: override ?? preferences.defaultEnabled };
}
