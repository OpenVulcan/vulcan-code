import type { ConversationScrollSnapshot } from "@/components/conversation/chat-area-scroll";
import type { ConversationTimelineVirtualRestoreState } from "@/components/conversation/conversation-virtualization";

// ConversationSurfaceExpandedState stores UI expansion choices for one transcript surface.
// ConversationSurfaceExpandedState 保存单个转录界面的 UI 展开选择。
export interface ConversationSurfaceExpandedState {
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

// ConversationSurfaceLayoutRestoreState stores all runtime layout restore data for one surface.
// ConversationSurfaceLayoutRestoreState 保存单个界面的全部运行期布局恢复数据。
export interface ConversationSurfaceLayoutRestoreState {
  // Latest scroll restoration point for this surface.
  // 当前界面的最近滚动恢复点。
  scroll: ConversationScrollSnapshot;
  // Latest virtual list restore state, when the transcript is long enough to virtualize.
  // 转录足够长并启用虚拟列表时的最近虚拟列表恢复状态。
  virtualizer: ConversationTimelineVirtualRestoreState | null;
  // Runtime-only expansion choices restored before row height estimation.
  // 行高估算前恢复的运行期展开选择。
  expandedState: ConversationSurfaceExpandedState;
  // Monotonic version for diagnostics and future persistence boundaries.
  // 用于诊断和后续持久化边界的单调版本号。
  layoutVersion: number;
}

// createEmptyConversationSurfaceExpandedState returns a neutral expansion state.
// createEmptyConversationSurfaceExpandedState 返回一个空的展开状态。
export function createEmptyConversationSurfaceExpandedState(): ConversationSurfaceExpandedState {
  return {
    expandedToolPresentationIds: [],
    expandedUserPromptIds: [],
    processCollapsedByKey: {},
  };
}

// createConversationSurfaceLayoutRestoreState returns a default per-surface restore record.
// createConversationSurfaceLayoutRestoreState 返回默认的单界面恢复记录。
export function createConversationSurfaceLayoutRestoreState(): ConversationSurfaceLayoutRestoreState {
  return {
    expandedState: createEmptyConversationSurfaceExpandedState(),
    layoutVersion: 0,
    scroll: {
      distanceFromBottom: 0,
      wasAtBottom: true,
    },
    virtualizer: null,
  };
}
