import React from "react";

// PendingPromptQueueActions are the owner's queue mutations that an exact queue edit must exclude.
// PendingPromptQueueActions 是精确队列编辑期间必须排除的所有者队列变更。
interface PendingPromptQueueActions {
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
}

// usePendingPromptQueueGuards wraps queue insertion, removal and reordering so none runs while an exact queue edit
// owns the queue; `editActive` is the synchronous edit gate.
// usePendingPromptQueueGuards 包装队列插入、移除与重排，使精确队列编辑持有队列期间均不执行；`editActive` 是同步编辑门控。
export function usePendingPromptQueueGuards(editActive: React.RefObject<boolean>, actions: PendingPromptQueueActions) {
  const { onInsertPendingPrompt, onRemovePendingPrompt, onReorderPendingPrompts } = actions;
  const onInsert = React.useCallback((entryId: string) => {
    if (!editActive.current) {
      void onInsertPendingPrompt(entryId);
    }
  }, [editActive, onInsertPendingPrompt]);
  const onRemove = React.useCallback((entryId: string) => {
    if (!editActive.current) {
      void onRemovePendingPrompt(entryId);
    }
  }, [editActive, onRemovePendingPrompt]);
  const onReorder = React.useCallback((entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => {
    if (!editActive.current) {
      void onReorderPendingPrompts(entryIds, draggedEntryId, targetEntryId);
    }
  }, [editActive, onReorderPendingPrompts]);
  return { onInsert, onRemove, onReorder };
}
