import React from "react";
import { toast } from "sonner";
import type { SkillSubAreaId } from "@/components/features/skills/skills-area";
import type { WorkspaceMainView } from "@/components/workspace/workspace-shell/types";
import { markConversationRead, selectConversation } from "@/service/project-actions-service";
import type { ConversationLiveStreamStore } from "@/state/conversation-live-stream-store";
import { markConversationLocallyRead, type WorkbenchMergeOptions } from "@/state/workbench-state";
import type { MainAreaId, WorkbenchSnapshot } from "@/types";

// ConversationNavigationState owns visual feedback while the authoritative route remains on its previous session.
// ConversationNavigationState 在权威路由仍属于原会话期间管理视觉反馈。
export interface ConversationNavigationState {
  // Conversation displayed when the navigation started.
  // 导航开始时显示的会话。
  sourceId: string | null;
  // Conversation requested by the navigation.
  // 导航请求的会话。
  targetId: string;
  // Current visual transition phase.
  // 当前视觉过渡阶段。
  phase: "exiting" | "loading" | "waiting-owner" | "entering";
  // Whether the target lacked an initialized event window when it was clicked.
  // 点击时目标是否缺少已初始化的事件窗口。
  showLoading: boolean;
}

// ConversationSelectionNavigationOptions supplies the route state and snapshot merges owned by the workbench.
// ConversationSelectionNavigationOptions 提供工作台持有的路由状态与快照合并。
export interface ConversationSelectionNavigationOptions {
  // Conversation currently selected by the local route.
  // 本地路由当前选中的会话。
  selectedConversationId: string | null;
  // Latest selection visible to async completions.
  // 异步完成回调可见的最新选择。
  selectedConversationIdRef: React.MutableRefObject<string | null>;
  // Commits the local route.
  // 提交本地路由。
  setSelectedConversationId: React.Dispatch<React.SetStateAction<string | null>>;
  // Updates the local workbench projection.
  // 更新本地工作台投影。
  setWorkbenchState: React.Dispatch<React.SetStateAction<WorkbenchSnapshot>>;
  // Latest workbench visible to async completions.
  // 异步完成回调可见的最新工作台。
  workbenchStateRef: React.MutableRefObject<WorkbenchSnapshot>;
  // Merges a service snapshot while preserving UI-local caches.
  // 合并服务快照并保留界面本地缓存。
  mergeWorkbenchWithLocalCaches: (current: WorkbenchSnapshot, incoming: WorkbenchSnapshot, options?: WorkbenchMergeOptions) => WorkbenchSnapshot;
  // Passes a service snapshot through the revision gate; returns false when it is stale.
  // 让服务快照通过版本闸门；过期时返回 false。
  acceptServiceWorkbenchSnapshot: (snapshot: WorkbenchSnapshot) => boolean;
  // Live stream store reconciled before the new conversation subtree mounts.
  // 新会话子树挂载前完成对账的实时流存储。
  liveStreamStore: ConversationLiveStreamStore;
  // Returns the workbench main panel to the conversation surface.
  // 将工作台主面板切回会话界面。
  handleOpenWorkspaceChat: () => void;
  // Reports whether the exact conversation already owns a hydrated event window.
  // 报告精确会话是否已有完成初始化的事件窗口。
  isConversationInitialized: (conversationId: string) => boolean;
  // Current feature area; leaving it cancels diagnostic-owner navigation.
  // 当前功能区域；离开时取消诊断所有者导航。
  activeArea: MainAreaId;
  // Current workbench main view; leaving it cancels diagnostic-owner navigation.
  // 当前工作台主视图；离开时取消诊断所有者导航。
  workspaceMainView: WorkspaceMainView;
  // Current plugin-management sub page; leaving it cancels diagnostic-owner navigation.
  // 当前插件管理子页面；离开时取消诊断所有者导航。
  activeSkillSubArea: SkillSubAreaId;
}

