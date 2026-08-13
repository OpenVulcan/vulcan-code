import type { ConversationScrollSnapshot } from "@/components/conversation/chat-area-scroll";

// ConversationSurfaceExpandedState stores UI expansion choices for one transcript surface.
// ConversationSurfaceExpandedState 保存单个转录界面的 UI 展开选择。
export interface ConversationSurfaceExpandedState {
  // Stable measured-collapse expansion choices keyed by stable collapse ids.
  // 按稳定折叠标识保存的实测折叠展开选择。
  measuredCollapseById: Record<string, ConversationMeasuredCollapseMemory>;
  // User-controlled process collapse states keyed by surface and process id.
  // 按界面和处理过程标识保存的用户控制折叠状态。
  processCollapsedByKey: Record<string, boolean>;
  // Tool presentation ids that are currently expanded.
  // 当前处于展开状态的工具展示标识。
  expandedToolPresentationIds: string[];
  // User prompt row ids that are currently expanded.
  // 当前处于展开状态的用户提示词行标识。
  expandedUserPromptIds: string[];
}

// ConversationMeasuredCollapseMemory stores only the remount-safe expansion choice for one collapsible node.
// ConversationMeasuredCollapseMemory 仅保存单个可折叠节点可安全重新挂载的展开选择。
export interface ConversationMeasuredCollapseMemory {
  // Last known expanded state for this collapse node.
  // 当前折叠节点最近一次记录的展开状态。
  expanded: boolean;
}

// ConversationSurfaceLayoutRestoreState stores all runtime layout restore data for one surface.
// ConversationSurfaceLayoutRestoreState 保存单个界面的全部运行期布局恢复数据。
export interface ConversationSurfaceLayoutRestoreState {
  // Latest scroll restoration point for this surface.
  // 当前界面的最近滚动恢复点。
  scroll: ConversationScrollSnapshot;
  // Runtime-only expansion choices restored before stable Turn rendering.
  // 稳定 Turn 渲染前恢复的运行期展开选择。
  expandedState: ConversationSurfaceExpandedState;
}

// createEmptyConversationSurfaceExpandedState returns a neutral expansion state.
// createEmptyConversationSurfaceExpandedState 返回一个空的展开状态。
export function createEmptyConversationSurfaceExpandedState(): ConversationSurfaceExpandedState {
  return {
    expandedToolPresentationIds: [],
    expandedUserPromptIds: [],
    measuredCollapseById: {},
    processCollapsedByKey: {},
  };
}

// createConversationSurfaceLayoutRestoreState returns a default per-surface restore record.
// createConversationSurfaceLayoutRestoreState 返回默认的单界面恢复记录。
export function createConversationSurfaceLayoutRestoreState(): ConversationSurfaceLayoutRestoreState {
  return {
    expandedState: createEmptyConversationSurfaceExpandedState(),
    scroll: {
      anchorKey: null,
      anchorTop: 0,
      followTail: true,
      scrollTop: 0,
    },
  };
}
