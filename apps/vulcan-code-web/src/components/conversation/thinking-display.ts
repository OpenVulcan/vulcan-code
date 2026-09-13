import type {
  ConversationBlock,
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";
import { isMergeableToolBlock } from "@/components/conversation/conversation-classifiers";
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
export const DEFAULT_THINKING_DISPLAY_MODE: ThinkingDisplayMode = "popup_only";

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
];

// parseThinkingDisplayMode returns a supported display mode or null for unknown values.
// parseThinkingDisplayMode 返回受支持的展示模式，未知值返回 null。
export function parseThinkingDisplayMode(value: string | null | undefined): ThinkingDisplayMode | null {
  // Legacy preferences retain their popup intent without retaining first-segment filtering.
  // 旧偏好保留弹窗意图，不再保留只展示首段的过滤行为。
  if (value === "first_inline_with_popup") return "popup_only";
  if (value === "first_inline_only") return "inline_output";
  return THINKING_DISPLAY_MODE_OPTIONS.some((option) => option.value === value)
    ? value as ThinkingDisplayMode
    : null;
}

// normalizeThinkingDisplayMode resolves an optional backend value to the effective display mode.
// normalizeThinkingDisplayMode 将可选后端值解析为实际展示模式。
export function normalizeThinkingDisplayMode(value: string | null | undefined): ThinkingDisplayMode {
  return parseThinkingDisplayMode(value) ?? DEFAULT_THINKING_DISPLAY_MODE;
}

// activeThinkingOverlayBlock returns the live thinking block that should use the floating surface.
// activeThinkingOverlayBlock 返回应使用浮动面板展示的实时思考块。
export function activeThinkingOverlayBlock(
  mode: ThinkingDisplayMode,
  activeBlock: ConversationBlock | null,
): ConversationBlock | null {
  if (normalizeThinkingDisplayMode(mode) === "popup_only") {
    return activeBlock;
  }
  return null;
}

// hiddenThinkingBlockIdsForDisplayMode preserves all reasoning under both popup preferences.
// hiddenThinkingBlockIdsForDisplayMode 在两种弹窗偏好下都保留全部思考。
// The arguments retain the shared renderer contract; returns an empty display filter.
// 参数保留共享渲染器契约；返回空的显示过滤集合。
export function hiddenThinkingBlockIdsForDisplayMode(
  _mode: ThinkingDisplayMode,
  _renderItems: ConversationRenderItem[],
): Set<string> {
  return new Set();
}

// integrateProcessThinking associates reasoning with the next tool or intermediate assistant row after final-answer extraction.
// integrateProcessThinking 在最终回答提取后，将思考关联到紧随的工具或中间助手行。
// items contains process rows only; isAssistantContent reuses the timeline's content classifier. Returns immutable display copies.
// items 仅包含过程行；isAssistantContent 复用时间线正文分类器。返回不可变展示副本。
export function integrateProcessThinking(
  items: readonly ConversationRenderItem[],
  isAssistantContent: (item: ConversationRenderItem) => boolean,
): ConversationRenderItem[] {
  // result retains original row order; pending holds reasoning until the next semantic row determines ownership.
  // result 保持原始行顺序；pending 保留思考，直到下一条语义记录确定归属。
  const result: ConversationRenderItem[] = [];
  let pending: ConversationRenderItem[] = [];
  // alignedToolRun lasts only through consecutive tools following a reasoning prefix.
  // alignedToolRun 仅持续于思考入口之后的连续工具序列。
  let alignedToolRun = false;
  for (const item of items) {
    if (item.role === "assistant" && item.blocks.length > 0 && item.blocks.every((block) => block.kind === "thinking")) {
      pending.push(item);
      alignedToolRun = false;
      continue;
    }
    // Tool classification comes from the same semantic predicate used by tool grouping.
    // 工具分类使用与工具分组相同的语义判断。
    const isTool = item.role !== "user" && item.blocks.length > 0 && item.blocks.every(isMergeableToolBlock);
    alignedToolRun = isTool && (alignedToolRun || pending.length > 0);
    if (pending.length > 0 && item.role !== "user" && (item.blocks.some(isMergeableToolBlock) || isAssistantContent(item))) {
      result.push({
        ...item,
        leadingThinking: pending.flatMap((reasoning) => reasoning.blocks),
        reserveThinkingPrefix: alignedToolRun,
        // Include reasoning sources so memoized timeline rows refresh when their prefix changes.
        // 纳入思考来源，确保前置思考变化时缓存的时间线行也会刷新。
        sourceMessages: [...pending.flatMap((reasoning) => reasoning.sourceMessages), ...item.sourceMessages],
      });
    } else {
      result.push(...pending, alignedToolRun ? { ...item, reserveThinkingPrefix: true } : item);
    }
    pending = [];
  }
  return [...result, ...pending];
}
