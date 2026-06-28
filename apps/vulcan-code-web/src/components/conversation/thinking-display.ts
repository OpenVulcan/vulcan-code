import type {
  ConversationBlock,
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";
import { isContextCompactionBlock } from "@/components/conversation/conversation-classifiers";
import { metadataValue } from "@/components/conversation/conversation-metadata";
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

// activeThinkingOverlayBlock returns the live thinking block that should use the floating surface.
// activeThinkingOverlayBlock 返回应使用浮动面板展示的实时思考块。
export function activeThinkingOverlayBlock(
  mode: ThinkingDisplayMode,
  activeBlock: ConversationBlock | null,
): ConversationBlock | null {
  if (mode === "popup_only" || mode === "first_inline_with_popup") {
    return activeBlock;
  }
  return null;
}

// hiddenThinkingBlockIdsForDisplayMode resolves thinking blocks filtered from the main transcript.
// hiddenThinkingBlockIdsForDisplayMode 解析需要从主会话面板过滤掉的思考块。
export function hiddenThinkingBlockIdsForDisplayMode(
  mode: ThinkingDisplayMode,
  renderItems: ConversationRenderItem[],
  visibleOverlayBlockId: string | null,
): Set<string> {
  const thinkingBlocks = collectThinkingBlockVisibility(renderItems);
  if (mode === "inline_output") {
    return new Set();
  }
  if (mode === "popup_only") {
    return visibleOverlayBlockId ? new Set([visibleOverlayBlockId]) : new Set();
  }
  const hidden = new Set(thinkingBlocks.all.filter((id) => !thinkingBlocks.firstByProcess.has(id)));
  if (mode === "first_inline_with_popup" && visibleOverlayBlockId && thinkingBlocks.firstByProcess.has(visibleOverlayBlockId)) {
    hidden.add(visibleOverlayBlockId);
  }
  return hidden;
}

// collectThinkingBlockVisibility collects all thinking ids and the first thinking id for each process.
// collectThinkingBlockVisibility 收集所有思考块标识，以及每个处理过程的首个思考块标识。
function collectThinkingBlockVisibility(renderItems: ConversationRenderItem[]): {
  // All thinking block ids in transcript order.
  // 按转录顺序排列的全部思考块标识。
  all: string[];
  // First thinking block ids selected per processing turn.
  // 每个处理回合选中的首个思考块标识。
  firstByProcess: Set<string>;
} {
  const all: string[] = [];
  const firstByProcess = new Set<string>();
  let hasOpenTurn = false;
  let currentTurnHasStats = false;
  let currentProcessFirstThinkingId: string | null = null;

  for (const item of renderItems) {
    if (item.role === "user") {
      if (isThinkingDisplayTurnProcessUserItem(item)) {
        continue;
      }
      if (!hasOpenTurn || currentTurnHasStats || isThinkingDisplayTurnStartUserItem(item)) {
        hasOpenTurn = true;
        currentTurnHasStats = false;
        currentProcessFirstThinkingId = null;
      }
      continue;
    }
    const contextCompactionItem = item.blocks.some(isContextCompactionBlock);
    if (!hasOpenTurn || currentTurnHasStats) {
      hasOpenTurn = true;
      currentTurnHasStats = false;
      currentProcessFirstThinkingId = null;
    }
    for (const block of item.blocks) {
      if (block.kind !== "thinking") {
        continue;
      }
      all.push(block.id);
      if (!currentProcessFirstThinkingId) {
        currentProcessFirstThinkingId = block.id;
        firstByProcess.add(block.id);
      }
    }
    if (item.blocks.some((block) => block.kind === "turn-stats")) {
      currentTurnHasStats = true;
      currentProcessFirstThinkingId = null;
      continue;
    }
    if (contextCompactionItem) {
      currentTurnHasStats = true;
      currentProcessFirstThinkingId = null;
    }
  }

  return { all, firstByProcess };
}

// isThinkingDisplayTurnProcessUserItem returns whether one user row belongs inside an existing process.
// isThinkingDisplayTurnProcessUserItem 判断单条用户记录是否属于已有过程内部。
function isThinkingDisplayTurnProcessUserItem(item: ConversationRenderItem): boolean {
  if (item.role !== "user") {
    return false;
  }
  return metadataValue(item.userMessage?.metadata ?? item.sourceMessage?.metadata, ["turn_process"]) === true;
}

// isThinkingDisplayTurnStartUserItem returns whether one user row explicitly starts a new process.
// isThinkingDisplayTurnStartUserItem 判断单条用户记录是否显式开启新的过程。
function isThinkingDisplayTurnStartUserItem(item: ConversationRenderItem): boolean {
  if (item.role !== "user") {
    return false;
  }
  const metadata = item.userMessage?.metadata ?? item.sourceMessage?.metadata;
  return metadataValue(metadata, ["starts_user_turn"]) === true
    && metadataValue(metadata, ["turn_process"]) !== true;
}
