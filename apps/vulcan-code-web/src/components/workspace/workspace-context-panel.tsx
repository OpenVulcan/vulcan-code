import React from "react";
import { ChildLifetimeUsageChip } from "@/components/conversation/session-lifetime-usage";
import { useThinkingDisplayPreference } from "@/components/conversation/thinking-display-preferences";
import { toast } from "@/ui-kit/primitives/toast";
import {
  ArrowDown,
  Bot,
  Brain,
  ChevronsDown,
  FileText,
  Folder,
  Globe2,
  GitPullRequest,
  ListTree,
  Plus,
  Search,
  Square,
  X,
} from "lucide-react";
import type { ConversationScrollSnapshot } from "@/components/conversation/chat-area-scroll";
import { useConversationScrollController } from "@/components/conversation/chat-area/conversation-scroll-controller";
import { RenderErrorBoundary } from "@/app/render-error-boundary";
import { PageNotice } from "@/components/page/page-feedback";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui-kit/primitives/empty";
import { PanelTab } from "@/ui-kit/primitives/panel-tab";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { ConversationBranchWorkspaceV1 } from "@/components/conversation/conversation-branch-workspace-v1";
import { TransientReasoningStatusLine } from "@/components/conversation/conversation-live-reasoning";
import { ProviderRetryCountdownLine } from "@/components/conversation/conversation-provider-retry-card";
import type {
  ConversationSurfaceExpandedState,
} from "@/components/conversation/conversation-layout-restore";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import type {
  DiffReviewSelection,
  WorkspaceGitFileActionRequest,
  WorkspaceGitFileActionResult,
  WorkspaceDiffFileDetailRequest,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffSearchRequest,
  WorkspaceDiffSearchResponse,
  WorkspaceDiffSnapshotResponse,
  WorkspaceReviewFocusRequest,
} from "@/diff/types";
import { cx } from "@/ui-kit/behavior/core";
import { useI18n, type TranslationKey } from "@/i18n";
import {
  useConversationProviderRetryStatus,
  useConversationTransientReasoningStatus,
} from "@/state/conversation-live-stream-context";
import { useConversationSessionEventWindowOwnerV1 } from "@/state/conversation-session-event-window-owner-v1";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";
import type {
  VmmMemoryActionRequest,
  VmmMemoryActionResult,
  WorkbenchSnapshot,
  WorkspaceFileDirectoryListing,
  WorkspaceFileFilterOptions,
  WorkspaceFileReadResponse,
} from "@/types";
import { WorkspaceMemoryPanel } from "@/components/workspace/workspace-memory-panel";
import { workspaceContextTabDefaultTitle } from "@/components/workspace/workspace-context-tab-title";

// LazyWorkspaceReviewPanelHost loads the heavy Git review surface only when the context rail renders it.
// LazyWorkspaceReviewPanelHost 仅在上下文栏渲染审核页时加载较重的 Git 审核界面。
const LazyWorkspaceReviewPanelHost = React.lazy(async () => {
  const module = await import("@/components/workspace/workspace-review-panel-host");
  return { default: module.WorkspaceReviewPanelHost };
});

// LazyWorkspaceFilesPanelHost loads the filesystem browser only when the context rail renders it.
// LazyWorkspaceFilesPanelHost 仅在上下文栏渲染文件页时加载文件系统浏览器。
const LazyWorkspaceFilesPanelHost = React.lazy(async () => {
  const module = await import("@/components/workspace/workspace-files-panel-host");
  return { default: module.WorkspaceFilesPanelHost };
});

// LazyClientBrowserArea mounts this window's conversation-scoped page only while its right tab is active.
// LazyClientBrowserArea 仅在当前会话的右侧页签激活时挂载此窗口的本地网页。
// LazyFileViewerTab loads the file viewers only when a viewer tab shows.
// LazyFileViewerTab 仅在显示查看页签时加载文件查看器。
const LazyFileViewerTab = React.lazy(() => import("@/components/file-viewers/file-viewer-tab"));

const LazyClientBrowserArea = React.lazy(async () => {
  const module = await import("@/components/features/browser/client-browser-area");
  return { default: module.ClientBrowserArea };
});

// LazySessionActivityPanel loads the activity panel and its styles only when its tab shows.
// LazySessionActivityPanel 仅在活动页签显示时加载活动面板及其样式。
const LazySessionActivityPanel = React.lazy(async () => {
  const module = await import("@/components/workspace/session-activity/session-activity-panel");
  return { default: module.SessionActivityPanel };
});

// WorkspaceContextPanelTab identifies one registered right context panel page.
// WorkspaceContextPanelTab 标识一个已注册的右侧上下文面板页面。
export type WorkspaceContextPanelTab = "review" | "memory" | "files" | "subagent" | "browser" | "local-browser" | "viewer" | "activity";

// WorkspaceContextTabKind identifies one dynamic right context tab type.
// WorkspaceContextTabKind 标识一个动态右侧上下文页签类型。
export type WorkspaceContextTabKind = WorkspaceContextPanelTab;

// WorkspaceContextTabItem describes one opened right context tab.
// WorkspaceContextTabItem 描述一个已打开的右侧上下文页签。
export interface WorkspaceContextTabItem {
  // Stable tab id for React keys and activation.
  // 用于 React key 与激活状态的稳定页签标识。
  id: string;
  // Registered content kind rendered by this tab.
  // 该页签渲染的已注册内容类型。
  kind: WorkspaceContextTabKind;
  // Visible tab title.
  // 可见页签标题。
  title: string;
  // Optional selected file name for file tabs.
  // 文件页签可选的已选中文件名。
  selectedFileName?: string | null;
  // Absolute path of the file a viewer tab shows, which may lie outside the project.
  // 查看页签显示的文件绝对路径，可位于项目之外。
  viewerPath?: string | null;
  // Optional subagent child task conversation id for subagent tabs.
  // 子代理页签可选的子任务会话标识。
  subagentTaskId?: string | null;
  // Optional parent conversation id that owns the subagent task.
  // 拥有子代理任务的可选父会话标识。
  subagentParentConversationId?: string | null;
  // Page an activity tab shows.
  // 活动页签显示的页面。
  activityPage?: "subagents" | "commands";
  // Increases with every request to open the activity tab, so the same page can be requested again.
  // 每次请求打开活动页签都会递增，使同一页面可再次被请求。
  activityRequest?: number;
  // Creation timestamp used for stable ordering diagnostics.
  // 用于稳定排序诊断的创建时间戳。
  createdAt: number;
}

// WorkspaceContextTabState stores the opened right context tab workspace.
// WorkspaceContextTabState 保存已打开的右侧上下文页签工作区。
export interface WorkspaceContextTabState {
  // Opened tabs in visual order.
  // 按视觉顺序排列的已打开页签。
  tabs: WorkspaceContextTabItem[];
  // Active tab id, or null when no tab is opened.
  // 当前活动页签标识；没有页签时为空。
  activeTabId: string | null;
}

// WorkspaceDraftConversationSelection carries a local draft conversation route.
// WorkspaceDraftConversationSelection 承载本地草稿会话路由。
export interface WorkspaceDraftConversationSelection {
  // Draft conversation id used by the selected route.
  // 当前选中路由使用的草稿会话标识。
  id: string;
  // Project id that owns the draft conversation.
  // 草稿会话所属的项目标识。
  projectId: string;
}

