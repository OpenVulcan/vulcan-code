import React from "react";
import type { ConversationScrollSnapshot } from "@/components/conversation/chat-area-scroll";
import {
  createConversationSurfaceLayoutRestoreState,
  type ConversationSurfaceExpandedState,
  type ConversationSurfaceLayoutRestoreState,
} from "@/components/conversation/conversation-layout-restore";

// UseConversationSurfaceLayoutMemoryOptions defines the surfaces whose runtime restore data remains valid.
// UseConversationSurfaceLayoutMemoryOptions 定义其运行期恢复数据仍有效的界面集合。
export interface UseConversationSurfaceLayoutMemoryOptions {
  // Stable ids for every live conversation and subagent surface.
  // 全部存活会话与子代理界面的稳定标识。
  liveSurfaceIds: ReadonlySet<string>;
}

// ConversationSurfaceLayoutMemory exposes typed reads and writes for all per-surface restore dimensions.
// ConversationSurfaceLayoutMemory 暴露全部单界面恢复维度的类型化读写操作。
export interface ConversationSurfaceLayoutMemory {
  // Reads one existing restore record without creating it.
  // 读取一条现有恢复记录但不创建。
  read: (surfaceId: string | null) => ConversationSurfaceLayoutRestoreState | null;
  // Stores one scroll restoration snapshot.
  // 保存一条滚动恢复快照。
  writeScroll: (surfaceId: string, snapshot: ConversationScrollSnapshot) => void;
  // Stores one transcript expansion state.
  // 保存一条转录展开状态。
  writeExpandedState: (surfaceId: string, state: ConversationSurfaceExpandedState) => void;
  // Stores one measured turn-shell height.
  // 保存一条轮次外壳实测高度。
  writeTurnHeight: (surfaceId: string, turnKey: string, height: number) => void;
  // Removes measured heights for turns no longer present in one surface.
  // 删除单个界面中已不存在轮次的实测高度。
  retainTurnHeights: (surfaceId: string, liveTurnKeys: ReadonlySet<string>) => void;
}

