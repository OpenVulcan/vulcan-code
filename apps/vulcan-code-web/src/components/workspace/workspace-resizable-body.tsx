import React from "react";
import {
  WorkspaceContextPanel,
  type WorkspaceContextTabItem,
  type WorkspaceContextTabKind,
  type WorkspaceContextPanelProps,
} from "@/components/workspace/WorkspaceContextPanel";
import { AdaptiveToolPanel } from "@/components/workspace/adaptive-tool-panel";
import { DynamicPluginMountPanel } from "@/components/plugins/dynamic-plugin-mount-panel";
import { WorkspaceTitleMenu } from "@/components/workspace/workspace-title-menu";
import { SidebarTrigger } from "@/components/ui/sidebar";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ConversationMenuAction } from "@/components/project/project-sidebar-types";
import type {
  ComposerAttachmentSummary,
  ConversationSummary,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";
import type { WorkspacePagePanelDescriptor } from "@/components/workspace/workspace-page-descriptor";
import type { WorkspaceReviewFocusRequest } from "@/diff/types";

// WORKSPACE_CONTEXT_PANEL_TRANSITION_MS matches the CSS slide animation duration.
// WORKSPACE_CONTEXT_PANEL_TRANSITION_MS 对齐 CSS 右侧面板滑动动画时长。
const WORKSPACE_CONTEXT_PANEL_TRANSITION_MS = 220;
// WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH stores the initial docked context panel width.
// WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH 保存停靠上下文面板的初始宽度。
const WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH = 420;
// WORKSPACE_CONTEXT_PANEL_MIN_WIDTH stores the smallest manual docked panel width.
// WORKSPACE_CONTEXT_PANEL_MIN_WIDTH 保存停靠面板手动调整时的最小宽度。
const WORKSPACE_CONTEXT_PANEL_MIN_WIDTH = 320;
// WORKSPACE_CONTEXT_PANEL_MAX_WIDTH stores the largest manual docked panel width.
// WORKSPACE_CONTEXT_PANEL_MAX_WIDTH 保存停靠面板手动调整时的最大宽度。
const WORKSPACE_CONTEXT_PANEL_MAX_WIDTH = 640;

// DraftConversationSelection describes a not-yet-persisted conversation opened from a project.
// DraftConversationSelection 描述从项目打开但尚未持久化的草稿会话。
export interface DraftConversationSelection {
  // Stable local-only draft identifier.
  // 稳定的仅本地草稿标识。
  id: string;
  // Project id that will own the conversation after first submit.
  // 首次发送后将拥有该会话的项目标识。
  projectId: string;
  // Project display name shown in empty-state text.
  // 空状态文案中展示的项目名称。
  projectName: string;
  // Optimistic first submission rendered while the backend materializes the durable conversation.
  // 后端物化持久化会话期间渲染的乐观首条提交。
  pendingSubmission?: DraftConversationPendingSubmission | null;
}

// DraftConversationPendingSubmission describes one accepted-by-the-UI first prompt before service acknowledgement.
// DraftConversationPendingSubmission 描述服务确认前已被界面接纳的首条提示词。
export interface DraftConversationPendingSubmission {
  // Stable client-generated identifier shared with the service request and acknowledgement.
  // 与服务请求及确认共享的客户端稳定生成标识。
  clientSubmissionId: string;
  // User-visible prompt body rendered immediately after send.
  // 点击发送后立即渲染的用户可见提示词正文。
  prompt: string;
  // Structured composer attachments rendered with the optimistic message.
  // 与乐观消息共同渲染的结构化输入区附件。
  attachments: ComposerAttachmentSummary[];
  // Unix-millisecond timestamp captured when the UI accepts the submission.
  // 界面接纳提交时捕获的 Unix 毫秒时间戳。
  startedAt: number;
  // Current bounded preparation stage shown to the user.
  // 向用户展示的当前有界准备阶段。
  stage: "creating" | "preparing";
}

// WorkspaceResizableBody renders the main chat area and optional right context panel.
// WorkspaceResizableBody 渲染主会话区域与可选右侧上下文面板。
export function WorkspaceResizableBody(props: {
  // Whether the right context panel is visible.
  // 右侧上下文面板是否可见。
  rightPanelOpen: boolean;
  // Whether the right context panel floats over the main area.
  // 右侧上下文面板是否浮动覆盖主区域。
  rightPanelFloating: boolean;
  // Opened right context panel tabs.
  // 已打开的右侧上下文面板页签。
  contextPanelTabs: WorkspaceContextTabItem[];
  // Active right context panel tab id.
  // 当前活动的右侧上下文面板页签标识。
  contextPanelActiveTabId: string | null;
  // Monotonic request id used to focus the review panel on uncommitted changes.
  // 用于让审核面板聚焦未提交变更的单调请求标识。
  reviewFocusRequest: WorkspaceReviewFocusRequest;
  // Startup snapshot forwarded to layout-bound panels.
  // 转发给布局相关面板的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot forwarded to the right context panel.
  // 转发给右侧上下文面板的工作台快照。
  workbench: WorkbenchSnapshot;
  // Current draft conversation selection.
  // 当前草稿会话选择。
  selectedDraftConversation: DraftConversationSelection | null;
  // Current selected conversation summary.
  // 当前选中会话摘要。
  selectedConversation?: ConversationSummary;
  // Whether the selected conversation is pinned.
  // 当前选中会话是否已置顶。
  isSelectedConversationPinned?: boolean;
  // Current selected conversation id.
  // 当前选中会话标识。
  selectedConversationId: string | null;
  // Main chat surface rendered inside the workspace body.
  // 渲染在工作区主体内的主会话界面。
  children: React.ReactNode;
  // Optional title overriding the conversation header title.
  // 覆盖会话标题栏标题的可选标题。
  headerTitle?: string;
  // Optional detail text shown inline after the override header title.
  // 显示在覆盖标题后方的可选行内详情。
  headerDetail?: string;
  // Optional action cluster rendered on the override header right side.
  // 渲染在覆盖标题右侧的可选操作区。
  headerActions?: React.ReactNode;
  // Optional right-edge header actions registered by the active page.
  // 当前页面注册的可选标题栏右侧动作。
  headerRightActions?: React.ReactNode;
  // App-wide chrome actions rendered at the far right of the shared header.
  // 渲染在共享标题栏最右侧的应用级窗口控制区。
  chromeActions?: React.ReactNode;
  // Optional page-registered adaptive panels rendered by the shared body shell.
  // 由共享主体外壳渲染的可选页面注册自适应面板列表。
  toolPanels?: WorkspacePagePanelDescriptor[];
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Handles one conversation-level title menu action.
  // 处理单个会话级标题菜单动作。
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Toggles the right context panel.
  // 切换右侧上下文面板。
  onToggleRightPanel: () => void;
  // Opens or focuses one right context tab kind.
  // 打开或聚焦一个右侧上下文页签类型。
  onOpenContextTab: (kind: WorkspaceContextTabKind) => void;
  // Closes one right context tab.
  // 关闭一个右侧上下文页签。
  onCloseContextTab: (tabId: string) => void;
  // Activates one right context tab.
  // 激活一个右侧上下文页签。
  onActivateContextTab: (tabId: string) => void;
  // Updates a file context tab title.
  // 更新文件上下文页签标题。
  onFileContextTabTitleChange: (tabId: string, title: string) => void;
  // Records a manual left sidebar toggle.
  // 记录一次手动左侧栏切换。
  onToggleLeftPanel: () => void;
  // Opens one subagent transcript in the right context panel.
  // 在右侧上下文面板中打开单个子代理转录。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Stops one backend subagent task.
  // 停止单个后端子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Closes one backend subagent lifecycle.
  // 关闭单个后端子智能体生命周期。
  onCloseSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Force-stops one live command session.
  // 强制停止单条实时命令会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
  // Copies raw diff text from the right context panel.
  // 从右侧上下文面板复制原始 diff 文本。
  onCopyRawDiff: (rawDiff: string) => void;
  // Copies plain text from the right context panel.
  // 从右侧上下文面板复制普通文本。
  onCopyText: WorkspaceContextPanelProps["onCopyText"];
  // Resolves local media paths from the right context panel.
  // 为右侧上下文面板解析本地媒体路径。
  resolveMediaSourceUrl: WorkspaceContextPanelProps["resolveMediaSourceUrl"];
  // Runs one VMM memory action for the right context panel.
  // 为右侧上下文面板执行单个 VMM 记忆动作。
  onRunVmmAction: WorkspaceContextPanelProps["onRunVmmAction"];
  // Loads one workspace diff snapshot for the right context panel.
  // 为右侧上下文面板载入单个工作区 diff 快照。
  loadWorkspaceDiffSnapshot: WorkspaceContextPanelProps["loadWorkspaceDiffSnapshot"];
  // Loads one workspace diff file detail for the right context panel.
  // 为右侧上下文面板载入单个工作区 diff 文件详情。
  loadWorkspaceDiffFileDetail: WorkspaceContextPanelProps["loadWorkspaceDiffFileDetail"];
  // Loads compact backend file-level search hits for the right context panel.
  // 为右侧上下文面板载入紧凑的后端文件级搜索命中。
  loadWorkspaceDiffSearch: WorkspaceContextPanelProps["loadWorkspaceDiffSearch"];
  // Runs one file-level Git action for the right context panel.
  // 为右侧上下文面板执行一次文件级 Git 操作。
  runWorkspaceGitFileAction: WorkspaceContextPanelProps["runWorkspaceGitFileAction"];
  // Records one loaded workspace diff snapshot in the project-level Git store.
  // 将一次已加载的工作区 diff 快照记录到项目级 Git 状态。
  onWorkspaceDiffSnapshotLoaded?: WorkspaceContextPanelProps["onWorkspaceDiffSnapshotLoaded"];
  // Refreshes the project-level Git summary after a Git mutation.
  // Git 变更后刷新项目级 Git 摘要。
  onWorkspaceGitStateChanged?: WorkspaceContextPanelProps["onWorkspaceGitStateChanged"];
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
  onSelectGitRepository?: WorkspaceContextPanelProps["onSelectGitRepository"];
  // Loads one lazy workspace file directory for the right context panel.
  // 为右侧上下文面板加载单个懒加载工作区文件目录。
  loadWorkspaceFileDirectory: WorkspaceContextPanelProps["loadWorkspaceFileDirectory"];
  // Loads one workspace file preview for the right context panel.
  // 为右侧上下文面板加载单个工作区文件预览。
  loadWorkspaceFilePreview: WorkspaceContextPanelProps["loadWorkspaceFilePreview"];
  // Loads one hidden tool detail record for the right context panel.
  // 为右侧上下文面板加载单条隐藏工具详情记录。
  // Emits WebUI click traces from nested console controls.
  // 从嵌套控制台控件发出 WebUI 点击追踪。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}) {
  const { t } = useI18n();
  // rightPanelTransition keeps the context panel mounted long enough for exit animation.
  // rightPanelTransition 让右侧上下文面板在退出动画结束前保持挂载。
  const [rightPanelTransition, setRightPanelTransition] = React.useState(() => ({
    floating: props.rightPanelFloating,
    mounted: props.rightPanelOpen,
    visible: props.rightPanelOpen,
  }));
  // dockedPanelWidth stores the user-adjustable non-floating context panel width.
  // dockedPanelWidth 保存用户可调整的非浮动上下文面板宽度。
  const [dockedPanelWidth, setDockedPanelWidth] = React.useState(WORKSPACE_CONTEXT_PANEL_DEFAULT_WIDTH);
  React.useEffect(() => {
    let frameId: number | null = null;
    let timeoutId: number | null = null;

    if (props.rightPanelOpen) {
      setRightPanelTransition((current) => ({
        floating: props.rightPanelFloating,
        mounted: true,
        visible: current.mounted,
      }));
      frameId = window.requestAnimationFrame(() => {
        setRightPanelTransition((current) => ({
          ...current,
          floating: props.rightPanelFloating,
          visible: true,
        }));
      });
    } else {
      setRightPanelTransition((current) => (
        current.mounted ? { ...current, visible: false } : current
      ));
      timeoutId = window.setTimeout(() => {
        setRightPanelTransition({
          floating: props.rightPanelFloating,
          mounted: false,
          visible: false,
        });
      }, WORKSPACE_CONTEXT_PANEL_TRANSITION_MS);
    }

    return () => {
      if (frameId != null) {
        window.cancelAnimationFrame(frameId);
      }
      if (timeoutId != null) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [props.rightPanelFloating, props.rightPanelOpen]);
  // rightPanelVisualState selects the transition class shared by docked and floating panels.
  // rightPanelVisualState 选择停靠与浮动面板共用的过渡类名。
  const rightPanelVisualState = rightPanelTransition.visible ? "is-open" : "is-closed";
  // renderedRightPanelFloating uses the preflight target while open and retains transition state only for exit.
  // renderedRightPanelFloating 在打开期间使用预判目标，仅在退出期间保留过渡状态。
  const renderedRightPanelFloating = props.rightPanelOpen
    ? props.rightPanelFloating
    : rightPanelTransition.floating;
  // floatingContextLayoutActive exposes the preflight floating target before the panel mount animation begins.
  // floatingContextLayoutActive 在面板挂载动画开始前暴露预判的浮动目标。
  const floatingContextLayoutActive = renderedRightPanelFloating
    && (props.rightPanelOpen || rightPanelTransition.mounted);
  // dockedPanelOpen indicates whether the non-floating panel currently reserves visible width.
  // dockedPanelOpen 表示非浮动面板当前是否保留可见宽度。
  const dockedPanelOpen = rightPanelTransition.visible && !renderedRightPanelFloating;
  // dockedPanelMounted indicates whether the non-floating panel remains in the flex layout.
  // dockedPanelMounted 表示非浮动面板是否仍保留在 flex 布局中。
  const dockedPanelMounted = rightPanelTransition.mounted && !renderedRightPanelFloating;
  // rightPanelMounted keeps one context panel instance alive across floating and docked modes.
  // rightPanelMounted 让同一个上下文面板实例在浮动与停靠模式切换时保持存活。
  const rightPanelMounted = rightPanelTransition.mounted;
  // dockedLayoutStyle feeds the animated context width into CSS.
  // dockedLayoutStyle 将动画所需的上下文宽度传入 CSS。
  const dockedLayoutStyle = {
    "--workspace-context-panel-width": dockedPanelOpen ? `${dockedPanelWidth}px` : "0px",
  } as React.CSSProperties;
  // handleDockedResizePointerDown starts manual resizing for the docked context panel.
  // handleDockedResizePointerDown 启动停靠上下文面板的手动调整宽度。
  const handleDockedResizePointerDown = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (!dockedPanelOpen) {
      return;
    }
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = dockedPanelWidth;
    const shellWidth = event.currentTarget
      .closest(".workspace-resizable-body-shell")
      ?.getBoundingClientRect().width ?? window.innerWidth;
    const maxWidth = Math.max(
      WORKSPACE_CONTEXT_PANEL_MIN_WIDTH,
      Math.min(WORKSPACE_CONTEXT_PANEL_MAX_WIDTH, shellWidth - 800 - 9),
    );
    // clampDockedPanelWidth keeps manual resizing inside usable workspace bounds.
    // clampDockedPanelWidth 将手动调整宽度限制在可用工作区范围内。
    const clampDockedPanelWidth = (width: number) => Math.min(
      maxWidth,
      Math.max(WORKSPACE_CONTEXT_PANEL_MIN_WIDTH, width),
    );
    // handlePointerMove mirrors the old right-edge panel drag behavior.
    // handlePointerMove 复刻原右侧面板拖拽时左移增宽、右移减宽的行为。
    const handlePointerMove = (moveEvent: PointerEvent) => {
      setDockedPanelWidth(clampDockedPanelWidth(startWidth - (moveEvent.clientX - startX)));
    };
    // handlePointerUp clears document-level drag listeners after resizing.
    // handlePointerUp 在调整结束后清理文档级拖拽监听。
    const handlePointerUp = () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
    };
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp, { once: true });
  }, [dockedPanelOpen, dockedPanelWidth]);
  // selectedProjectId is the exact trusted project context for workspace plugin actions.
  // selectedProjectId 是工作区插件 Action 使用的精确可信项目上下文。
  const selectedProjectId = props.selectedDraftConversation?.projectId
    ?? props.selectedConversation?.projectId
    ?? null;
  // selectedWorkspacePath is resolved only from the host-projected project catalog.
  // selectedWorkspacePath 仅从宿主投影的项目目录解析。
  const selectedWorkspacePath = props.workbench.projects.find(
    (project) => project.id === selectedProjectId,
  )?.path ?? null;
  // enabledPluginMounts flattens only enabled manifest-declared dynamic UI contributions.
  // enabledPluginMounts 仅展开已启用且由清单声明的动态界面贡献。
  const enabledPluginMounts = props.workbench.plugins.flatMap((plugin) => (
    plugin.enabled && plugin.extension
      ? plugin.extension.uiMounts.map((mount) => ({ plugin, mount }))
      : []
  ));
  // workspaceMainPluginMounts target the integrated primary workspace surface.
  // workspaceMainPluginMounts 指向集成式工作区主界面。
  const workspaceMainPluginMounts = enabledPluginMounts.filter(
    ({ mount }) => mount.point === "workspaceMain",
  );
  // conversationRightPluginMounts target the selected conversation's right-side context surface.
  // conversationRightPluginMounts 指向所选会话的右侧上下文界面。
  const conversationRightPluginMounts = enabledPluginMounts.filter(
    ({ mount }) => mount.point === "conversationRightPanel",
  );
  // mainContent keeps the body content shape identical with or without a side panel.
  // mainContent 保持有无侧边面板时主体内容结构一致。
  const mainContent = (
    <section className="workspace-main-shell">
      {props.children}
      {workspaceMainPluginMounts.length > 0 ? (
        <div className="workspace-plugin-main-mounts">
          {workspaceMainPluginMounts.map(({ plugin, mount }) => (
            <DynamicPluginMountPanel
              key={`${plugin.id}:${mount.id}`}
              mount={mount}
              plugin={plugin}
              workspacePath={selectedWorkspacePath}
            />
          ))}
        </div>
      ) : null}
    </section>
  );
  // renderedMainContent lets registered page panels share one adaptive layout implementation.
  // renderedMainContent 让页面注册面板共用同一套自适应布局实现。
  // Keep descriptor shells mounted when panel content is temporarily absent so the main React subtree retains identity.
  // 面板内容暂时不存在时仍保留描述槽外壳，使主 React 子树保持组件身份。
  const renderedMainContent = (props.toolPanels ?? [])
    .reduceRight<React.ReactNode>((content, panel) => (
      <AdaptiveToolPanel
        className={panel.className}
        contentClassName={panel.contentClassName}
        contentShellClassName={panel.contentShellClassName}
        expanded={panel.expanded}
        forceOverlay={panel.forceOverlay}
        inlineGap={panel.inlineGap}
        minContentWidth={panel.minContentWidth}
        onLayoutModeWillChange={panel.onLayoutModeWillChange}
        panel={panel.panel}
        panelWidth={panel.panelWidth}
        reserveGap={panel.reserveGap}
        side={panel.side}
      >
        {content}
      </AdaptiveToolPanel>
    ), mainContent);

  return (
    <div className={cn("workspace-resizable-body-shell", floatingContextLayoutActive && "has-floating-context")}>
      <ConsoleHeader
        className={props.headerTitle ? "workspace-custom-header" : undefined}
        draftConversation={props.selectedDraftConversation}
        conversation={props.selectedConversation}
        detail={props.headerDetail}
        headerActions={props.headerActions}
        headerRightActions={props.headerRightActions}
        chromeActions={props.chromeActions}
        titlebarDragRegion={props.titlebarDragRegion}
        onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        isConversationPinned={props.isSelectedConversationPinned}
        title={props.headerTitle}
        onConversationMenuAction={props.onConversationMenuAction}
        onToggleLeftPanel={props.onToggleLeftPanel}
      />
      <div className="workspace-resizable-body" style={dockedPanelMounted ? dockedLayoutStyle : undefined}>
        <div className="workspace-main-panel">
          {renderedMainContent}
        </div>
        {dockedPanelMounted ? (
          <>
            <div
              aria-label={t("workspacePanel.resizeRight")}
              aria-orientation="vertical"
              className={cn("workspace-context-resize-handle", rightPanelVisualState)}
              onPointerDown={handleDockedResizePointerDown}
              role="separator"
            />
          </>
        ) : null}
        {rightPanelMounted ? (
          <div
            className={cn(
              "workspace-context-panel",
              rightPanelVisualState,
              renderedRightPanelFloating && "is-floating",
            )}
            id="workspaceContext"
          >
            <WorkspaceContextPanel
              activeTabId={props.contextPanelActiveTabId}
              floating={renderedRightPanelFloating}
              reviewFocusRequest={props.reviewFocusRequest}
              selectedDraftConversation={props.selectedDraftConversation}
              selectedConversationId={props.selectedConversationId}
              tabs={props.contextPanelTabs}
              workbench={props.workbench}
              onActivateContextTab={props.onActivateContextTab}
              onCloseContextTab={props.onCloseContextTab}
              onClose={props.onToggleRightPanel}
              onCopyRawDiff={props.onCopyRawDiff}
              onCopyText={props.onCopyText}
              onFileTabTitleChange={props.onFileContextTabTitleChange}
              onOpenContextTab={props.onOpenContextTab}
              onOpenSubagentPanel={props.onOpenSubagentPanel}
              onCloseSubagentTask={props.onCloseSubagentTask}
              onStopCommandSession={props.onStopCommandSession}
              onStopSubagentTask={props.onStopSubagentTask}
              onRunVmmAction={props.onRunVmmAction}
              resolveMediaSourceUrl={props.resolveMediaSourceUrl}
              loadWorkspaceFileDirectory={props.loadWorkspaceFileDirectory}
              loadWorkspaceFilePreview={props.loadWorkspaceFilePreview}
              loadWorkspaceDiffFileDetail={props.loadWorkspaceDiffFileDetail}
              loadWorkspaceDiffSearch={props.loadWorkspaceDiffSearch}
              loadWorkspaceDiffSnapshot={props.loadWorkspaceDiffSnapshot}
              runWorkspaceGitFileAction={props.runWorkspaceGitFileAction}
              onWorkspaceDiffSnapshotLoaded={props.onWorkspaceDiffSnapshotLoaded}
              onWorkspaceGitStateChanged={props.onWorkspaceGitStateChanged}
              onSelectGitRepository={props.onSelectGitRepository}
              gitCommitActionBusy={props.gitCommitActionBusy}
              gitCommitActionDisabled={props.gitCommitActionDisabled}
              gitCommitActionLabel={props.gitCommitActionLabel}
              onOpenGitCommitAction={props.onOpenGitCommitAction}
            />
            {conversationRightPluginMounts.length > 0 ? (
              <div className="conversation-plugin-right-mounts">
                {conversationRightPluginMounts.map(({ plugin, mount }) => (
                  <DynamicPluginMountPanel
                    key={`${plugin.id}:${mount.id}`}
                    mount={mount}
                    plugin={plugin}
                    workspacePath={selectedWorkspacePath}
                  />
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}

// ConsoleHeader renders the inset title bar for the current conversation.
// ConsoleHeader 渲染当前会话的 inset 标题栏。
function ConsoleHeader(props: {
  // Optional CSS class for specialized header layouts.
  // 专用标题栏布局使用的可选 CSS 类。
  className?: string;
  // Current selected conversation summary.
  // 当前选中会话摘要。
  conversation?: ConversationSummary;
  // Whether the selected conversation is pinned.
  // 当前选中会话是否已置顶。
  isConversationPinned?: boolean;
  // Optional override title used by embedded workspace views.
  // 内嵌工作台视图使用的可选覆盖标题。
  title?: string;
  // Optional override detail used by embedded workspace views.
  // 内嵌工作台视图使用的可选覆盖详情。
  detail?: string;
  // Optional override actions used by embedded workspace views.
  // 内嵌工作台视图使用的可选覆盖操作区。
  headerActions?: React.ReactNode;
  // Optional right-edge actions registered by the active workspace page.
  // 当前工作区页面注册的可选右侧操作区。
  headerRightActions?: React.ReactNode;
  // App-wide chrome actions rendered at the far right of the shared header.
  // 渲染在共享标题栏最右侧的应用级窗口控制区。
  chromeActions?: React.ReactNode;
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Current draft conversation selection.
  // 当前草稿会话选择。
  draftConversation: DraftConversationSelection | null;
  // Handles one conversation-level title menu action.
  // 处理单个会话级标题菜单动作。
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Records a manual left sidebar toggle.
  // 记录一次手动左侧栏切换。
  onToggleLeftPanel: () => void;
}) {
  // t translates stable console title fallbacks.
  // t 翻译稳定的控制台标题回退文案。
  const { t } = useI18n();
  // headerTitle chooses an embedded view title before falling back to conversation naming.
  // headerTitle 优先使用内嵌视图标题，否则回退到会话命名。
  const headerTitle = props.title ?? (props.draftConversation ? t("console.newConversation") : props.conversation?.title ?? t("console.title"));
  // titleMenuConversation limits overflow actions to real conversation headers.
  // titleMenuConversation 将溢出操作限制在真实会话标题栏中。
  const titleMenuConversation = props.title || props.draftConversation ? undefined : props.conversation;

  return (
    <header className={cn("console-header", props.className)}>
      <div className="console-header-main">
        <SidebarTrigger onClick={props.onToggleLeftPanel} />
        <span className="console-header-separator" aria-hidden="true" />
        <div
          className="workspace-custom-header-title"
        >
          <div
            className="workspace-header-title-text"
            data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
            onDoubleClick={props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick}
          >
            <h1>{headerTitle}</h1>
            {props.detail ? <small>{props.detail}</small> : null}
          </div>
          <WorkspaceTitleMenu
            conversation={titleMenuConversation}
            isPinned={props.isConversationPinned}
            title={headerTitle}
            onConversationMenuAction={props.onConversationMenuAction}
          />
        </div>
        <span
          aria-hidden="true"
          className="console-header-drag-fill"
          data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
          onDoubleClick={props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick}
        />
      </div>
      <div className="console-header-actions">
        {props.headerActions}
        {props.headerRightActions}
        {props.chromeActions}
      </div>
    </header>
  );
}