// WorkspaceContextPanelProps describes the right-side workspace context panel boundary.
// WorkspaceContextPanelProps 描述右侧工作区上下文面板边界。
export interface WorkspaceContextPanelProps {
  // Whether this client has a complete native local-browser bridge.
  // 当前客户端是否具备完整的原生本地浏览器桥。
  localBrowserAvailable: boolean;
  // Whether the selected conversation has an active Core browser to display.
  // 当前会话是否有可显示的活跃 Core 浏览器。
  aiBrowserAvailable?: boolean;
  // Browser plugin content for this exact conversation, mounted only on its tab.
  // 仅在当前会话浏览器页签挂载的浏览器插件内容。
  aiBrowserPanel?: React.ReactNode;
  // Current draft conversation selection when the selected route is not persisted yet.
  // 当前路由尚未持久化时对应的草稿会话选择。
  selectedDraftConversation: WorkspaceDraftConversationSelection | null;
  // Current selected conversation id from the local workbench route.
  // 来自本地工作台路由的当前选中会话标识。
  selectedConversationId: string | null;
  // Workbench snapshot carrying selected-session side-panel projections.
  // 携带当前选中会话侧面板投影的工作台快照。
  workbench: WorkbenchSnapshot;
  // Whether the panel is rendered as a floating overlay.
  // 面板是否以浮动覆盖层方式渲染。
  floating?: boolean;
  // Opened dynamic context tabs.
  // 已打开的动态上下文页签。
  tabs: WorkspaceContextTabItem[];
  // Active dynamic context tab id.
  // 当前活动的动态上下文页签标识。
  activeTabId: string | null;
  // Opens or focuses one context tab kind.
  // 打开或聚焦一个上下文页签类型。
  onOpenContextTab: (kind: WorkspaceContextTabKind) => void;
  // Closes one opened context tab.
  // 关闭一个已打开的上下文页签。
  onCloseContextTab: (tabId: string) => void;
  // Activates one opened context tab.
  // 激活一个已打开的上下文页签。
  onActivateContextTab: (tabId: string) => void;
  // Updates one file tab title after file selection changes.
  // 文件选择变化后更新单个文件页签标题。
  onFileTabTitleChange: (tabId: string, title: string) => void;
  // Monotonic request id used to focus the review tab on uncommitted changes.
  // 用于让审核标签页聚焦未提交变更的单调请求标识。
  reviewFocusRequest: WorkspaceReviewFocusRequest;
  // Closes the right context panel.
  // 关闭右侧上下文面板。
  onClose?: () => void;
  // Force-stops one live command session.
  // 强制停止单条实时命令会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
  // Copies raw diff text using the host-aware clipboard implementation.
  // 使用宿主感知的剪贴板实现复制原始 diff 文本。
  onCopyRawDiff: (rawDiff: string) => void | Promise<void>;
  // Copies plain text using the host-aware clipboard implementation.
  // 使用宿主感知的剪贴板实现复制普通文本。
  onCopyText: (text: string) => void | Promise<void>;
  // Resolves one local media path into a WebUI-renderable URL.
  // 将单个本地媒体路径解析为 WebUI 可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Runs one VMM memory management action.
  // 执行单个 VMM 记忆管理动作。
  onRunVmmAction: (request: VmmMemoryActionRequest) => Promise<VmmMemoryActionResult>;
  // Opens the memory settings page from the memory tab.
  // 从记忆标签页打开记忆设置页面。
  onOpenMemorySettings?: () => void;
  // Opens one subagent transcript in the right context panel.
  // 在右侧上下文面板中打开单个子代理转录。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Stops one running subagent task.
  // 停止单个正在运行的子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Closes one open subagent lifecycle.
  // 关闭单个未关闭子智能体生命周期。
  onCloseSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Loads one live workspace Git diff snapshot.
  // 加载一次实时工作区 Git diff 快照。
  loadWorkspaceDiffSnapshot: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
  ) => Promise<WorkspaceDiffSnapshotResponse>;
  // Loads one live workspace Git diff file detail.
  // 加载一次实时工作区 Git diff 文件详情。
  loadWorkspaceDiffFileDetail: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
    file: WorkspaceDiffFileDetailRequest,
    options?: WorkspaceDiffFileDetailLoadOptions,
  ) => Promise<WorkspaceDiffFileDetailResponse>;
  // Loads compact backend file-level search hits for one workspace diff.
  // 为单个工作区 diff 加载紧凑的后端文件级搜索命中。
  loadWorkspaceDiffSearch: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: DiffReviewSelection,
    search: WorkspaceDiffSearchRequest,
  ) => Promise<WorkspaceDiffSearchResponse>;
  // Runs one file-level Git action for the current workspace review.
  // 为当前工作区审核执行一次文件级 Git 操作。
  runWorkspaceGitFileAction: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: WorkspaceGitFileActionRequest,
  ) => Promise<WorkspaceGitFileActionResult>;
  // Records one loaded workspace diff snapshot in the project-level Git store.
  // 将一次已加载的工作区 diff 快照记录到项目级 Git 状态。
  onWorkspaceDiffSnapshotLoaded?: (
    workspacePath: string,
    projectId: string | null,
    selection: DiffReviewSelection,
    response: WorkspaceDiffSnapshotResponse,
  ) => void;
  // Refreshes the project-level Git summary after a Git mutation.
  // Git 变更后刷新项目级 Git 摘要。
  onWorkspaceGitStateChanged?: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
  ) => Promise<void>;
  // Optional shared Git commit action label shown in the review toolbar.
  // 审核工具栏中展示的可选共享 Git 提交动作标签。
  gitCommitActionLabel?: string;
  // Whether the shared Git commit action is currently running.
  // 共享 Git 提交动作当前是否正在运行。
  gitCommitActionBusy?: boolean;
  // Whether the shared Git commit action is currently unavailable.
  // 共享 Git 提交动作当前是否不可用。
  gitCommitActionDisabled?: boolean;
  // Opens the shared Git commit or push dialog.
  // 打开共享 Git 提交或推送弹窗。
  onOpenGitCommitAction?: () => void;
  // Synchronizes the active right-panel repository with shared commit controls.
  // 将右侧面板当前仓库同步到共享提交控件。
  onSelectGitRepository?: (repositoryKey: string | null) => void;
  // Loads one lazy workspace file directory.
  // 加载单个懒加载工作区文件目录。
  loadWorkspaceFileDirectory: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    directoryPath: string | null,
    filters: WorkspaceFileFilterOptions,
  ) => Promise<WorkspaceFileDirectoryListing>;
  // Loads one workspace file preview response.
  // 加载单个工作区文件预览响应。
  loadWorkspaceFilePreview: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    filePath: string,
    filters: WorkspaceFileFilterOptions,
  ) => Promise<WorkspaceFileReadResponse>;
}

