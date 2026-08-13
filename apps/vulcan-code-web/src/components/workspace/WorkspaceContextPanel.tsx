import React from "react";
import {
  ArrowDown,
  Bot,
  Brain,
  ChevronsDown,
  Folder,
  GitPullRequest,
  Plus,
  Search,
  Square,
  X,
} from "lucide-react";
import { composerAttachmentMedia } from "@/components/composer/composer-attachment-media";
import type { ConversationScrollSnapshot } from "@/components/conversation/chat-area-scroll";
import { useConversationScrollController } from "@/components/conversation/chat-area/conversation-scroll-controller";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ConversationWorkspace } from "@/components/conversation/conversation-workspace";
import {
  generatedMediaAttachmentsFromToolDetailPayload,
  projectConversationRenderItems,
} from "@/components/conversation/conversation-transcript";
import type {
  ConversationSurfaceExpandedState,
} from "@/components/conversation/conversation-layout-restore";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
import { cn } from "@/lib/utils";
import { useI18n, type TranslationKey } from "@/i18n";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";
import type {
  ConversationMessage,
  ConversationSummary,
  VmmMemoryActionRequest,
  VmmMemoryActionResult,
  WorkbenchSnapshot,
  WorkspaceFileDirectoryListing,
  WorkspaceFileFilterOptions,
  WorkspaceFileReadResponse,
} from "@/types";
import { WorkspaceMemoryPanel } from "@/components/workspace/WorkspaceMemoryPanel";

// LazyWorkspaceReviewPanelHost loads the heavy Git review surface only when the context rail renders it.
// LazyWorkspaceReviewPanelHost 仅在上下文栏渲染审核页时加载较重的 Git 审核界面。
const LazyWorkspaceReviewPanelHost = React.lazy(async () => {
  const module = await import("@/components/workspace/WorkspaceReviewPanelHost");
  return { default: module.WorkspaceReviewPanelHost };
});

// LazyWorkspaceFilesPanelHost loads the filesystem browser only when the context rail renders it.
// LazyWorkspaceFilesPanelHost 仅在上下文栏渲染文件页时加载文件系统浏览器。
const LazyWorkspaceFilesPanelHost = React.lazy(async () => {
  const module = await import("@/components/workspace/WorkspaceFilesPanelHost");
  return { default: module.WorkspaceFilesPanelHost };
});

// WorkspaceContextPanelTab identifies one registered right context panel page.
// WorkspaceContextPanelTab 标识一个已注册的右侧上下文面板页面。
export type WorkspaceContextPanelTab = "review" | "memory" | "files" | "subagent";

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
  // Optional subagent child task conversation id for subagent tabs.
  // 子代理页签可选的子任务会话标识。
  subagentTaskId?: string | null;
  // Optional parent conversation id that owns the subagent task.
  // 拥有子代理任务的可选父会话标识。
  subagentParentConversationId?: string | null;
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

// WorkspaceConversationBlock is the minimal conversation block shape needed by diff extraction.
// WorkspaceConversationBlock 是 diff 提取所需的最小会话块结构。
export interface WorkspaceConversationBlock {
  // Block kind projected from a transcript message.
  // 从转录消息投影出的块类型。
  kind: string;
  // Block body text.
  // 块正文文本。
  body: string;
}

