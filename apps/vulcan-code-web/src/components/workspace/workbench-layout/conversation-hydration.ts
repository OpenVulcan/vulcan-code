import React from "react";
import type { WorkbenchSnapshot } from "@/types";
import {
  loadConversationHistoryPage,
  loadConversationMessages,
} from "@/service/workbench-snapshot-service";
import { conversationHistoryCursorFromWindow } from "./types";

// WorkbenchStateMerger merges one incoming backend snapshot into the current UI snapshot.
// WorkbenchStateMerger 将一份后端快照合并进当前 UI 快照。
type WorkbenchStateMerger = (current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => WorkbenchSnapshot;

// ConversationHydrationOptions carries the selected conversation and snapshot merge dependencies.
// ConversationHydrationOptions 携带当前选中会话与快照合并依赖。
interface ConversationHydrationOptions {
  // selectedConversationId is the currently selected sidebar or draft conversation id.
  // selectedConversationId 是当前侧栏或草稿选中的会话标识。
  selectedConversationId: string | null;
  // draftConversationId identifies the local-only draft conversation that should not be hydrated from the backend.
  // draftConversationId 标识不应从后端水合的本地草稿会话。
  draftConversationId: string | null;
  // selectedConversationHydrated reports whether the selected backend conversation body is already retained.
  // selectedConversationHydrated 表示当前选中的后端会话正文是否已经被保留。
  selectedConversationHydrated: boolean;
  // hydratedConversationWindowById indexes the backend-retained body windows by session id.
  // hydratedConversationWindowById 按会话标识索引后端保留的正文窗口。
  hydratedConversationWindowById: Map<string, NonNullable<WorkbenchSnapshot["hydratedConversationWindows"]>[number]>;
  // mergeWorkbenchWithLocalCaches preserves local-only caches while applying a service snapshot.
  // mergeWorkbenchWithLocalCaches 在应用服务快照时保留本地缓存。
  mergeWorkbenchWithLocalCaches: WorkbenchStateMerger;
  // setWorkbenchState updates the owning WorkbenchLayout snapshot state.
  // setWorkbenchState 更新所属 WorkbenchLayout 的快照状态。
  setWorkbenchState: React.Dispatch<React.SetStateAction<WorkbenchSnapshot>>;
}

// ConversationHydrationState exposes selected-conversation loading and older-history paging state.
// ConversationHydrationState 暴露当前会话加载与更早历史分页状态。
interface ConversationHydrationState {
  // loadingConversationId stores the conversation currently loading from the service.
  // loadingConversationId 保存当前正在从服务加载正文的会话标识。
  loadingConversationId: string | null;
  // conversationLoadError stores the latest conversation hydration error.
  // conversationLoadError 保存最近一次会话正文水合错误。
  conversationLoadError: string | null;
  // conversationHistoryLoadingIds stores UI-only older-history requests currently in flight.
  // conversationHistoryLoadingIds 保存当前进行中的仅 UI 更早历史请求。
  conversationHistoryLoadingIds: Set<string>;
  // handleLoadConversationHistory requests one older turn page for the selected conversation.
  // handleLoadConversationHistory 为当前选中会话请求一页更早轮次。
  handleLoadConversationHistory: (conversationId: string) => Promise<boolean>;
}

// useConversationHydration owns backend body hydration and older-history pagination state.
// useConversationHydration 持有后端正文水合与更早历史分页状态。
export function useConversationHydration({
  selectedConversationId,
  draftConversationId,
  selectedConversationHydrated,
  hydratedConversationWindowById,
  mergeWorkbenchWithLocalCaches,
  setWorkbenchState,
}: ConversationHydrationOptions): ConversationHydrationState {
  // loadingConversationId stores the conversation currently loading from the service.
  // loadingConversationId 保存当前正在从服务加载正文的会话标识。
  const [loadingConversationId, setLoadingConversationId] = React.useState<string | null>(null);
  // hydratingConversationIdsRef stores conversation ids with an active service hydration request.
  // hydratingConversationIdsRef 保存当前已有服务端正文水合请求在途的会话标识。
  const hydratingConversationIdsRef = React.useRef<Set<string>>(new Set());
  // conversationLoadError stores the latest conversation hydration error.
  // conversationLoadError 保存最近一次会话正文水合错误。
  const [conversationLoadError, setConversationLoadError] = React.useState<string | null>(null);
  // conversationHistoryLoadingIds stores UI-only older-history requests currently in flight.
  // conversationHistoryLoadingIds 保存当前进行中的仅 UI 更早历史请求。
  const [conversationHistoryLoadingIds, setConversationHistoryLoadingIds] = React.useState<Set<string>>(() => new Set());

  React.useEffect(() => {
    if (!selectedConversationId) {
      setLoadingConversationId(null);
      setConversationLoadError(null);
      return;
    }
    if (selectedConversationId === draftConversationId) {
      setLoadingConversationId(null);
      setConversationLoadError(null);
      return;
    }
    const conversationId = selectedConversationId;
    if (selectedConversationHydrated) {
      setLoadingConversationId((current) => (current === conversationId ? null : current));
      return;
    }
    if (hydratingConversationIdsRef.current.has(conversationId)) {
      return;
    }

    let cancelled = false;
    hydratingConversationIdsRef.current.add(conversationId);
    setLoadingConversationId(conversationId);
    setConversationLoadError(null);

    // hydrateConversation loads the selected conversation without reloading the whole workbench.
    // hydrateConversation 在不重载整个工作台的情况下加载当前选中会话正文。
    async function hydrateConversation() {
      try {
        const response = await loadConversationMessages(conversationId);
        if (cancelled) {
          return;
        }
        const hydratedWorkbench = {
          ...response.workbench,
          conversationHydration: response.progress,
        };
        setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, hydratedWorkbench));
      } catch (nextError) {
        if (!cancelled) {
          setConversationLoadError(nextError instanceof Error ? nextError.message : String(nextError));
        }
      } finally {
        hydratingConversationIdsRef.current.delete(conversationId);
        if (!cancelled) {
          setLoadingConversationId(null);
        }
      }
    }

    void hydrateConversation();
    return () => {
      cancelled = true;
    };
  }, [
    draftConversationId,
    mergeWorkbenchWithLocalCaches,
    selectedConversationHydrated,
    selectedConversationId,
    setWorkbenchState,
  ]);

  // handleLoadConversationHistory requests one older turn page for the selected conversation.
  // handleLoadConversationHistory 为当前选中会话请求一页更早轮次。
  const handleLoadConversationHistory = React.useCallback(async (conversationId: string): Promise<boolean> => {
    const window = hydratedConversationWindowById.get(conversationId);
    const cursor = window ? conversationHistoryCursorFromWindow(window, conversationHistoryLoadingIds.has(conversationId)) : null;
    if (!cursor?.hasMoreHistory || cursor.earliestOrdinal == null || cursor.loading) {
      return false;
    }
    setConversationHistoryLoadingIds((current) => new Set(current).add(conversationId));
    try {
      const response = await loadConversationHistoryPage(conversationId, cursor.earliestOrdinal);
      setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, response.workbench));
      setConversationHistoryLoadingIds((current) => {
        const next = new Set(current);
        next.delete(conversationId);
        return next;
      });
      return true;
    } catch (error) {
      console.error("Failed to load older conversation history", error);
      setConversationHistoryLoadingIds((current) => {
        const next = new Set(current);
        next.delete(conversationId);
        return next;
      });
      return false;
    }
  }, [conversationHistoryLoadingIds, hydratedConversationWindowById, mergeWorkbenchWithLocalCaches, setWorkbenchState]);

  return {
    loadingConversationId,
    conversationLoadError,
    conversationHistoryLoadingIds,
    handleLoadConversationHistory,
  };
}