// WorkspaceContextPanel renders the dynamic right context tab workspace.
// WorkspaceContextPanel 渲染动态右侧上下文页签工作区。
// Parameters: props supplies the selected route, ordered tabs and typed service callbacks. Returns the context workspace.
// 参数：props 提供选中路由、有序页签及强类型服务回调。返回上下文工作区。
export function WorkspaceContextPanel(props: WorkspaceContextPanelProps) {
  // visibleTabs keeps AI pages tied to live Core sessions and local pages tied to this GUI bridge.
  // visibleTabs 使 AI 页面只随活跃 Core 会话显示，本地页面只随当前 GUI 桥显示。
  const visibleTabs = React.useMemo(() => props.tabs.filter((tab) =>
    (tab.kind !== "browser" || props.aiBrowserAvailable === true)
    && (tab.kind !== "local-browser" || props.localBrowserAvailable)),
  [props.aiBrowserAvailable, props.localBrowserAvailable, props.tabs]);
  // activeTab stores the currently visible opened tab.
  // activeTab 保存当前可见的已打开页签。
  const activeTab = React.useMemo(
    () => visibleTabs.find((tab) => tab.id === props.activeTabId) ?? visibleTabs[0] ?? null,
    [props.activeTabId, visibleTabs],
  );
  // tabRefs stores DOM nodes for quick scroll-to-tab navigation.
  // tabRefs 保存用于快速滚动定位页签的 DOM 节点。
  const tabRefs = React.useRef(new Map<string, HTMLDivElement>());
  // subagentScrollSnapshots stores semantic viewport state for every opened child transcript.
  // subagentScrollSnapshots 为每个已打开的子转录保存语义视口状态。
  const subagentScrollSnapshots = React.useRef(new Map<string, ConversationScrollSnapshot>());
  // subagentExpandedStates stores complete expansion memory for every opened child transcript.
  // subagentExpandedStates 为每个已打开的子转录保存完整展开记忆。
  const subagentExpandedStates = React.useRef(new Map<string, ConversationSurfaceExpandedState>());

  React.useEffect(() => {
    if (!activeTab) {
      return;
    }
    // activeTabElement stores the selected tab without asking outer scroll containers to reveal it.
    // activeTabElement 保存选中的页签，且不要求外层滚动容器将其显示出来。
    const activeTabElement = tabRefs.current.get(activeTab.id);
    // tabStrip is the only scroll container that owns horizontal tab navigation.
    // tabStrip 是唯一负责页签横向导航的滚动容器。
    const tabStrip = activeTabElement?.parentElement;
    if (!activeTabElement || !tabStrip) {
      return;
    }
    // activeTabRect provides the selected tab position inside the translated panel.
    // activeTabRect 提供平移面板内部所选页签的位置。
    const activeTabRect = activeTabElement.getBoundingClientRect();
    // tabStripRect provides the local viewport used to center the selected tab.
    // tabStripRect 提供用于居中所选页签的局部视口。
    const tabStripRect = tabStrip.getBoundingClientRect();
    // Keep scrolling local because the floating panel is translated off-screen during its mount frame.
    // 将滚动限制在页签条内，因为浮动面板挂载帧仍被平移到屏幕之外。
    tabStrip.scrollTo({
      behavior: "smooth",
      left: tabStrip.scrollLeft
        + activeTabRect.left
        - tabStripRect.left
        - ((tabStripRect.width - activeTabRect.width) / 2),
    });
  }, [activeTab]);

  return (
    <aside className="vk-panel vk-context-panel" data-floating={props.floating ? "" : undefined}>
      {activeTab ? (
        <>
          <WorkspaceContextTabBar
            activeTabId={activeTab.id}
            tabs={visibleTabs}
            aiBrowserAvailable={props.aiBrowserAvailable === true}
            localBrowserAvailable={props.localBrowserAvailable}
            tabRefs={tabRefs}
            onActivateTab={props.onActivateContextTab}
            onCloseTab={(tab) => {
              if (tab.kind === "files") {
                void import("@/components/workspace/workspace-files-panel-host")
                  .then((module) => module.forgetWorkspaceFilesPanelCacheInstance(tab.id))
                  .catch((error: unknown) => toast.error(error instanceof Error ? error.message : String(error)));
              }
              if (tab.kind === "subagent" && tab.subagentTaskId) {
                subagentScrollSnapshots.current.delete(tab.subagentTaskId);
                subagentExpandedStates.current.delete(tab.subagentTaskId);
              }
              props.onCloseContextTab(tab.id);
            }}
            onOpenTab={props.onOpenContextTab}
          />
          <WorkspaceContextTabBoundary resetKey={`${activeTab.id}:${props.selectedConversationId ?? ""}`}>
            <WorkspaceContextTabContent
              activeTab={activeTab}
              props={props}
              subagentExpandedStates={subagentExpandedStates}
              subagentScrollSnapshots={subagentScrollSnapshots}
            />
          </WorkspaceContextTabBoundary>
        </>
      ) : (
        <WorkspaceContextLauncher
          aiBrowserAvailable={props.aiBrowserAvailable === true}
          localBrowserAvailable={props.localBrowserAvailable}
          onOpenTab={props.onOpenContextTab}
        />
      )}
    </aside>
  );
}

// WorkspaceContextTabBoundary keeps a failure inside one context tab: the tab shows what went wrong and can be
// retried, while the tab bar, the conversation and the rest of the workbench keep working. Switching tab or
// session clears the failure.
// WorkspaceContextTabBoundary 将失败限制在单个上下文页签内：该页签说明出错原因并可重试，页签栏、会话与工作台其余部分照常可用。
// 切换页签或会话会清除该失败。
export function WorkspaceContextTabBoundary(props: { resetKey: string; children: React.ReactNode }) {
  const { t } = useI18n();
  // attempt remounts the tab content for an explicit retry.
  // attempt 在明确重试时重新挂载页签内容。
  const [attempt, setAttempt] = React.useState(0);
  return (
    <RenderErrorBoundary key={attempt} resetKey={props.resetKey} fallback={(error) => (
      <div className="vk-panel-body" data-fill="">
        <PageNotice
          actions={<Button onClick={() => setAttempt((value) => value + 1)} size="sm" variant="outline">{t("toolPresentation.output.retryLoad")}</Button>}
          className="vk-region-fault"
          details={error.message}
          detailsLabel={t("features.shell.errorDetails")}
          title={t("workspace.contextPanel.fault")}
          tone="error"
        >
          {t("workspace.contextPanel.faultHint")}
        </PageNotice>
      </div>
    )}>
      {props.children}
    </RenderErrorBoundary>
  );
}

// WorkspaceContextTabBar renders opened tabs plus switcher and add menus.
// WorkspaceContextTabBar 渲染已打开页签以及切换与新增菜单。
// Parameters: props supplies tab order, active identity, DOM references and callbacks. Returns the navigable tab strip.
// 参数：props 提供页签顺序、活动身份、DOM 引用和回调。返回可导航的页签栏。
function WorkspaceContextTabBar(props: {
  // Whether this GUI can open its own local browser page.
  // 此 GUI 是否可以打开自己的本地浏览器页面。
  localBrowserAvailable: boolean;
  // Whether the browser page can be opened from the add menu.
  // 浏览器页面是否可从新增菜单打开。
  aiBrowserAvailable: boolean;
  // Opened tabs in visual order.
  // 按视觉顺序排列的已打开页签。
  tabs: WorkspaceContextTabItem[];
  // Active tab id.
  // 当前活动页签标识。
  activeTabId: string;
  // DOM refs used by menu navigation.
  // 菜单导航使用的 DOM 引用。
  tabRefs: React.MutableRefObject<Map<string, HTMLDivElement>>;
  // Activates one tab.
  // 激活单个页签。
  onActivateTab: (tabId: string) => void;
  // Closes one tab.
  // 关闭单个页签。
  onCloseTab: (tab: WorkspaceContextTabItem) => void;
  // Opens one tab kind.
  // 打开单个页签类型。
  onOpenTab: (kind: WorkspaceContextTabKind) => void;
}) {
  // The tablist owns arrow navigation; close/add controls retain their independent native keyboard behavior.
  // 页签列表持有方向键导航；关闭/新增控件保留独立原生键盘行为。
  // Parameters: event is a bubbling tablist key event. Returns nothing; supported keys move focus and selection together.
  // 参数：event 是向页签列表冒泡的键盘事件。无返回值；支持的按键同步移动焦点与选择。
  const handleTabKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    if (!(event.target instanceof HTMLButtonElement) || event.target.getAttribute("role") !== "tab") return;
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    // DOM tab order exactly mirrors props.tabs and excludes each separate close button.
    // DOM 页签顺序与 props.tabs 精确一致，并排除各自独立的关闭按钮。
    const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]'));
    const currentIndex = buttons.indexOf(event.target);
    if (currentIndex < 0 || buttons.length === 0) return;
    const nextIndex = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1
      : (currentIndex + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) % buttons.length;
    event.preventDefault();
    props.onActivateTab(props.tabs[nextIndex].id);
    buttons[nextIndex].focus();
  };
  return (
    <div className="vk-panel-tabs">
      {props.tabs.length > 1 ? (
        <WorkspaceContextTabSwitcher
          activeTabId={props.activeTabId}
          tabs={props.tabs}
          onActivateTab={props.onActivateTab}
        />
      ) : null}
      <WorkspaceContextAddMenu
        tabs={props.tabs}
        aiBrowserAvailable={props.aiBrowserAvailable}
        localBrowserAvailable={props.localBrowserAvailable}
        onOpenTab={props.onOpenTab}
      />
      <div className="vk-panel-tabs-scroll" role="tablist" onKeyDown={handleTabKeyDown}>
        {props.tabs.map((tab) => (
          <WorkspaceContextTabButton
            active={tab.id === props.activeTabId}
            key={tab.id}
            tab={tab}
            tabRef={(element) => {
              if (element) {
                props.tabRefs.current.set(tab.id, element);
              } else {
                props.tabRefs.current.delete(tab.id);
              }
            }}
            onActivateTab={props.onActivateTab}
            onCloseTab={props.onCloseTab}
          />
        ))}
      </div>
    </div>
  );
}

