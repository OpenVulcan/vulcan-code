// STREAMING_THINKING_MARKDOWN_STORAGE_KEY owns the rendering preference shared by this client's conversations.
// STREAMING_THINKING_MARKDOWN_STORAGE_KEY 持有当前客户端各会话共享的渲染偏好。
export const STREAMING_THINKING_MARKDOWN_STORAGE_KEY = "vulcan-streaming-thinking-markdown";

// DEFAULT_STREAMING_THINKING_MARKDOWN_ENABLED avoids Markdown parsing until the user explicitly enables it.
// DEFAULT_STREAMING_THINKING_MARKDOWN_ENABLED 在用户明确开启前避免执行 Markdown 解析。
const DEFAULT_STREAMING_THINKING_MARKDOWN_ENABLED = false;

// loadStreamingThinkingMarkdownEnabled reads the saved client preference without writing a default.
// loadStreamingThinkingMarkdownEnabled 读取已保存的客户端偏好，不回写默认值。
// Takes no arguments; returns the saved selection or the disabled default when storage is unavailable or invalid.
// 无参数；返回已保存的选择，存储不可用或值无效时返回默认关闭状态。
export function loadStreamingThinkingMarkdownEnabled(): boolean {
  if (typeof window === "undefined") return DEFAULT_STREAMING_THINKING_MARKDOWN_ENABLED;
  try {
    return window.localStorage.getItem(STREAMING_THINKING_MARKDOWN_STORAGE_KEY) === "true"
      ? true
      : DEFAULT_STREAMING_THINKING_MARKDOWN_ENABLED;
  } catch {
    return DEFAULT_STREAMING_THINKING_MARKDOWN_ENABLED;
  }
}

// persistStreamingThinkingMarkdownEnabled saves an explicit user choice for future popup mounts and restarts.
// persistStreamingThinkingMarkdownEnabled 保存用户明确选择，供后续弹窗挂载及重启使用。
// Parameter: enabled selects rendered Markdown or plain text. Returns nothing; storage failures are logged.
// 参数：enabled 选择 Markdown 渲染或纯文本。无返回值；存储失败时记录错误。
export function persistStreamingThinkingMarkdownEnabled(enabled: boolean): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STREAMING_THINKING_MARKDOWN_STORAGE_KEY, String(enabled));
  } catch (error) {
    // Restricted or full browser storage must not prevent switching the current popup's presentation.
    // 浏览器存储受限或已满不得阻止切换当前弹窗的显示方式。
    console.warn("Could not save the streaming thinking Markdown preference.", error);
  }
}
