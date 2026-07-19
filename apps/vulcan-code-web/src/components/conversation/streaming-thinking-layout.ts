// StreamingThinkingLayoutMode identifies the globally remembered floating thinking layout.
// StreamingThinkingLayoutMode 标识全局记忆的浮动思考布局。
export type StreamingThinkingLayoutMode = "centered" | "right_docked";

// STREAMING_THINKING_LAYOUT_STORAGE_KEY is the stable browser preference key shared by all conversations.
// STREAMING_THINKING_LAYOUT_STORAGE_KEY 是所有会话共享的稳定浏览器偏好键。
export const STREAMING_THINKING_LAYOUT_STORAGE_KEY = "vulcan-streaming-thinking-layout";

// DEFAULT_STREAMING_THINKING_LAYOUT_MODE preserves the original centered expanding popup.
// DEFAULT_STREAMING_THINKING_LAYOUT_MODE 保留原有的居中增长弹窗。
export const DEFAULT_STREAMING_THINKING_LAYOUT_MODE: StreamingThinkingLayoutMode = "centered";

// loadStreamingThinkingLayoutMode reads and validates the global browser preference.
// loadStreamingThinkingLayoutMode 读取并校验全局浏览器偏好。
// storage optionally supplies a testable storage reader; the browser localStorage is used by default.
// storage 可选提供便于测试的存储读取器；默认使用浏览器 localStorage。
// Returns a supported layout mode, falling back to the centered mode for missing or invalid data.
// 返回受支持的布局模式；数据缺失或无效时回退到居中模式。
export function loadStreamingThinkingLayoutMode(
  storage?: Pick<Storage, "getItem">,
): StreamingThinkingLayoutMode {
  if (!storage && typeof window === "undefined") {
    return DEFAULT_STREAMING_THINKING_LAYOUT_MODE;
  }
  try {
    const value = (storage ?? window.localStorage).getItem(STREAMING_THINKING_LAYOUT_STORAGE_KEY);
    return value === "right_docked" || value === "centered"
      ? value
      : DEFAULT_STREAMING_THINKING_LAYOUT_MODE;
  } catch {
    return DEFAULT_STREAMING_THINKING_LAYOUT_MODE;
  }
}

// persistStreamingThinkingLayoutMode stores one global layout preference without blocking rendering on storage failures.
// persistStreamingThinkingLayoutMode 保存单个全局布局偏好；存储失败时不阻塞渲染。
// mode is the validated layout mode selected by the user.
// mode 是用户选择的已校验布局模式。
// storage optionally supplies a testable storage writer; the browser localStorage is used by default.
// storage 可选提供便于测试的存储写入器；默认使用浏览器 localStorage。
// Returns nothing.
// 无返回值。
export function persistStreamingThinkingLayoutMode(
  mode: StreamingThinkingLayoutMode,
  storage?: Pick<Storage, "setItem">,
): void {
  if (!storage && typeof window === "undefined") {
    return;
  }
  try {
    (storage ?? window.localStorage).setItem(STREAMING_THINKING_LAYOUT_STORAGE_KEY, mode);
  } catch {
    // Ignore quota or privacy-mode failures because the current in-memory selection remains usable.
    // 忽略容量或隐私模式失败，因为当前内存中的选择仍可继续使用。
  }
}

// nextStreamingThinkingLayoutMode returns the opposite layout for the header toggle.
// nextStreamingThinkingLayoutMode 返回标题栏切换按钮对应的另一种布局。
// mode is the current persisted layout mode.
// mode 是当前持久化布局模式。
// Returns the alternate supported layout mode.
// 返回另一个受支持的布局模式。
export function nextStreamingThinkingLayoutMode(
  mode: StreamingThinkingLayoutMode,
): StreamingThinkingLayoutMode {
  return mode === "centered" ? "right_docked" : "centered";
}
