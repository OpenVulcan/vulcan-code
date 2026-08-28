import type { TranslationKey } from "@/i18n";

// ThinkingDisplayMode stores the persisted display preference for reasoning output.
// ThinkingDisplayMode 保存持久化的思考过程展示偏好。
export type ThinkingDisplayMode =
  | "popup_only"
  | "inline_output"
  | "first_inline_with_popup"
  | "first_inline_only";

// DEFAULT_THINKING_DISPLAY_MODE is used when the backend has no persisted display preference yet.
// DEFAULT_THINKING_DISPLAY_MODE 用于后端尚未持久化展示偏好时的默认值。
export const DEFAULT_THINKING_DISPLAY_MODE: ThinkingDisplayMode = "first_inline_with_popup";

// ThinkingDisplayModeOption describes one selectable display mode in the composer menu.
// ThinkingDisplayModeOption 描述输入区菜单中的单个可选展示模式。
export interface ThinkingDisplayModeOption {
  // Stable value stored in the shared Vulcan configuration and passed through component props.
  // 存入共享 Vulcan 配置并通过组件属性传递的稳定值。
  value: ThinkingDisplayMode;
  // Translation key used for the primary menu text.
  // 用于菜单主文案的翻译键。
  labelKey: TranslationKey;
}

// THINKING_DISPLAY_MODE_OPTIONS lists every user-facing thinking display mode.
// THINKING_DISPLAY_MODE_OPTIONS 列出所有面向用户的思考展示模式。
export const THINKING_DISPLAY_MODE_OPTIONS: ThinkingDisplayModeOption[] = [
  { value: "popup_only", labelKey: "thinkingDisplay.popupOnly" },
  { value: "inline_output", labelKey: "thinkingDisplay.inlineOutput" },
  { value: "first_inline_with_popup", labelKey: "thinkingDisplay.firstInlineWithPopup" },
  { value: "first_inline_only", labelKey: "thinkingDisplay.firstInlineOnly" },
];

// parseThinkingDisplayMode returns a supported display mode or null for unknown values.
// parseThinkingDisplayMode 返回受支持的展示模式，未知值返回 null。
export function parseThinkingDisplayMode(value: string | null | undefined): ThinkingDisplayMode | null {
  return THINKING_DISPLAY_MODE_OPTIONS.some((option) => option.value === value)
    ? value as ThinkingDisplayMode
    : null;
}

// normalizeThinkingDisplayMode resolves an optional backend value to the effective display mode.
// normalizeThinkingDisplayMode 将可选后端值解析为实际展示模式。
export function normalizeThinkingDisplayMode(value: string | null | undefined): ThinkingDisplayMode {
  return parseThinkingDisplayMode(value) ?? DEFAULT_THINKING_DISPLAY_MODE;
}