// useConversationSurfaceLayoutMemory owns the runtime restore registry for conversation and subagent surfaces.
// useConversationSurfaceLayoutMemory 管理会话与子代理界面的运行期恢复注册表。
// Parameters: options provides the authoritative set of live surface ids used for pruning.
// 参数：options 提供用于清理的权威存活界面标识集合。
// Returns: stable typed operations for scroll, expansion state, and turn-shell height memory.
// 返回值：滚动、展开状态与轮次外壳高度记忆的稳定类型化操作。
export function useConversationSurfaceLayoutMemory(
  options: UseConversationSurfaceLayoutMemoryOptions,
): ConversationSurfaceLayoutMemory {
  // statesRef stores one mutable restore record per live surface without triggering renders.
  // statesRef 为每个存活界面保存一条不触发渲染的可变恢复记录。
  const statesRef = React.useRef<Map<string, ConversationSurfaceLayoutRestoreState>>(new Map());

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
  const read = React.useCallback((surfaceId: string | null): ConversationSurfaceLayoutRestoreState | null => {
    if (!surfaceId) {
      return null;
    }
    return statesRef.current.get(surfaceId) ?? null;
  }, []);
  // update applies one immutable record change and skips identical references.
  // update 应用一次不可变记录修改，并跳过相同引用。
  // Parameters: surfaceId selects the record; updater receives the complete current record and returns its replacement.
  // 参数：surfaceId 选择记录；updater 接收完整当前记录并返回替换值。
  // Returns: nothing; the mutable registry owns the resulting record.
  // 返回值：无；可变注册表持有结果记录。
  const update = React.useCallback((
    surfaceId: string,
    updater: (current: ConversationSurfaceLayoutRestoreState) => ConversationSurfaceLayoutRestoreState,
  ): void => {
    if (!surfaceId) {
      return;
    }
    // current is the stored record or a neutral record for the first write.
    // current 是已保存记录，首次写入时为中性记录。
    const current = statesRef.current.get(surfaceId) ?? createConversationSurfaceLayoutRestoreState();
    // next is the complete replacement returned by the typed dimension writer.
    // next 是类型化维度写入器返回的完整替换记录。
    const next = updater(current);
    if (next === current) {
      return;
    }
    statesRef.current.set(surfaceId, next);
  }, []);
  // writeScroll stores the latest viewport restoration point.
  // writeScroll 保存最近的视口恢复点。
  // Parameters: surfaceId selects the record; snapshot contains the exact viewport anchor or bottom distance.
  // 参数：surfaceId 选择记录；snapshot 包含精确视口锚点或底部距离。
  // Returns: nothing; the snapshot is retained in runtime memory.
  // 返回值：无；快照保留在运行期内存中。
  const writeScroll = React.useCallback((surfaceId: string, snapshot: ConversationScrollSnapshot): void => {
    update(surfaceId, (current) => ({
      ...current,
      scroll: snapshot,
    }));
  }, [update]);
  // writeExpandedState stores transcript expansion choices only when their reference changed.
  // writeExpandedState 仅在引用变化时保存转录展开选择。
  // Parameters: surfaceId selects the record; state contains every persisted runtime expansion dimension.
  // 参数：surfaceId 选择记录；state 包含全部持有的运行期展开维度。
  // Returns: nothing; equal state references deliberately leave the registry unchanged.
  // 返回值：无；相同状态引用会故意保持注册表不变。
  const writeExpandedState = React.useCallback((surfaceId: string, state: ConversationSurfaceExpandedState): void => {
    update(surfaceId, (current) => (
      current.expandedState === state
        ? current
        : { ...current, expandedState: state }
    ));
  }, [update]);
  // writeTurnHeight stores one finite positive shell height when its value changed.
  // writeTurnHeight 仅在有限正高度变化时保存轮次外壳高度。
  // Parameters: surfaceId and turnKey select the exact record; height is the measured pixel height.
  // 参数：surfaceId 与 turnKey 选择精确记录；height 是实测像素高度。
  // Returns: nothing; invalid or unchanged measurements leave the registry unchanged.
  // 返回值：无；无效或未变测量值保持注册表不变。
  const writeTurnHeight = React.useCallback((surfaceId: string, turnKey: string, height: number): void => {
    if (!turnKey || !Number.isFinite(height) || height <= 0) {
      return;
    }
    update(surfaceId, (current) => (
      current.turnHeightsByKey[turnKey] === height
        ? current
        : {
            ...current,
            turnHeightsByKey: {
              ...current.turnHeightsByKey,
              [turnKey]: height,
            },
          }
    ));
  }, [update]);
  // retainTurnHeights removes stale turn keys without allocating records for unseen surfaces.
  // retainTurnHeights 删除过期轮次键，且不为未见界面分配记录。
  // Parameters: surfaceId selects the record; liveTurnKeys is the authoritative rendered-turn key set.
  // 参数：surfaceId 选择记录；liveTurnKeys 是权威已渲染轮次键集合。
  // Returns: nothing; the record changes only when at least one stored turn disappeared.
  // 返回值：无；仅当至少一个已保存轮次消失时修改记录。
  const retainTurnHeights = React.useCallback((surfaceId: string, liveTurnKeys: ReadonlySet<string>): void => {
    update(surfaceId, (current) => {
      // retainedEntries contains only measurements whose turn keys still exist.
      // retainedEntries 仅包含轮次键仍存在的测量值。
      const retainedEntries = Object.entries(current.turnHeightsByKey)
        .filter(([turnKey]) => liveTurnKeys.has(turnKey));
      if (retainedEntries.length === Object.keys(current.turnHeightsByKey).length) {
        return current;
      }
      return {
        ...current,
        turnHeightsByKey: Object.fromEntries(retainedEntries),
      };
    });
  }, [update]);

  return {
    read,
    retainTurnHeights,
    writeScroll,
    writeExpandedState,
    writeTurnHeight,
  };
}