// ConversationSelectionNavigation exposes the visual transition and the selection handlers.
// ConversationSelectionNavigation 暴露视觉过渡与选择处理器。
export interface ConversationSelectionNavigation {
  // Current visual transition, or null when none is running.
  // 当前视觉过渡；没有过渡时为 null。
  conversationNavigation: ConversationNavigationState | null;
  // Starts the reveal once the exact target owner is ready.
  // 精确目标归属就绪后开始入场。
  handleConversationNavigationReady: (conversationId: string) => void;
  // Selects a conversation from the sidebar, notifications or slash routes.
  // 从侧栏、通知或指令路由选择会话。
  handleSelectConversation: (conversationId: string) => void;
  // Confirms a plugin diagnostic's owner conversation before leaving diagnostics; resolves true after exact confirmation.
  // 离开诊断前确认插件诊断的所有者会话；精确确认后解析为 true。
  handleSelectPluginResourceOwner: (conversationId: string, observation: AbortSignal) => Promise<boolean>;
}

// useConversationSelectionNavigation owns conversation selection ordering and its visual transition.
// useConversationSelectionNavigation 持有会话选择的先后顺序及其视觉过渡。
// Parameters: options supplies route state, snapshot merges and the navigation scope.
// 参数：options 提供路由状态、快照合并与导航作用域。
// Returns: the transition state and the selection handlers.
// 返回值：过渡状态与选择处理器。
export function useConversationSelectionNavigation(options: ConversationSelectionNavigationOptions): ConversationSelectionNavigation {
  const {
    selectedConversationId,
    selectedConversationIdRef,
    setSelectedConversationId,
    setWorkbenchState,
    workbenchStateRef,
    mergeWorkbenchWithLocalCaches,
    acceptServiceWorkbenchSnapshot,
    liveStreamStore,
    handleOpenWorkspaceChat,
    isConversationInitialized,
    activeArea,
    workspaceMainView,
    activeSkillSubArea,
  } = options;
  // conversationSelectionRequestVersionRef prevents an older selection response from committing after a newer click.
  // conversationSelectionRequestVersionRef 防止较早的选择响应在较新的点击之后提交。
  const conversationSelectionRequestVersionRef = React.useRef(0);
  // pluginOwnerNavigationVersionRef cancels diagnostic-owner navigation when its source page leaves or disposes.
  // pluginOwnerNavigationVersionRef 在来源页面离开或销毁时取消诊断所有者导航。
  const pluginOwnerNavigationVersionRef = React.useRef(0);
  React.useLayoutEffect(() => {
    pluginOwnerNavigationVersionRef.current += 1;
    return () => { pluginOwnerNavigationVersionRef.current += 1; };
  }, [activeArea, workspaceMainView, activeSkillSubArea]);
  // conversationNavigation owns visual feedback while the authoritative route remains on its previous session.
  // conversationNavigation 在权威路由仍属于原会话期间管理视觉反馈。
  const [conversationNavigation, setConversationNavigation] = React.useState<ConversationNavigationState | null>(null);
  // External route changes cancel an in-flight visual selection and invalidate its service response.
  // 外部路由变化会取消正在进行的视觉选择，并使其服务响应失效。
  React.useEffect(() => {
    if (conversationNavigation
      && selectedConversationId !== conversationNavigation.sourceId
      && selectedConversationId !== conversationNavigation.targetId) {
      conversationSelectionRequestVersionRef.current += 1;
      setConversationNavigation(null);
    }
  }, [conversationNavigation, selectedConversationId]);

  // handleConversationNavigationReady starts the 200 ms reveal only for the current target owner.
  // handleConversationNavigationReady 仅为当前目标归属启动 200 毫秒入场。
  const handleConversationNavigationReady = React.useCallback((conversationId: string) => {
    const requestVersion = conversationSelectionRequestVersionRef.current;
    setConversationNavigation((current) => current?.targetId === conversationId && current.phase === "waiting-owner"
      ? { ...current, phase: "entering" }
      : current);
    window.setTimeout(() => {
      if (conversationSelectionRequestVersionRef.current === requestVersion) {
        setConversationNavigation((current) => current?.targetId === conversationId && current.phase === "entering" ? null : current);
      }
    }, 200);
  }, []);

  // handleSelectConversation selects a conversation with immediate sidebar feedback and exact-owner reveal.
  // handleSelectConversation 以即时侧栏反馈和精确归属入场选择会话。
  const handleSelectConversation = React.useCallback((conversationId: string) => {
    handleOpenWorkspaceChat();
    // requestVersion keeps old responses and animations from overtaking the newest click.
    // requestVersion 防止旧响应和动画越过最新点击。
    const requestVersion = conversationSelectionRequestVersionRef.current + 1;
    conversationSelectionRequestVersionRef.current = requestVersion;
    const sourceId = selectedConversationIdRef.current;
    // showLoading is based on the registry's exact initialized window at click time.
    // showLoading 根据点击时 registry 中精确会话的已初始化窗口判定。
    const showLoading = !isConversationInitialized(conversationId);
    setConversationNavigation({ sourceId, targetId: conversationId, phase: "exiting", showLoading });
    const fadedOut = new Promise<void>((resolve) => {
      window.setTimeout(() => {
        if (conversationSelectionRequestVersionRef.current === requestVersion) {
          setConversationNavigation((current) => current?.targetId === conversationId && current.phase === "exiting"
            ? { ...current, phase: "loading" }
            : current);
        }
        resolve();
      }, 200);
    });
    void selectConversation(conversationId)
      .then(async (snapshot) => {
        await fadedOut;
        if (conversationSelectionRequestVersionRef.current !== requestVersion) {
          return null;
        }
        // A non-null service snapshot is authoritative even when missing storage changed or cleared its selection.
        // 非空服务快照始终具有权威性，即使存储缺失使其选择改变或清空。
        if (snapshot && snapshot.selectedConversationId !== conversationId) {
          selectedConversationIdRef.current = snapshot.selectedConversationId ?? null;
          setSelectedConversationId(snapshot.selectedConversationId ?? null);
          setConversationNavigation(null);
          liveStreamStore.resetForWorkbenchSnapshot();
          liveStreamStore.flushNotifications();
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(
            current,
            snapshot,
            { selectedConversationSource: "incoming" },
          ));
          return null;
        }
        // Commit the route and its session-sensitive controls atomically after the exact selection response arrives.
        // 精确选择响应到达后，原子提交路由及其会话敏感控制字段。
        selectedConversationIdRef.current = conversationId;
        setSelectedConversationId(conversationId);
        if (snapshot && snapshot.selectedConversationId === conversationId) {
          liveStreamStore.resetForWorkbenchSnapshot();
          liveStreamStore.flushNotifications();
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(
            current,
            snapshot,
            { selectedConversationSource: "incoming" },
          ));
          setConversationNavigation({ sourceId, targetId: conversationId, phase: "waiting-owner", showLoading });
        } else {
          // Browser preview has no service snapshot but still owns a local-only route.
          // 浏览器预览没有服务快照，但仍持有本地路由。
          setWorkbenchState((current) => markConversationLocallyRead(current, conversationId));
          setConversationNavigation({ sourceId, targetId: conversationId, phase: "entering", showLoading });
          window.setTimeout(() => {
            if (conversationSelectionRequestVersionRef.current === requestVersion) {
              setConversationNavigation((current) => current?.targetId === conversationId && current.phase === "entering" ? null : current);
            }
          }, 200);
        }
        // Read-marker persistence is ancillary and must never turn a successful navigation into an error toast.
        // 已读标记持久化属于附属操作，绝不能把成功导航变成错误提示。
        return markConversationRead(conversationId).catch((error) => {
          console.warn("[workbench] conversation read marker update failed", error);
          return null;
        });
      })
      .then((snapshot) => {
        // Read-marker responses can arrive after another conversation has already been selected.
        // 已读标记响应可能在另一个会话已经被选中后才到达。
        if (
          snapshot
          && conversationSelectionRequestVersionRef.current === requestVersion
          && selectedConversationIdRef.current === conversationId
          && snapshot.selectedConversationId === conversationId
        ) {
          liveStreamStore.resetForWorkbenchSnapshot();
          liveStreamStore.flushNotifications();
          setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot));
        }
      })
      .catch((error) => {
        if (conversationSelectionRequestVersionRef.current !== requestVersion) {
          return;
        }
        setConversationNavigation({ sourceId, targetId: conversationId, phase: "entering", showLoading });
        window.setTimeout(() => {
          if (conversationSelectionRequestVersionRef.current === requestVersion) {
            setConversationNavigation((current) => current?.targetId === conversationId && current.phase === "entering" ? null : current);
          }
        }, 200);
        const message = error instanceof Error ? error.message : String(error);
        toast.error(message);
      });
  }, [
    handleOpenWorkspaceChat,
    isConversationInitialized,
    liveStreamStore,
    selectedConversationIdRef,
    setSelectedConversationId,
  ]);

  // handleSelectPluginResourceOwner confirms the original conversation before leaving diagnostics.
  // handleSelectPluginResourceOwner 离开诊断前确认原会话。
  // conversationId is the retained host owner; observation cancels only local navigation, returning true after exact confirmation.
  // conversationId 是保留宿主所有者；observation 仅取消本地导航，精确确认后返回真值。
  const handleSelectPluginResourceOwner = React.useCallback(async (conversationId: string, observation: AbortSignal): Promise<boolean> => {
    if (observation.aborted) return false;
    // requestVersion prevents a late owner lookup from overtaking a newer sidebar or diagnostics selection.
    // requestVersion 阻止迟到所有者查询越过较新侧栏或诊断选择。
    const requestVersion = ++conversationSelectionRequestVersionRef.current;
    // Retiring the previous selection also retires its visual transition, whose old response can no longer clear it.
    // 使先前选择失效时也结束其视觉过渡，因为旧响应已无法清除该过渡。
    setConversationNavigation(null);
    // ownerVersion fences the original diagnostics page independently from generic sidebar selection.
    // ownerVersion 独立于通用侧栏选择约束原诊断页面。
    const ownerVersion = ++pluginOwnerNavigationVersionRef.current;
    // sourceId preserves the currently displayed owner until the service confirms the requested identity.
    // sourceId 保留当前显示所有者，直至服务确认请求身份。
    const sourceId = selectedConversationIdRef.current;
    // snapshot can legitimately select another remaining conversation after missing storage cleanup.
    // snapshot 在缺失存储清理后可合法选中另一个剩余会话。
    const snapshot = await selectConversation(conversationId);
    if (observation.aborted || conversationSelectionRequestVersionRef.current !== requestVersion || pluginOwnerNavigationVersionRef.current !== ownerVersion || snapshot === null
      || snapshot.selectedConversationId !== conversationId) return false;
    // Both projections must still contain this owner; the shared gate rejects replies older than already accepted lifecycle updates.
    // 两份投影必须仍包含此所有者；共享闸门拒绝早于已接受生命周期更新的响应。
    if (!workbenchStateRef.current.conversations.some((conversation) => conversation.id === conversationId)
      || !snapshot.conversations.some((conversation) => conversation.id === conversationId)
      || !acceptServiceWorkbenchSnapshot(snapshot)) return false;
    // Only this confirmed owner may publish a route change; sidebar fallback semantics remain separate.
    // 仅此已确认所有者可以发布路由变更；侧栏回退语义保持独立。
    handleOpenWorkspaceChat();
    selectedConversationIdRef.current = conversationId;
    setSelectedConversationId(conversationId);
    liveStreamStore.resetForWorkbenchSnapshot();
    liveStreamStore.flushNotifications();
    setWorkbenchState((current) => mergeWorkbenchWithLocalCaches(current, snapshot, { selectedConversationSource: "incoming" }));
    setConversationNavigation({ sourceId, targetId: conversationId, phase: "waiting-owner", showLoading: !isConversationInitialized(conversationId) });
    // Read persistence is ancillary; a later marker snapshot must not change the exact navigation decision.
    // 已读持久化属于附属操作；迟到标记快照不得改变精确导航决定。
    void markConversationRead(conversationId).catch((error) => console.warn("[workbench] conversation read marker update failed", error));
    return true;
  }, [acceptServiceWorkbenchSnapshot, handleOpenWorkspaceChat, liveStreamStore, mergeWorkbenchWithLocalCaches, isConversationInitialized]);

  return {
    conversationNavigation,
    handleConversationNavigationReady,
    handleSelectConversation,
    handleSelectPluginResourceOwner,
  };
}