// WorkspaceContextTabButton renders one dynamic tab with hover close affordance.
// WorkspaceContextTabButton 渲染一个带悬停关闭入口的动态页签。
// Parameters: props identifies the tab, active state and scoped actions. Returns separate select and close buttons.
// 参数：props 标识页签、活动状态与作用域化动作。返回独立的选择和关闭按钮。
function WorkspaceContextTabButton(props: {
  // Opened tab represented by this button.
  // 当前按钮代表的已打开页签。
  tab: WorkspaceContextTabItem;
  // Whether this tab is active.
  // 当前页签是否处于活动状态。
  active: boolean;
  // Ref callback used for scroll positioning.
  // 用于滚动定位的引用回调。
  tabRef: (element: HTMLDivElement | null) => void;
  // Activates one tab.
  // 激活单个页签。
  onActivateTab: (tabId: string) => void;
  // Closes one tab.
  // 关闭单个页签。
  onCloseTab: (tab: WorkspaceContextTabItem) => void;
}) {
  const { t } = useI18n();
  // displayTitle resolves singleton tab titles from the current locale at render time.
  // displayTitle 在渲染时按当前语言解析单例页签标题。
  const displayTitle = workspaceContextTabDisplayTitle(props.tab, t);
  return (
    <PanelTab
      active={props.active}
      closeLabel={t("workspaceContext.closeTab").replace("{title}", displayTitle)}
      icon={workspaceContextTabIcon(props.tab.kind, 14)}
      label={displayTitle}
      ref={props.tabRef}
      title={displayTitle}
      onClose={() => props.onCloseTab(props.tab)}
      onSelect={() => props.onActivateTab(props.tab.id)}
    />
  );
}

