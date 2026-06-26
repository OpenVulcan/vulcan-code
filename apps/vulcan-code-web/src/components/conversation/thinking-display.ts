import type {
  ConversationBlock,
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";
import { isContextCompactionBlock } from "@/components/conversation/conversation-classifiers";
import { metadataValue } from "@/components/conversation/conversation-metadata";

// ThinkingDisplayMode stores the WebUI-only display preference for reasoning output.
// ThinkingDisplayMode 保存仅 WebUI 使用的思考过程展示偏好。
export type ThinkingDisplayMode =
  | "popup_only"
  | "inline_output"
  | "first_inline_with_popup"
  | "first_inline_only";

// ThinkingDisplayModeOption describes one selectable display mode in the composer menu.
// ThinkingDisplayModeOption 描述输入区菜单中的单个可选展示模式。
export interface ThinkingDisplayModeOption {
  // Stable value stored in localStorage and passed through component props.
  // 存入 localStorage 并通过组件属性传递的稳定值。
  value: ThinkingDisplayMode;
  // Chinese label shown as the primary menu text.
  // 菜单中作为主文案展示的中文名称。
  labelZh: string;
  // English label shown as the secondary menu text.
  // 菜单中作为辅助文案展示的英文名称。
  labelEn: string;
}

// THINKING_DISPLAY_STORAGE_KEY persists the current four-state display mode.
// THINKING_DISPLAY_STORAGE_KEY 持久化当前四态思考展示模式。
export const THINKING_DISPLAY_STORAGE_KEY = "vulcan-webui-thinking-display-mode";

// LEGACY_THINKING_OVERLAY_STORAGE_KEY is read once to migrate the old boolean preference.
// LEGACY_THINKING_OVERLAY_STORAGE_KEY 仅用于只读迁移旧版布尔偏好。
export const LEGACY_THINKING_OVERLAY_STORAGE_KEY = "vulcan-webui-thinking-overlay-enabled";

// THINKING_DISPLAY_MODE_OPTIONS lists every user-facing thinking display mode.
// THINKING_DISPLAY_MODE_OPTIONS 列出所有面向用户的思考展示模式。
export const THINKING_DISPLAY_MODE_OPTIONS: ThinkingDisplayModeOption[] = [
  { value: "popup_only", labelZh: "弹窗显示", labelEn: "Popup Then Inline" },
  { value: "inline_output", labelZh: "直接输出", labelEn: "Inline Output" },
  { value: "first_inline_with_popup", labelZh: "首段输出+弹窗", labelEn: "First Inline + Popup" },
  { value: "first_inline_only", labelZh: "仅首段输出", labelEn: "First Inline Only" },
];

// parseThinkingDisplayMode returns a supported display mode or null for unknown values.
// parseThinkingDisplayMode 返回受支持的展示模式，未知值返回 null。
export function parseThinkingDisplayMode(value: string | null | undefined): ThinkingDisplayMode | null {
  return THINKING_DISPLAY_MODE_OPTIONS.some((option) => option.value === value)
    ? value as ThinkingDisplayMode
    : null;
}

// initialThinkingDisplayMode resolves the persisted mode and migrates the legacy boolean setting.
// initialThinkingDisplayMode 解析持久化模式，并迁移旧版布尔设置。
export function initialThinkingDisplayMode(storage: Storage = window.localStorage): ThinkingDisplayMode {
  const storedMode = parseThinkingDisplayMode(storage.getItem(THINKING_DISPLAY_STORAGE_KEY));
  if (storedMode) {
    return storedMode;
  }
  const legacyOverlayEnabled = storage.getItem(LEGACY_THINKING_OVERLAY_STORAGE_KEY);
  if (legacyOverlayEnabled === "false") {
    return "inline_output";
  }
  return "popup_only";
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
