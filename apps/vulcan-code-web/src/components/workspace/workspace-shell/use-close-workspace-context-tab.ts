import { useCallback, useRef, type Dispatch, type SetStateAction } from "react";
import { readElectronLocalBrowserBridge } from "@/host-shell/electron-bridge";
import { toast } from "@/ui-kit/primitives/toast";
import {
  closeWorkspaceContextTab,
  workspaceContextTabStateCache,
  type WorkspaceContextTabStateEntry,
} from "./context-tabs";

// useCloseWorkspaceContextTab returns a tab close action that releases local browser pages before removing their panel.
// useCloseWorkspaceContextTab 返回页签关闭动作，在移除本地浏览器面板前释放其页面。
export function useCloseWorkspaceContextTab(
  // The tab list rendered by the workspace, paired with its cache identity.
  // 工作区正在渲染的页签列表及其缓存标识。
  { cacheKey, state: { tabs } }: WorkspaceContextTabStateEntry,
  // The selected route's cache identity, which may change before its tab state loads.
  // 当前路由的缓存标识，可能先于页签状态更新。
  selectedCacheKey: string,
  // The selected conversation that owns the native browser pages.
  // 拥有原生浏览器页面的当前会话。
  ownerSessionId: string | null,
  // Updates the visible tab state only if it still belongs to the originating route.
  // 仅当可见页签仍属于发起路由时更新其状态。
  setTabState: Dispatch<SetStateAction<WorkspaceContextTabStateEntry>>,
): (tabId: string) => void {
  // Track in-flight closes per owner so rapid clicks cannot issue duplicate native requests.
  // 按会话记录进行中的关闭操作，避免连续点击发出重复原生请求。
  const pendingOwners = useRef(new Set<string>());

  // Close the exact tab identified by tabId and report native failures through the workspace toast.
  // 关闭 tabId 标识的页签，并通过工作区通知显示原生关闭失败。
  return useCallback((tabId: string): void => {
    if (cacheKey !== selectedCacheKey) return;

    // Resolve the clicked tab even when a different tab is active.
    // 即使当前激活的是其他页签，也按点击的标识定位。
    const tab = tabs.find((candidate) => candidate.id === tabId);
    if (!tab) return;

    // Apply the close to the latest visible state for this route only.
    // 只在此路由的最新可见状态上移除页签。
    const removeTab = (): void => {
      setTabState((current) => current.cacheKey === cacheKey
        ? { ...current, state: closeWorkspaceContextTab(current.state, tabId) }
        : current);
    };
    if (tab.kind !== "local-browser") {
      removeTab();
      return;
    }

    // Native closure is keyed by the captured owner and does not require a current-page snapshot.
    // 原生关闭按点击时的会话标识执行，无需查询当前页面快照。
    const bridge = readElectronLocalBrowserBridge();
    if (!bridge || !ownerSessionId || pendingOwners.current.has(ownerSessionId)) return;
    pendingOwners.current.add(ownerSessionId);
    void (async () => {
      try {
        if (!await bridge.closeOwner(ownerSessionId, true)) throw new Error("Browser close was not completed.");

        // Update the originating cache as well so switching conversations cannot restore a closed panel.
        // 同步更新发起会话的缓存，避免切换会话后恢复已经关闭的面板。
        const cached = workspaceContextTabStateCache.get(cacheKey);
        if (cached) workspaceContextTabStateCache.set(cacheKey, closeWorkspaceContextTab(cached, tabId));
        removeTab();
      } catch (error) {
        toast.error(error instanceof Error ? error.message : String(error));
      } finally {
        pendingOwners.current.delete(ownerSessionId);
      }
    })();
  }, [cacheKey, ownerSessionId, selectedCacheKey, setTabState, tabs]);
}