// WorkspaceContextPanelProps describes the right-side workspace context panel boundary.
// WorkspaceContextPanelProps 描述右侧工作区上下文面板边界。
export interface WorkspaceContextPanelProps {
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
  onCopyRawDiff: (rawDiff: string) => void;
  // Copies plain text using the host-aware clipboard implementation.
  // 使用宿主感知的剪贴板实现复制普通文本。
  onCopyText: (text: string) => void;
  // Resolves one local media path into a WebUI-renderable URL.
  // 将单个本地媒体路径解析为 WebUI 可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Runs one VMM memory management action.
  // 执行单个 VMM 记忆管理动作。
  onRunVmmAction: (request: VmmMemoryActionRequest) => Promise<VmmMemoryActionResult>;
  // Parses one conversation message into render blocks.
  // 将单条会话消息解析为渲染块。
  parseConversationBlocks: (message: ConversationMessage) => WorkspaceConversationBlock[];
  // Opens one subagent transcript in the right context panel.
  // 在右侧上下文面板中打开单个子代理转录。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Stops one running subagent task.
  // 停止单个正在运行的子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Closes one open subagent lifecycle.
  // 关闭单个未关闭子智能体生命周期。
  onCloseSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Loads one hidden tool detail record for transcript rows.
  // 为转录行加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
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
export function WorkspaceContextPanel(props: WorkspaceContextPanelProps) {
  // activeTab stores the currently visible opened tab.
  // activeTab 保存当前可见的已打开页签。
  const activeTab = React.useMemo(
    () => props.tabs.find((tab) => tab.id === props.activeTabId) ?? props.tabs[0] ?? null,
    [props.activeTabId, props.tabs],
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
    <aside className={cn("context-panel-shell", props.floating && "floating")}>
      {activeTab ? (
        <div className="context-dynamic-workspace">
          <WorkspaceContextTabBar
            activeTabId={activeTab.id}
            tabs={props.tabs}
            tabRefs={tabRefs}
            onActivateTab={props.onActivateContextTab}
            onCloseTab={(tab) => {
              if (tab.kind === "files") {
                void import("@/components/workspace/WorkspaceFilesPanelHost")
                  .then((module) => module.forgetWorkspaceFilesPanelCacheInstance(tab.id));
              }
              if (tab.kind === "subagent" && tab.subagentTaskId) {
                subagentScrollSnapshots.current.delete(tab.subagentTaskId);
                subagentExpandedStates.current.delete(tab.subagentTaskId);
              }
              props.onCloseContextTab(tab.id);
            }}
            onOpenTab={props.onOpenContextTab}
          />
          <WorkspaceContextTabContent
            activeTab={activeTab}
            props={props}
            subagentExpandedStates={subagentExpandedStates}
            subagentScrollSnapshots={subagentScrollSnapshots}
          />
        </div>
      ) : (
        <WorkspaceContextLauncher onOpenTab={props.onOpenContextTab} />
      )}
    </aside>
  );
}

// WorkspaceContextTabBar renders opened tabs plus switcher and add menus.
// WorkspaceContextTabBar 渲染已打开页签以及切换与新增菜单。
function WorkspaceContextTabBar(props: {
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
  return (
    <div className="context-dynamic-tabbar">
      {props.tabs.length > 1 ? (
        <WorkspaceContextTabSwitcher
          activeTabId={props.activeTabId}
          tabs={props.tabs}
          onActivateTab={props.onActivateTab}
        />
      ) : null}
      <WorkspaceContextAddMenu onOpenTab={props.onOpenTab} />
      <div className="context-dynamic-tab-strip" role="tablist">
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
    <div
      className={cn("context-dynamic-tab", props.active && "active")}
      ref={props.tabRef}
      role="presentation"
    >
      <span className="context-dynamic-tab-icon">{workspaceContextTabIcon(props.tab.kind, 14)}</span>
      <button
        aria-label={t("workspaceContext.closeTab").replace("{title}", displayTitle)}
        className="context-dynamic-tab-close"
        onClick={(event) => {
          event.stopPropagation();
          props.onCloseTab(props.tab);
        }}
        type="button"
      >
        <X size={13} />
      </button>
      <button
        aria-selected={props.active}
        className="context-dynamic-tab-label"
        onClick={() => props.onActivateTab(props.tab.id)}
        role="tab"
        title={displayTitle}
        type="button"
      >
        <span>{displayTitle}</span>
      </button>
    </div>
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
        <button aria-label={t("workspaceContext.switchTabs")} className="context-dynamic-icon-button" type="button">
          <ChevronsDown size={15} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="context-dynamic-tabs-menu">
        <label className="context-dynamic-tabs-menu-search">
          <Search size={13} />
          <input
            aria-label={t("workspaceContext.searchTabs")}
            onChange={(event) => setQuery(event.currentTarget.value)}
            onKeyDown={(event) => event.stopPropagation()}
            placeholder={t("workspaceContext.searchTabsPlaceholder")}
            value={query}
          />
          {query ? (
            <button
              aria-label={t("workspaceContext.clearTabSearch")}
              className="context-dynamic-tabs-menu-search-clear"
              onClick={() => setQuery("")}
              type="button"
            >
              <X size={12} />
            </button>
          ) : null}
        </label>
        <DropdownMenuLabel>{t("workspaceContext.openTabs")}</DropdownMenuLabel>
        {visibleTabs.map((tab) => {
          const displayTitle = workspaceContextTabDisplayTitle(tab, t);
          return (
            <DropdownMenuItem
              className={cn("context-dynamic-tabs-menu-item", tab.id === props.activeTabId && "active")}
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
  // Opens one tab kind.
  // 打开单个页签类型。
  onOpenTab: (kind: WorkspaceContextTabKind) => void;
}) {
  const { t } = useI18n();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button aria-label={t("workspaceContext.openRightPage")} className="context-dynamic-icon-button add" type="button">
          <Plus size={15} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="context-dynamic-add-menu">
        <DropdownMenuLabel>{t("workspaceContext.openPage")}</DropdownMenuLabel>
        <WorkspaceContextOpenMenuItem kind="files" onOpenTab={props.onOpenTab} />
        <DropdownMenuSeparator />
        <WorkspaceContextOpenMenuItem kind="review" onOpenTab={props.onOpenTab} />
        <WorkspaceContextOpenMenuItem kind="memory" onOpenTab={props.onOpenTab} />
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
  // Opens one tab kind.
  // 打开单个页签类型。
  onOpenTab: (kind: WorkspaceContextTabKind) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="context-launcher">
      <WorkspaceContextLauncherCard
        description={t("workspaceContext.launcher.filesDescription")}
        kind="files"
        shortcut="Ctrl+P"
        title={t("contextPanel.files")}
        onOpenTab={props.onOpenTab}
      />
      <WorkspaceContextLauncherCard
        description={t("workspaceContext.launcher.reviewDescription")}
        kind="review"
        shortcut="Ctrl+Shift+G"
        title={t("contextPanel.review")}
        onOpenTab={props.onOpenTab}
      />
      <WorkspaceContextLauncherCard
        description={t("workspaceContext.launcher.memoryDescription")}
        kind="memory"
        title={t("contextPanel.memory")}
        onOpenTab={props.onOpenTab}
      />
    </div>
  );
}

// WorkspaceContextLauncherCard renders one empty-panel action card.
// WorkspaceContextLauncherCard 渲染一个空面板操作卡片。
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
  // Optional shortcut hint.
  // 可选快捷键提示。
  shortcut?: string;
  // Opens one tab kind.
  // 打开单个页签类型。
  onOpenTab: (kind: WorkspaceContextTabKind) => void;
}) {
  return (
    <button className="context-launcher-card" onClick={() => props.onOpenTab(props.kind)} type="button">
      {workspaceContextTabIcon(props.kind, 22)}
      <strong>{props.title}</strong>
      <span>{props.description}</span>
      {props.shortcut ? <kbd>{props.shortcut}</kbd> : null}
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

  if (args.activeTab.kind === "review") {
    return (
      <div className="context-tab-content review-tab-content">
        <React.Suspense fallback={<WorkspaceReviewPanelLoading />}>
          <LazyWorkspaceReviewPanelHost
            loadWorkspaceDiffFileDetail={args.props.loadWorkspaceDiffFileDetail}
            loadWorkspaceDiffSearch={args.props.loadWorkspaceDiffSearch}
            loadWorkspaceDiffSnapshot={args.props.loadWorkspaceDiffSnapshot}
            parseConversationBlocks={args.props.parseConversationBlocks}
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
      <div className="context-tab-content memory-tab-content">
        <WorkspaceMemoryPanel
          selectedConversationId={args.props.selectedConversationId}
          selectedProjectId={selectedContextProjectId(args.props)}
          snapshot={args.props.workbench.contextInjection ?? null}
          onRunVmmAction={args.props.onRunVmmAction}
        />
      </div>
    );
  }
  if (args.activeTab.kind === "subagent") {
    return (
      <div className="context-tab-content subagent-tab-content">
        <WorkspaceSubagentPanel
          tab={args.activeTab}
          props={args.props}
          expandedStates={args.subagentExpandedStates}
          scrollSnapshots={args.subagentScrollSnapshots}
        />
      </div>
    );
  }
  return (
    <div className="context-tab-content files-tab-content">
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
  // transcriptLoading stores whether an empty child transcript is currently being restored.
  // transcriptLoading 保存空子转录当前是否正在恢复。
  const [transcriptLoading, setTranscriptLoading] = React.useState(false);
  // scrollRef owns the read-only transcript viewport inside this tab.
  // scrollRef 持有该页签内只读转录视口。
  const scrollRef = React.useRef<HTMLElement | null>(null);
  // tailLocked controls whether new subagent transcript content follows the viewport tail.
  // tailLocked 控制新增子代理转录内容是否持续跟随视口尾部。
  const [tailLocked, setTailLocked] = React.useState(true);
  // hiddenThinkingBlockIds leaves subagent thinking rows in the inline transcript.
  // hiddenThinkingBlockIds 让子代理思考行保留在内联转录中。
  const hiddenThinkingBlockIds = React.useMemo(() => new Set<string>(), []);
  // parentConversationId stores the task owner required by the stop API.
  // parentConversationId 保存停止接口所需的任务所属父会话。
  const parentConversationId = args.tab.subagentParentConversationId ?? null;
  // taskId stores the subagent child conversation id represented by this tab.
  // taskId 保存该页签代表的子代理子会话标识。
  const taskId = args.tab.subagentTaskId ?? null;
  // navigation stores the matching backend subagent projection for this parent conversation.
  // navigation 保存该父会话匹配的后端子代理投影。
  const navigation = parentConversationId && args.props.workbench.subagentNavigation?.parentConversationId === parentConversationId
    ? args.props.workbench.subagentNavigation
    : null;
  // task stores the current backend task row when it is still present in the snapshot.
  // task 保存快照中仍存在时的当前后端任务行。
  const task = taskId ? navigation?.tasks?.find((candidate) => candidate.taskId === taskId) ?? null : null;
  // messages stores the read-only transcript rows for the child task conversation.
  // messages 保存子任务会话的只读转录行。
  const messages = React.useMemo(
    () => taskId ? args.props.workbench.messages.filter((message) => message.conversationId === taskId) : [],
    [args.props.workbench.messages, taskId],
  );
  // renderItems stores parsed transcript rows for the side-panel child conversation.
  // renderItems 保存侧栏子会话解析后的转录行。
  const renderItems = React.useMemo(
    () => projectConversationRenderItems(messages, t),
    [messages, t],
  );
  // subagentConversation stores the minimal conversation summary consumed by ConversationWorkspace.
  // subagentConversation 保存 ConversationWorkspace 消费的最小会话摘要。
  const subagentConversation = React.useMemo<ConversationSummary | undefined>(() => {
    if (!taskId) {
      return undefined;
    }
    return {
      id: taskId,
      projectId: selectedContextProjectId(args.props),
      runPhase: task?.isRunning ? "requesting" : task?.runState,
      status: task?.isRunning ? "running" : task?.runState ?? "completed",
      title: task?.label ?? (args.tab.title || workspaceContextTabDefaultTitle("subagent", t)),
    };
  }, [args.props, args.tab.title, task, taskId, t]);
  // workspacePath stores the parent project path used for Markdown media resolution.
  // workspacePath 保存用于 Markdown 媒体解析的父项目路径。
  const workspacePath = subagentWorkspacePath(args.props.workbench, parentConversationId);
  // taskRunning records whether the backend still allows stopping this task.
  // taskRunning 记录后端是否仍允许停止该任务。
  const taskRunning = Boolean(task?.isRunning && parentConversationId && taskId);
  // taskOpen records whether the backend still exposes this task as an unclosed agent.
  // taskOpen 记录后端是否仍将该任务暴露为未关闭智能体。
  const taskOpen = Boolean(task && parentConversationId && taskId);
  // handleStopConfirmed executes the single-task stop after confirmation.
  // handleStopConfirmed 在确认后执行单任务停止。
  const handleStopConfirmed = React.useCallback(async () => {
    if (!parentConversationId || !taskId || stopSubmitting) {
      return;
    }
    setStopSubmitting(true);
    try {
      await args.props.onStopSubagentTask(parentConversationId, taskId);
      setStopConfirmOpen(false);
    } finally {
      setStopSubmitting(false);
    }
  }, [args.props, parentConversationId, stopSubmitting, taskId]);
  // handleCloseConfirmed closes the child-agent lifecycle after confirmation.
  // handleCloseConfirmed 在确认后关闭子智能体生命周期。
  const handleCloseConfirmed = React.useCallback(async () => {
    if (!parentConversationId || !taskId || closeSubmitting) {
      return;
    }
    setCloseSubmitting(true);
    try {
      await args.props.onCloseSubagentTask(parentConversationId, taskId);
      setCloseConfirmOpen(false);
      args.scrollSnapshots.current.delete(taskId);
      args.expandedStates.current.delete(taskId);
      args.props.onCloseContextTab(args.tab.id);
    } finally {
      setCloseSubmitting(false);
    }
  }, [args.expandedStates, args.props, args.scrollSnapshots, args.tab.id, closeSubmitting, parentConversationId, taskId]);
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
  // handleSubagentExpandedStateChange stores expansion choices for the exact child transcript only.
  // handleSubagentExpandedStateChange 仅为精确子转录保存展开选择。
  const handleSubagentExpandedStateChange = React.useCallback((
    surfaceId: string,
    state: ConversationSurfaceExpandedState,
  ): void => {
    if (surfaceId === taskId) {
      args.expandedStates.current.set(surfaceId, state);
    }
  }, [args.expandedStates, taskId]);
  // handleLoadMoreHistory keeps the side transcript display-only until backend exposes child pagination here.
  // handleLoadMoreHistory 在此处暴露子会话分页前保持侧栏转录仅展示。
  const handleLoadMoreHistory = React.useCallback(async () => false, []);
  // autoLoadRouteRef remembers the last empty child transcript that already asked the backend for detail.
  // autoLoadRouteRef 记录已经向后端请求过详情的空子会话转录。
  const autoLoadRouteRef = React.useRef<string | null>(null);
  // onOpenSubagentPanel loads a focused persisted child transcript when this tab was restored without message cache.
  // onOpenSubagentPanel 在页签恢复但消息缓存缺失时加载聚焦的持久化子会话转录。
  const onOpenSubagentPanel = args.props.onOpenSubagentPanel;

  React.useEffect(() => {
    if (!parentConversationId || !taskId) {
      setTranscriptLoading(false);
      return;
    }
    if (messages.length > 0) {
      setTranscriptLoading(false);
      return;
    }
    const routeKey = `${parentConversationId}:${taskId}`;
    if (autoLoadRouteRef.current === routeKey) {
      return;
    }
    autoLoadRouteRef.current = routeKey;
    let active = true;
    setTranscriptLoading(true);
    void onOpenSubagentPanel(parentConversationId, taskId)
      .catch(() => undefined)
      .finally(() => {
        if (active) {
          setTranscriptLoading(false);
        }
      });
    return () => {
      active = false;
    };
  }, [messages.length, onOpenSubagentPanel, parentConversationId, taskId]);

  if (!parentConversationId || !taskId) {
    return <WorkspaceSubagentEmptyState title={workspaceContextTabDefaultTitle("subagent", t)} />;
  }
  // commandSessions limits side-panel terminal state to the displayed child task.
  // commandSessions 将侧栏终端状态限定在当前展示的子任务内。
  const commandSessions = (args.props.workbench.commandSessions ?? [])
    .filter((session) => session.ownerSessionId === taskId);

  return (
    <div className="workspace-subagent-panel">
      <ConversationWorkspace
        commandSessions={commandSessions}
        conversation={subagentConversation}
        error={null}
        hasMoreHistory={false}
        hiddenThinkingBlockIds={hiddenThinkingBlockIds}
        historyLoading={false}
        initialExpandedState={args.expandedStates.current.get(taskId) ?? null}
        messages={messages}
        parentConversationId={taskId}
        renderItems={renderItems}
        resolveComposerAttachmentMedia={composerAttachmentMedia}
        resolveMediaSourceUrl={args.props.resolveMediaSourceUrl}
        scrollRef={scrollRef}
        sessionRunning={Boolean(task?.isRunning)}
        suppressEmptyPlaceholder={transcriptLoading && messages.length === 0}
        workspacePath={workspacePath}
        onCopyText={args.props.onCopyText}
        onExpandedStateChange={handleSubagentExpandedStateChange}
        onExtractGeneratedMediaAttachments={generatedMediaAttachmentsFromToolDetailPayload}
        onLoadMoreHistory={handleLoadMoreHistory}
        onPreserveHistoryPrepend={subagentScrollController.preserveHistoryPrepend}
        onLoadToolDetail={args.props.onLoadToolDetail}
        onOpenSubagentPanel={args.props.onOpenSubagentPanel}
        onScrollStateChange={subagentScrollController.handleScroll}
      />
      {transcriptLoading && messages.length === 0 ? (
        <div className="workspace-subagent-loading" role="status" aria-live="polite">
          <Bot size={18} />
          <span>{t("conversation.subagent.loadingTranscript")}</span>
        </div>
      ) : null}
      {!tailLocked ? (
        <Button
          aria-label={t("composer.scrollToBottom")}
          className={cn("workspace-subagent-scroll-bottom", taskRunning || taskOpen ? "workspace-subagent-scroll-bottom-with-footer" : null)}
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
              onClick={() => setStopConfirmOpen(true)}
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
              onClick={() => setCloseConfirmOpen(true)}
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
    <div className="workspace-subagent-empty">
      <Bot size={22} />
      <span>{props.title}</span>
    </div>
  );
}

// workspaceContextTabIcon returns the icon used for one tab kind.
// workspaceContextTabIcon 返回单个页签类型使用的图标。
function workspaceContextTabIcon(kind: WorkspaceContextTabKind, size: number): React.ReactNode {
  if (kind === "subagent") {
    return <Bot size={size} />;
  }
  if (kind === "files") {
    return <Folder size={size} />;
  }
  if (kind === "memory") {
    return <Brain size={size} />;
  }
  return <GitPullRequest size={size} />;
}

// workspaceContextTabDefaultTitle returns the initial title for one tab kind.
// workspaceContextTabDefaultTitle 返回单个页签类型的初始标题。
export function workspaceContextTabDefaultTitle(kind: WorkspaceContextTabKind, t?: (key: TranslationKey) => string): string {
  if (kind === "subagent") {
    return t ? t("conversation.subagent.schedule") : "Subagent";
  }
  if (kind === "files") {
    return t ? t("workspaceFilePreview.openFile") : "Open file";
  }
  if (kind === "memory") {
    return t ? t("contextPanel.memory") : "Memory";
  }
  return t ? t("contextPanel.review") : "Review";
}

// workspaceContextTabDisplayTitle returns the locale-aware visible title for one opened tab.
// workspaceContextTabDisplayTitle 返回单个已打开页签按当前语言显示的标题。
function workspaceContextTabDisplayTitle(tab: WorkspaceContextTabItem, t: (key: TranslationKey) => string): string {
  if (tab.kind === "files" || tab.kind === "subagent") {
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
    <div className="context-panel-loading-state">
      <span>{t("workspaceContext.loadingReviewPanel")}</span>
    </div>
  );
}

// WorkspaceFilesPanelLoading renders while the file panel chunk is loading.
// WorkspaceFilesPanelLoading 渲染文件面板 chunk 加载期间的轻量状态。
function WorkspaceFilesPanelLoading() {
  const { t } = useI18n();
  return (
    <div className="context-panel-loading-state">
      <span>{t("workspaceContext.loadingFilesPanel")}</span>
    </div>
  );
}