// WorkspaceContextTabSwitcher renders a searchable list of opened tabs.
// WorkspaceContextTabSwitcher 渲染可搜索的已打开页签列表。
function WorkspaceContextTabSwitcher(props: {
  // Opened tabs shown in the menu.
  // 菜单中展示的已打开页签。
  tabs: WorkspaceContextTabItem[];
  // Active tab id.
  // 当前活动页签标识。
  activeTabId: string;
  // Activates one tab.
  // 激活单个页签。
  onActivateTab: (tabId: string) => void;
}) {
  const { t } = useI18n();
  // query stores the local tab filter text.
  // query 保存本地页签筛选文本。
  const [query, setQuery] = React.useState("");
  // visibleTabs stores the menu rows after local filtering.
  // visibleTabs 保存本地过滤后的菜单行。
  const visibleTabs = React.useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) {
      return props.tabs;
    }
    return props.tabs.filter((tab) => workspaceContextTabDisplayTitle(tab, t).toLowerCase().includes(normalized));
  }, [props.tabs, query, t]);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button aria-label={t("workspaceContext.switchTabs")} className="vk-icon-btn" data-size="sm" type="button">
          <ChevronsDown />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="vk-context-menu">
        <label className="vk-menu-filter">
          <Search />
          <input
            aria-label={t("workspaceContext.searchTabs")}
            className="vk-input"
            onChange={(event) => setQuery(event.currentTarget.value)}
            onKeyDown={(event) => event.stopPropagation()}
            placeholder={t("workspaceContext.searchTabsPlaceholder")}
            value={query}
          />
          {query ? (
            <button
              aria-label={t("workspaceContext.clearTabSearch")}
              className="vk-icon-btn"
              data-size="sm"
              onClick={() => setQuery("")}
              type="button"
            >
              <X />
            </button>
          ) : null}
        </label>
        <DropdownMenuLabel>{t("workspaceContext.openTabs")}</DropdownMenuLabel>
        {visibleTabs.map((tab) => {
          const displayTitle = workspaceContextTabDisplayTitle(tab, t);
          return (
            <DropdownMenuItem
              data-active={tab.id === props.activeTabId ? "" : undefined}
              key={tab.id}
              onClick={() => props.onActivateTab(tab.id)}
            >
              {workspaceContextTabIcon(tab.kind, 14)}
              <span>{displayTitle}</span>
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// WorkspaceContextAddMenu renders the fixed plus menu for opening more tabs.
// WorkspaceContextAddMenu 渲染用于打开更多页签的固定加号菜单。
function WorkspaceContextAddMenu(props: {
  // Open tabs determine which singleton entries remain available.
  // 已打开页签决定哪些单实例入口仍然可用。
  tabs: WorkspaceContextTabItem[];
  // Whether this GUI can open its own local browser page.
  // 此 GUI 是否可以打开自己的本地浏览器页面。
  localBrowserAvailable: boolean;
  // Whether the current conversation owns a running AI browser.
  // 当前会话是否持有正在运行的 AI 浏览器。
  aiBrowserAvailable: boolean;
  // Opens one tab kind.
  // 打开单个页签类型。
  onOpenTab: (kind: WorkspaceContextTabKind) => void;
}) {
  const { t } = useI18n();
  // availableKinds excludes existing singletons; file pages always keep their multi-tab entry.
  // availableKinds 排除已经存在的单实例；文件页面始终保留多标签入口。
  const availableKinds = (["review", "memory", "local-browser", "browser"] as const).filter((kind) =>
    !props.tabs.some((tab) => tab.kind === kind)
    && (kind !== "local-browser" || props.localBrowserAvailable)
    && (kind !== "browser" || props.aiBrowserAvailable));
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button aria-label={t("workspaceContext.openRightPage")} className="vk-icon-btn" data-size="sm" type="button">
          <Plus />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="vk-context-menu">
        <DropdownMenuLabel>{t("workspaceContext.openPage")}</DropdownMenuLabel>
        <WorkspaceContextOpenMenuItem kind="files" onOpenTab={props.onOpenTab} />
        {availableKinds.length > 0 && <DropdownMenuSeparator />}
        {availableKinds.map((kind) => <WorkspaceContextOpenMenuItem key={kind} kind={kind} onOpenTab={props.onOpenTab} />)}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// WorkspaceContextOpenMenuItem renders one entry that opens a context tab kind.
// WorkspaceContextOpenMenuItem 渲染一个打开上下文页签类型的入口。
function WorkspaceContextOpenMenuItem(props: {
  // Context tab kind opened by this row.
  // 当前行打开的上下文页签类型。
  kind: WorkspaceContextTabKind;
  // Opens one tab kind.
  // 打开单个页签类型。
  onOpenTab: (kind: WorkspaceContextTabKind) => void;
}) {
  const { t } = useI18n();
  return (
    <DropdownMenuItem onClick={() => props.onOpenTab(props.kind)}>
      {workspaceContextTabIcon(props.kind, 14)}
      <span>{workspaceContextTabDefaultTitle(props.kind, t)}</span>
    </DropdownMenuItem>
  );
}

// WorkspaceContextLauncher renders the empty right-panel entry cards.
// WorkspaceContextLauncher 渲染右侧面板空状态入口卡片。
function WorkspaceContextLauncher(props: {
  // Whether this GUI can open its own local browser page.
  // 此 GUI 是否可以打开自己的本地浏览器页面。
  localBrowserAvailable: boolean;
  // Whether the current conversation owns a running AI browser.
  // 当前会话是否持有正在运行的 AI 浏览器。
  aiBrowserAvailable: boolean;
  // Opens one tab kind.
  // 打开单个页签类型。
  onOpenTab: (kind: WorkspaceContextTabKind) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="vk-panel-launch">
      <WorkspaceContextLauncherCard
        description={t("workspaceContext.launcher.filesDescription")}
        kind="files"
        title={t("contextPanel.files")}
        onOpenTab={props.onOpenTab}
      />
      <WorkspaceContextLauncherCard
        description={t("workspaceContext.launcher.reviewDescription")}
        kind="review"
        title={t("contextPanel.review")}
        onOpenTab={props.onOpenTab}
      />
      <WorkspaceContextLauncherCard
        description={t("workspaceContext.launcher.memoryDescription")}
        kind="memory"
        title={t("contextPanel.memory")}
        onOpenTab={props.onOpenTab}
      />
      {props.localBrowserAvailable ? <WorkspaceContextLauncherCard
        description={t("workspace.context.browsePagesInThis")}
        kind="local-browser"
        title={t("workspace.context.localBrowser")}
        onOpenTab={props.onOpenTab}
      /> : null}
      {props.aiBrowserAvailable ? <WorkspaceContextLauncherCard
        description={t("workspace.context.viewPagesOpenedBy")}
        kind="browser"
        title={t("workspace.context.aiBrowser")}
        onOpenTab={props.onOpenTab}
      /> : null}
    </div>
  );
}

// WorkspaceContextLauncherCard renders one empty-panel action card.
// WorkspaceContextLauncherCard 渲染一个空面板操作卡片。
// Parameters: props supplies the exact kind, localized labels and opening callback. Returns one native action button.
// 参数：props 提供精确种类、本地化文案与打开回调。返回一个原生操作按钮。
function WorkspaceContextLauncherCard(props: {
  // Context tab kind opened by this card.
  // 当前卡片打开的上下文页签类型。
  kind: WorkspaceContextTabKind;
  // Visible title.
  // 可见标题。
  title: string;
  // Secondary description.
  // 次级说明。
  description: string;
  // Opens one tab kind.
  // 打开单个页签类型。
  onOpenTab: (kind: WorkspaceContextTabKind) => void;
}) {
  return (
    <button className="vk-panel-launch-card" data-kind={props.kind} onClick={() => props.onOpenTab(props.kind)} type="button">
      <span aria-hidden className="vk-panel-launch-glyph">{workspaceContextTabIcon(props.kind, 18)}</span>
      <strong>{props.title}</strong>
      <span>{props.description}</span>
    </button>
  );
}

// WorkspaceContextTabContent renders the active dynamic tab content.
// WorkspaceContextTabContent 渲染当前活动动态页签内容。
function WorkspaceContextTabContent(args: {
  // Active opened tab.
  // 当前活动的已打开页签。
  activeTab: WorkspaceContextTabItem;
  // Parent panel props forwarded to content hosts.
  // 转发给内容宿主的父面板属性。
  props: WorkspaceContextPanelProps;
  // Semantic scroll memory shared by every child-transcript tab in this mounted panel.
  // 当前已挂载面板内所有子转录页签共享的语义滚动记忆。
  subagentScrollSnapshots: React.MutableRefObject<Map<string, ConversationScrollSnapshot>>;
  // Expansion memory shared by every child-transcript tab in this mounted panel.
  // 当前已挂载面板内所有子转录页签共享的展开记忆。
  subagentExpandedStates: React.MutableRefObject<Map<string, ConversationSurfaceExpandedState>>;
}) {
  // activeTabId stores the current file tab id for stable callbacks.
  // activeTabId 保存当前文件页签标识以稳定回调。
  const activeTabId = args.activeTab.id;
  // onFileTabTitleChange stores the parent title update callback.
  // onFileTabTitleChange 保存父级标题更新回调。
  const onFileTabTitleChange = args.props.onFileTabTitleChange;
  // handleFileTabTitleChange keeps file tab title updates stable across parent renders.
  // handleFileTabTitleChange 保持文件页签标题更新在父级重渲染间稳定。
  const handleFileTabTitleChange = React.useCallback((title: string) => {
    onFileTabTitleChange(activeTabId, title);
  }, [activeTabId, onFileTabTitleChange]);

  if (args.activeTab.kind === "browser") {
    return <div className="vk-panel-body" data-fill="">{args.props.aiBrowserPanel}</div>;
  }

  if (args.activeTab.kind === "viewer") {
    return <div className="vk-panel-body" data-fill="">
      {args.activeTab.viewerPath ? <React.Suspense fallback={null}>
        <LazyFileViewerTab key={args.activeTab.viewerPath} onUndock={() => args.props.onCloseContextTab(activeTabId)} path={args.activeTab.viewerPath} />
      </React.Suspense> : null}
    </div>;
  }

  if (args.activeTab.kind === "activity") {
    return <div className="vk-panel-body" data-fill="">
      <React.Suspense fallback={null}>
        <LazySessionActivityPanel
          page={args.activeTab.activityPage ?? "subagents"}
          pageRequest={args.activeTab.activityRequest ?? 0}
          sessionId={args.props.selectedConversationId}
          subagentRevision={subagentNavigationRevision(args.props.workbench, args.props.selectedConversationId)}
          onOpenSubagent={args.props.onOpenSubagentPanel}
        />
      </React.Suspense>
    </div>;
  }

  if (args.activeTab.kind === "local-browser") {
    return <div className="vk-panel-body" data-fill="">
      {args.props.selectedConversationId ? <React.Suspense fallback={null}>
        <LazyClientBrowserArea key={args.props.selectedConversationId} ownerSessionId={args.props.selectedConversationId} />
      </React.Suspense> : null}
    </div>;
  }

  if (args.activeTab.kind === "review") {
    return (
      <div className="vk-panel-body" data-fill="">
        <React.Suspense fallback={<WorkspaceReviewPanelLoading />}>
          <LazyWorkspaceReviewPanelHost
            loadWorkspaceDiffFileDetail={args.props.loadWorkspaceDiffFileDetail}
            loadWorkspaceDiffSearch={args.props.loadWorkspaceDiffSearch}
            loadWorkspaceDiffSnapshot={args.props.loadWorkspaceDiffSnapshot}
            runWorkspaceGitFileAction={args.props.runWorkspaceGitFileAction}
            onWorkspaceDiffSnapshotLoaded={args.props.onWorkspaceDiffSnapshotLoaded}
            onWorkspaceGitStateChanged={args.props.onWorkspaceGitStateChanged}
            onSelectGitRepository={args.props.onSelectGitRepository}
            gitCommitActionBusy={args.props.gitCommitActionBusy}
            gitCommitActionDisabled={args.props.gitCommitActionDisabled}
            gitCommitActionLabel={args.props.gitCommitActionLabel}
            reviewFocusRequest={args.props.reviewFocusRequest}
            selectedConversationId={args.props.selectedConversationId}
            selectedDraftConversation={args.props.selectedDraftConversation}
            workbench={args.props.workbench}
            onCopyRawDiff={args.props.onCopyRawDiff}
            onOpenGitCommitAction={args.props.onOpenGitCommitAction}
          />
        </React.Suspense>
      </div>
    );
  }
  if (args.activeTab.kind === "memory") {
    return (
      <div className="vk-panel-body" data-fill="">
        <WorkspaceMemoryPanel
          selectedConversationId={args.props.selectedConversationId}
          selectedProjectId={selectedContextProjectId(args.props)}
          snapshot={args.props.workbench.contextInjection ?? null}
          onRunVmmAction={args.props.onRunVmmAction}
          onOpenSettings={args.props.onOpenMemorySettings}
          memoryState={args.props.workbench.vmmMemory?.experienceState ?? null}
        />
      </div>
    );
  }
  if (args.activeTab.kind === "subagent") {
    return (
      <div className="vk-panel-body" data-fill="">
        <WorkspaceSubagentPanel
          key={args.activeTab.id}
          tab={args.activeTab}
          props={args.props}
          expandedStates={args.subagentExpandedStates}
          scrollSnapshots={args.subagentScrollSnapshots}
        />
      </div>
    );
  }
  return (
    <div className="vk-panel-body" data-fill="">
      <React.Suspense fallback={<WorkspaceFilesPanelLoading />}>
        <LazyWorkspaceFilesPanelHost
          instanceId={args.activeTab.id}
          key={args.activeTab.id}
          loadWorkspaceFileDirectory={args.props.loadWorkspaceFileDirectory}
          loadWorkspaceFilePreview={args.props.loadWorkspaceFilePreview}
          resolveMediaSourceUrl={args.props.resolveMediaSourceUrl}
          selectedConversationId={args.props.selectedConversationId}
          selectedDraftConversation={args.props.selectedDraftConversation}
          workbench={args.props.workbench}
          onCopyText={args.props.onCopyText}
          onTitleChange={handleFileTabTitleChange}
        />
      </React.Suspense>
    </div>
  );
}

// WorkspaceSubagentPanel renders a read-only subagent transcript inside the right context rail.
// WorkspaceSubagentPanel 在右侧上下文栏中渲染只读子代理转录。
// Parameters: args supplies the tab-owned task and retained transcript view state. Returns transcript and lifecycle controls.
// 参数：args 提供页签持有的任务及保留的转录视图状态。返回转录和生命周期控件。
function WorkspaceSubagentPanel(args: {
  // Active subagent tab descriptor.
  // 当前活动的子代理页签描述。
  tab: WorkspaceContextTabItem;
  // Parent context panel props.
  // 父级上下文面板属性。
  props: WorkspaceContextPanelProps;
  // Semantic viewport state retained across active context-tab unmounts.
  // 跨活动上下文页签卸载保留的语义视口状态。
  scrollSnapshots: React.MutableRefObject<Map<string, ConversationScrollSnapshot>>;
  // Expansion state retained across active context-tab unmounts.
  // 跨活动上下文页签卸载保留的展开状态。
  expandedStates: React.MutableRefObject<Map<string, ConversationSurfaceExpandedState>>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // stopConfirmOpen stores the destructive stop confirmation dialog state.
  // stopConfirmOpen 保存危险停止确认弹窗状态。
  const [stopConfirmOpen, setStopConfirmOpen] = React.useState(false);
  // stopSubmitting stores whether the stop request is currently in flight.
  // stopSubmitting 保存停止请求是否正在提交。
  const [stopSubmitting, setStopSubmitting] = React.useState(false);
  // closeConfirmOpen stores the lifecycle close confirmation dialog state.
  // closeConfirmOpen 保存生命周期关闭确认弹窗状态。
  const [closeConfirmOpen, setCloseConfirmOpen] = React.useState(false);
  // closeSubmitting stores whether the close request is currently in flight.
  // closeSubmitting 保存关闭请求是否正在提交。
  const [closeSubmitting, setCloseSubmitting] = React.useState(false);
  // taskMutation blocks duplicate and competing lifecycle requests before React commits disabled state.
  // taskMutation 在 React 提交禁用状态前阻止重复及相互冲突的生命周期请求。
  const taskMutation = React.useRef(false);
  // taskActionError retains the original failure inside the active confirmation.
  // taskActionError 在当前确认弹窗内保留原始失败原因。
  const [taskActionError, setTaskActionError] = React.useState<string | null>(null);
  // scrollRef owns the read-only transcript viewport inside this tab.
  // scrollRef 持有该页签内只读转录视口。
  const scrollRef = React.useRef<HTMLElement | null>(null);
  // tailLocked controls whether new subagent transcript content follows the viewport tail.
  // tailLocked 控制新增子代理转录内容是否持续跟随视口尾部。
  const [tailLocked, setTailLocked] = React.useState(true);
  // parentConversationId stores the task owner required by the stop API.
  // parentConversationId 保存停止接口所需的任务所属父会话。
  const parentConversationId = args.tab.subagentParentConversationId ?? null;
  // taskId stores the subagent child conversation id represented by this tab.
  // taskId 保存该页签代表的子代理子会话标识。
  const taskId = args.tab.subagentTaskId ?? null;
  // A child transcript uses its own display choice, never its parent's session override.
  // 子转录使用自己的展示选择，绝不使用父会话的会话覆盖值。
  const thinkingPreference = useThinkingDisplayPreference(taskId, args.props.workbench.thinkingDisplayMode);
  // navigation stores the matching backend subagent projection for this parent conversation.
  // navigation 保存该父会话匹配的后端子代理投影。
  const navigation = parentConversationId && args.props.workbench.subagentNavigation?.parentConversationId === parentConversationId
    ? args.props.workbench.subagentNavigation
    : null;
  // task stores the current backend task row when it is still present in the snapshot.
  // task 保存快照中仍存在时的当前后端任务行。
  const task = taskId ? navigation?.tasks?.find((candidate) => candidate.taskId === taskId) ?? null : null;
  // Retained root addons keep this child accurate when another main conversation is selected.
  // 保留的根附属状态使当前子会话在选中其他主会话时仍保持准确。
  const rootUsage = args.props.workbench.sessionAddonsByConversation
    ?.find((entry) => entry.conversationId === parentConversationId)?.sessionProviderUsage?.lifetime;
  // taskRunning records whether the backend still allows stopping this task.
  // taskRunning 记录后端是否仍允许停止该任务。
  const taskRunning = Boolean(task?.isRunning && parentConversationId && taskId);
  // taskOpen is the sole authority for retaining a live child handle and event subscription.
  // taskOpen 是保留实时子会话句柄与事件订阅的唯一权威。
  const taskOpen = Boolean(task && task.runState !== "closed" && parentConversationId && taskId);
  // transientReasoningStatus subscribes only while this exact child remains live.
  // transientReasoningStatus 仅在当前精确子会话仍实时存在时订阅。
  const transientReasoningStatus = useConversationTransientReasoningStatus(taskOpen ? taskId : null);
  // providerRetryStatus subscribes to the child's retry wait under the same liveness rule.
  // providerRetryStatus 在相同存活规则下订阅子会话的重试等待。
  const providerRetryStatus = useConversationProviderRetryStatus(taskOpen ? taskId : null);
  // transientReasoningStatusVisible tracks insertion and removal while shared resize handling owns node reflow.
  // transientReasoningStatusVisible 跟踪插入与移除，共享尺寸处理负责节点重排。
  const transientReasoningStatusVisible = transientReasoningStatus !== null || providerRetryStatus !== null;
  // subagentConversation supplies only the visual summary fields required by the restored branch shell.
  // subagentConversation 仅提供恢复后的分支界面所需的视觉摘要字段。
  const subagentConversation = React.useMemo(() => ({
    id: taskId ?? "",
    title: task?.label ?? args.tab.title,
    status: task?.runState ?? "unknown",
    runPhase: task?.runState,
  }), [args.tab.title, task?.label, task?.runState, taskId]);
  // subagentCommandSessions keeps only live terminal sessions owned by this exact child session.
  // subagentCommandSessions 仅保留由当前精确子会话拥有的实时终端会话。
  const subagentCommandSessions = React.useMemo(
    () => (args.props.workbench.commandSessions ?? []).filter((session) => session.ownerSessionId === taskId),
    [args.props.workbench.commandSessions, taskId],
  );
  // workspacePath stores the parent project path used for Markdown media resolution.
  // workspacePath 保存用于 Markdown 媒体解析的父项目路径。
  const workspacePath = subagentWorkspacePath(args.props.workbench, parentConversationId);
  // transcriptOwner owns the exact child V1 window independently from the selected root session.
  // transcriptOwner 独立于当前选中根会话持有精确子会话 V1 窗口。
  const transcriptOwner = useConversationSessionEventWindowOwnerV1({
    live: taskOpen,
    parentSessionId: parentConversationId,
    sessionId: taskId,
  });
  // handleStopConfirmed executes the single-task stop after confirmation.
  // handleStopConfirmed 在确认后执行单任务停止。
  // Parameters: none; the mounted tab owns the parent/task ids. Returns after the request or visible failure handling finishes.
  // 参数：无；挂载页签持有父会话/任务标识。请求或可见失败处理结束后返回。
  const handleStopConfirmed = React.useCallback(async () => {
    if (!parentConversationId || !taskId || taskMutation.current) {
      return;
    }
    taskMutation.current = true;
    setTaskActionError(null);
    setStopSubmitting(true);
    try {
      await args.props.onStopSubagentTask(parentConversationId, taskId);
      setStopConfirmOpen(false);
    } catch (error) {
      setTaskActionError(error instanceof Error ? error.message : String(error));
    } finally {
      taskMutation.current = false;
      setStopSubmitting(false);
    }
  }, [args.props, parentConversationId, stopSubmitting, taskId]);
  // handleCloseConfirmed closes the child-agent lifecycle after confirmation.
  // handleCloseConfirmed 在确认后关闭子智能体生命周期。
  // Parameters: none; the mounted tab owns the parent/task ids. Returns after success closes the dialog or failure preserves it.
  // 参数：无；挂载页签持有父会话/任务标识。成功关闭弹窗或失败保留弹窗后返回。
  const handleCloseConfirmed = React.useCallback(async () => {
    if (!parentConversationId || !taskId || taskMutation.current) {
      return;
    }
    taskMutation.current = true;
    setTaskActionError(null);
    setCloseSubmitting(true);
    try {
      await args.props.onCloseSubagentTask(parentConversationId, taskId);
      setCloseConfirmOpen(false);
    } catch (error) {
      setTaskActionError(error instanceof Error ? error.message : String(error));
    } finally {
      taskMutation.current = false;
      setCloseSubmitting(false);
    }
  }, [args.props, closeSubmitting, parentConversationId, taskId]);
  // updateTailLock publishes the child controller's explicit tail ownership state.
  // updateTailLock 发布子界面控制器的显式尾部所有权状态。
  const updateTailLock = React.useCallback((locked: boolean) => {
    setTailLocked(locked);
  }, []);
  // readSubagentScrollSnapshot returns this exact mounted child surface's semantic state.
  // readSubagentScrollSnapshot 返回当前精确已挂载子界面的语义状态。
  const readSubagentScrollSnapshot = React.useCallback((surfaceId: string) => (
    surfaceId === taskId ? args.scrollSnapshots.current.get(surfaceId) ?? null : null
  ), [args.scrollSnapshots, taskId]);
  // writeSubagentScrollSnapshot stores semantic state only for the exact mounted child surface.
  // writeSubagentScrollSnapshot 仅为当前精确已挂载子界面保存语义状态。
  const writeSubagentScrollSnapshot = React.useCallback((
    surfaceId: string,
    snapshot: ConversationScrollSnapshot,
  ) => {
    if (surfaceId === taskId) {
      args.scrollSnapshots.current.set(surfaceId, snapshot);
    }
  }, [args.scrollSnapshots, taskId]);
  // writeSubagentExpandedState retains branch process and card expansion choices for this child surface only.
  // writeSubagentExpandedState 仅为当前子界面保留分支处理过程与卡片展开选择。
  const writeSubagentExpandedState = React.useCallback((
    surfaceId: string,
    state: ConversationSurfaceExpandedState,
  ): void => {
    if (surfaceId === taskId) {
      args.expandedStates.current.set(surfaceId, state);
    }
  }, [args.expandedStates, taskId]);
  // subagentScrollController is the sole owner of child-transcript scrolling and geometry restoration.
  // subagentScrollController 是子转录滚动与几何恢复的唯一所有者。
  const subagentScrollController = useConversationScrollController({
    active: Boolean(taskId),
    onFollowTailChange: updateTailLock,
    readSnapshot: readSubagentScrollSnapshot,
    scrollRef,
    surfaceId: taskId,
    writeSnapshot: writeSubagentScrollSnapshot,
  });
  // Adding or removing child transient reasoning follows the child transcript's semantic scroll ownership.
  // 增删子智能体临时思考状态时遵循子转录自身的语义滚动所有权。
  React.useLayoutEffect(() => {
    subagentScrollController.scheduleStoredRestore();
  }, [subagentScrollController, transientReasoningStatusVisible]);
  // transcriptHasEvents distinguishes a full empty-state failure from a non-blocking refresh warning.
  // transcriptHasEvents 区分完整空状态失败与不阻塞内容的刷新警告。
  const transcriptHasEvents = transcriptOwner.snapshot.events.length > 0;
  if (!parentConversationId || !taskId) {
    return <WorkspaceSubagentEmptyState title={workspaceContextTabDefaultTitle("subagent", t)} />;
  }

  return (
    <div className="workspace-subagent-panel vk-subchat">
      <ChildLifetimeUsageChip bucket={rootUsage?.rootSessionId === parentConversationId
        ? rootUsage.childrenBySessionId[taskId]?.usage : undefined} />
      {transcriptOwner.error && transcriptHasEvents ? (
        <div className="workspace-subagent-error-inline" role="alert">
          <Bot size={16} />
          <span>{transcriptOwner.error}</span>
        </div>
      ) : null}
      {task?.pendingMessages?.length ? (
        <div className="workspace-subagent-message-queue" aria-live="polite">
          {task.pendingMessages.map((message) => (
            <div className="workspace-subagent-message-queue-item" key={message.deliveryId}>
              <strong>{t(message.steering ? "conversation.subagent.queue.steering" : "conversation.subagent.queue.followup")}</strong>
              <span>{message.message}</span>
            </div>
          ))}
        </div>
      ) : null}
      <ConversationBranchWorkspaceV1
        providers={args.props.workbench.providers}
        commandSessions={subagentCommandSessions}
        commands={transcriptOwner.commands}
        conversation={subagentConversation}
        initialExpandedState={args.expandedStates.current.get(taskId) ?? null}
        onExpandedStateChange={writeSubagentExpandedState}
        onOpenSubagentPanel={args.props.onOpenSubagentPanel}
        reasoningDisplayMode={thinkingPreference.mode}
        scrollRef={scrollRef}
        sessionId={taskId}
        sessionRunning={Boolean(task?.isRunning)}
        snapshot={transcriptOwner.snapshot}
        suppressEmptyPlaceholder={transcriptOwner.loading}
        transientReasoningStatus={providerRetryStatus ? (
          <ProviderRetryCountdownLine countdown={providerRetryStatus} />
        ) : transientReasoningStatus ? (
          <TransientReasoningStatusLine status={transientReasoningStatus} />
        ) : null}
        workspacePath={workspacePath}
        onCopyText={args.props.onCopyText}
        onPreserveHistoryPrepend={subagentScrollController.preserveHistoryPrepend}
        onScrollStateChange={subagentScrollController.handleScroll}
      />
      {transcriptOwner.loading && transcriptOwner.snapshot.events.length === 0 ? (
        <StreamPlaceholder className="vk-stream-overlay" fill label={t("conversation.subagent.loadingTranscript")} />
      ) : null}
      {transcriptOwner.error && !transcriptHasEvents ? (
        <div className="workspace-subagent-loading" role="alert">
          <Bot size={18} />
          <span>{transcriptOwner.error}</span>
        </div>
      ) : null}
      {!tailLocked ? (
        <Button
          aria-label={t("composer.scrollToBottom")}
          className={cx("workspace-subagent-scroll-bottom", taskRunning || taskOpen ? "workspace-subagent-scroll-bottom-with-footer" : null)}
          onClick={() => subagentScrollController.scrollToBottom()}
          size="icon"
          title={t("composer.scrollToBottom")}
          type="button"
          variant="outline"
        >
          <ArrowDown size={18} />
        </Button>
      ) : null}
      {taskRunning || taskOpen ? (
        <div className="workspace-subagent-footer">
          {taskRunning ? (
            <Button
              className="workspace-subagent-stop-button"
              disabled={closeSubmitting}
              onClick={() => { setTaskActionError(null); setStopConfirmOpen(true); }}
              type="button"
              variant="outline"
            >
              <Square fill="currentColor" size={12} strokeWidth={0} />
              <span>{t("conversation.subagent.stopTask")}</span>
            </Button>
          ) : null}
          {taskOpen ? (
            <Button
              className="workspace-subagent-stop-button"
              disabled={stopSubmitting}
              onClick={() => { setTaskActionError(null); setCloseConfirmOpen(true); }}
              type="button"
              variant="outline"
            >
              <X size={13} />
              <span>{t("conversation.subagent.closeTask")}</span>
            </Button>
          ) : null}
        </div>
      ) : null}
      <Dialog open={stopConfirmOpen} onOpenChange={(open) => !stopSubmitting && setStopConfirmOpen(open)}>
        <DialogContent showCloseButton={!stopSubmitting} size="sm">
          <DialogHeader>
            <DialogTitle>{t("conversation.subagent.stopConfirmTitle")}</DialogTitle>
            <DialogDescription>
              {t("conversation.subagent.stopConfirmDescription").replace("{task}", task?.label ?? args.tab.title)}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            {taskActionError ? <p role="alert" className="vk-field-error">{taskActionError}</p> : null}
            <Button disabled={stopSubmitting} onClick={() => setStopConfirmOpen(false)} type="button" variant="outline">
              {t("conversation.action.cancel")}
            </Button>
            <Button disabled={stopSubmitting} onClick={() => void handleStopConfirmed()} type="button" variant="destructive">
              <Square fill="currentColor" size={12} strokeWidth={0} />
              <span>{stopSubmitting ? t("conversation.subagent.stopping") : t("conversation.subagent.stopConfirm")}</span>
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={closeConfirmOpen} onOpenChange={(open) => !closeSubmitting && setCloseConfirmOpen(open)}>
        <DialogContent showCloseButton={!closeSubmitting} size="sm">
          <DialogHeader>
            <DialogTitle>{t("conversation.subagent.closeConfirmTitle")}</DialogTitle>
            <DialogDescription>
              {t("conversation.subagent.closeConfirmDescription").replace("{task}", task?.label ?? args.tab.title)}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            {taskActionError ? <p role="alert" className="vk-field-error">{taskActionError}</p> : null}
            <Button disabled={closeSubmitting} onClick={() => setCloseConfirmOpen(false)} type="button" variant="outline">
              {t("conversation.action.cancel")}
            </Button>
            <Button disabled={closeSubmitting} onClick={() => void handleCloseConfirmed()} type="button" variant="destructive">
              <X size={13} />
              <span>{closeSubmitting ? t("conversation.subagent.closing") : t("conversation.subagent.closeConfirm")}</span>
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// WorkspaceSubagentEmptyState renders a compact fallback for incomplete subagent tab descriptors.
// WorkspaceSubagentEmptyState 为不完整的子代理页签描述渲染紧凑回退状态。
function WorkspaceSubagentEmptyState(props: {
  // Visible fallback title.
  // 可见回退标题。
  title: string;
}) {
  return (
    <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon"><Bot /></EmptyMedia>
        <EmptyTitle>{props.title}</EmptyTitle>
      </EmptyHeader>
    </Empty>
  );
}

// workspaceContextTabIcon returns the icon used for one tab kind.
// workspaceContextTabIcon 返回单个页签类型使用的图标。
function workspaceContextTabIcon(kind: WorkspaceContextTabKind, size: number): React.ReactNode {
  if (kind === "browser" || kind === "local-browser") {
    return <Globe2 size={size} />;
  }
  if (kind === "subagent") {
    return <Bot size={size} />;
  }
  if (kind === "files") {
    return <Folder size={size} />;
  }
  if (kind === "viewer") {
    return <FileText size={size} />;
  }
  if (kind === "memory") {
    return <Brain size={size} />;
  }
  if (kind === "activity") {
    return <ListTree size={size} />;
  }
  return <GitPullRequest size={size} />;
}

// subagentNavigationRevision summarizes the subagent rows of one parent so the activity panel reloads when they change.
// subagentNavigationRevision 概括一个父会话的子代理行，使活动面板在其变化时重新加载。
function subagentNavigationRevision(workbench: WorkbenchSnapshot, parentConversationId: string | null): string {
  const navigation = workbench.subagentNavigation;
  if (!navigation || navigation.parentConversationId !== parentConversationId) return "";
  return (navigation.tasks ?? []).map((task) => `${task.taskId}:${task.runState}`).join("|");
}

// workspaceContextTabDisplayTitle returns the locale-aware visible title for one opened tab.
// workspaceContextTabDisplayTitle 返回单个已打开页签按当前语言显示的标题。
function workspaceContextTabDisplayTitle(tab: WorkspaceContextTabItem, t: (key: TranslationKey) => string): string {
  if (tab.kind === "files" || tab.kind === "subagent" || tab.kind === "viewer") {
    return tab.title || workspaceContextTabDefaultTitle(tab.kind, t);
  }
  return workspaceContextTabDefaultTitle(tab.kind, t);
}

// selectedContextProjectId resolves the current project id for project-scoped VMM actions.
// selectedContextProjectId 解析当前用于项目级 VMM 动作的项目标识。
function selectedContextProjectId(props: WorkspaceContextPanelProps): string | null {
  if (props.selectedDraftConversation?.projectId) {
    return props.selectedDraftConversation.projectId;
  }
  const selectedConversation = props.workbench.conversations.find(
    (conversation) => conversation.id === props.selectedConversationId,
  );
  return selectedConversation?.projectId ?? null;
}

// subagentWorkspacePath resolves the parent conversation project path for side-panel transcript media.
// subagentWorkspacePath 为侧栏转录媒体解析父会话所属项目路径。
function subagentWorkspacePath(workbench: WorkbenchSnapshot, parentConversationId: string | null): string | null {
  if (!parentConversationId) {
    return null;
  }
  const parentConversation = workbench.conversations.find((conversation) => conversation.id === parentConversationId);
  if (!parentConversation?.projectId) {
    return null;
  }
  return workbench.projects.find((project) => project.id === parentConversation.projectId)?.path ?? null;
}

// WorkspaceReviewPanelLoading renders while the review panel chunk is loading.
// WorkspaceReviewPanelLoading 渲染审核面板 chunk 加载期间的轻量状态。
function WorkspaceReviewPanelLoading() {
  const { t } = useI18n();
  return (
    <StreamPlaceholder className="vk-stream-inset" fill label={t("workspaceContext.loadingReviewPanel")} shape="rows" />
  );
}

// WorkspaceFilesPanelLoading renders while the file panel chunk is loading.
// WorkspaceFilesPanelLoading 渲染文件面板 chunk 加载期间的轻量状态。
function WorkspaceFilesPanelLoading() {
  const { t } = useI18n();
  return (
    <StreamPlaceholder className="vk-stream-inset" fill label={t("workspaceContext.loadingFilesPanel")} shape="rows" />
  );
}
