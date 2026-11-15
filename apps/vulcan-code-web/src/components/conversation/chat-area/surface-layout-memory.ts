import React from "react";
import type { ConversationSurfaceExpandedState } from "@/components/conversation/conversation-layout-restore";

// UseConversationSurfaceLayoutMemoryOptions defines the surfaces whose runtime restore data remains valid.
// UseConversationSurfaceLayoutMemoryOptions 定义其运行期恢复数据仍有效的界面集合。
export interface UseConversationSurfaceLayoutMemoryOptions {
  // Stable ids for every live conversation and subagent surface.
  // 全部存活会话与子代理界面的稳定标识。
  liveSurfaceIds: ReadonlySet<string>;
}

// ConversationSurfaceLayoutMemory exposes typed reads and writes for per-surface expansion state.
// ConversationSurfaceLayoutMemory 暴露单界面展开状态的类型化读写操作。
export interface ConversationSurfaceLayoutMemory {
  // Reads one existing restore record without creating it.
  // 读取一条现有恢复记录但不创建。
  read: (surfaceId: string | null) => ConversationSurfaceExpandedState | null;
  // Stores one transcript expansion state.
  // 保存一条转录展开状态。
  writeExpandedState: (surfaceId: string, state: ConversationSurfaceExpandedState) => void;
}

// useConversationSurfaceLayoutMemory owns the runtime restore registry for conversation and subagent surfaces.
// useConversationSurfaceLayoutMemory 管理会话与子代理界面的运行期恢复注册表。
// Parameters: options provides the authoritative set of live surface ids used for pruning.
// 参数：options 提供用于清理的权威存活界面标识集合。
// Returns: stable typed operations for expansion-state memory.
// 返回值：展开状态记忆的稳定类型化操作。
export function useConversationSurfaceLayoutMemory(
  options: UseConversationSurfaceLayoutMemoryOptions,
): ConversationSurfaceLayoutMemory {
  // statesRef stores one expansion state per live surface without triggering renders.
  // statesRef 为每个存活界面保存一条不触发渲染的展开状态。
  const statesRef = React.useRef<Map<string, ConversationSurfaceExpandedState>>(new Map());

  // Pruning releases restore records immediately after their authoritative surface ids disappear.
  // 清理流程在权威界面标识消失后立即释放对应恢复记录。
  React.useEffect(() => {
    // surfaceId identifies one restore record currently retained by the registry.
    // surfaceId 标识恢复注册表当前保留的一条记录。
    for (const surfaceId of statesRef.current.keys()) {
      if (!options.liveSurfaceIds.has(surfaceId)) {
        statesRef.current.delete(surfaceId);
      }
    }
  }, [options.liveSurfaceIds]);

  // read returns one existing record without allocating state for unseen surfaces.
  // read 读取一条现有记录，但不为未见界面分配状态。
  // Parameters: surfaceId is the exact conversation or subagent surface id, or null when no surface is active.
  // 参数：surfaceId 是精确的会话或子代理界面标识；无活动界面时为 null。
  // Returns: the existing complete restore record, or null when the registry has no record.
  // 返回值：现有的完整恢复记录；注册表中无记录时返回 null。
  const read = React.useCallback((surfaceId: string | null): ConversationSurfaceExpandedState | null => {
    if (!surfaceId) {
      return null;
    }
    return statesRef.current.get(surfaceId) ?? null;
  }, []);
  // writeExpandedState stores transcript expansion choices only when their reference changed.
  // writeExpandedState 仅在引用变化时保存转录展开选择。
  // Parameters: surfaceId selects the record; state contains every persisted runtime expansion dimension.
  // 参数：surfaceId 选择记录；state 包含全部持有的运行期展开维度。
  // Returns: nothing; equal state references deliberately leave the registry unchanged.
  // 返回值：无；相同状态引用会故意保持注册表不变。
  const writeExpandedState = React.useCallback((surfaceId: string, state: ConversationSurfaceExpandedState): void => {
    if (statesRef.current.get(surfaceId) !== state) {
      statesRef.current.set(surfaceId, state);
    }
  }, []);
  return {
    read,
    writeExpandedState,
  };
}
