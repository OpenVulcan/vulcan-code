// ComposerSettingsSection names one section of the session settings panel a slash command can open at.
// ComposerSettingsSection 命名 slash 指令可直接定位的会话设置面板区块。
export type ComposerSettingsSection =
  | "style"
  | "answer"
  | "knowledge"
  | "tools"
  | "thinking"
  | "memory";

// ComposerSettingsRequest asks the session settings panel to open, optionally at one section.
// ComposerSettingsRequest 请求打开会话设置面板，可选地定位到某个区块。
export interface ComposerSettingsRequest {
  // Section to bring into view, or null for the panel itself.
  // 要定位的区块；为 null 时只打开面板本身。
  section: ComposerSettingsSection | null;
  // Distinguishes repeated requests for the same section.
  // 区分对同一区块的重复请求。
  nonce: number;
}
