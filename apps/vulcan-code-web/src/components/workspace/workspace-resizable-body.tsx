import React from "react";
import { SubagentNavigationMenu } from "@/components/conversation/conversation-chrome";
import {
  WorkspaceContextPanel,
  type WorkspaceContextPanelProps,
} from "@/components/workspace/WorkspaceContextPanel";
import { Button } from "@/components/ui/button";
import { SidebarTrigger } from "@/components/ui/sidebar";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type {
  ConversationSummary,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";
import {
  PanelRightClose,
  PanelRightOpen,
} from "lucide-react";

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
  // Current selected conversation id.
  // 当前选中会话标识。
  selectedConversationId: string | null;
  // Main chat surface rendered inside the workspace body.
  // 渲染在工作区主体内的主会话界面。
  children: React.ReactNode;
  // Optional title overriding the conversation header title.
  // 覆盖会话标题栏标题的可选标题。
  headerTitle?: string;
  // Optional detail line shown below the override header title.
  // 显示在覆盖标题下方的可选详情行。
  headerDetail?: string;
  // Optional action cluster rendered on the override header right side.
  // 渲染在覆盖标题右侧的可选操作区。
  headerActions?: React.ReactNode;
  // App-wide chrome actions rendered at the far right of the shared header.
  // 渲染在共享标题栏最右侧的应用级窗口控制区。
  chromeActions?: React.ReactNode;
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Whether the right panel toggle should be hidden for the current main view.
  // 当前主视图是否隐藏右侧面板切换按钮。
  hideRightPanelToggle?: boolean;
  // Toggles the right context panel.
  // 切换右侧上下文面板。
  onToggleRightPanel: () => void;
  // Records a manual left sidebar toggle.
  // 记录一次手动左侧栏切换。
  onToggleLeftPanel: () => void;
  // Opens one backend-routed subagent surface.
  // 打开单个由后端路由的子代理界面。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Closes the backend-routed subagent surface.
  // 关闭由后端路由的子代理界面。
  onCloseSubagentPanel: () => Promise<void>;
  // Stops one backend-routed subagent task.
  // 停止单个由后端路由的子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Copies raw diff text from the right context panel.
  // 从右侧上下文面板复制原始 diff 文本。
  onCopyRawDiff: (rawDiff: string) => void;
  // Runs one VMM memory action for the right context panel.
  // 为右侧上下文面板执行单个 VMM 记忆动作。
  onRunVmmAction: WorkspaceContextPanelProps["onRunVmmAction"];
  // Parses conversation messages into display blocks for the right context panel.
  // 为右侧上下文面板将会话消息解析为展示块。
  parseConversationBlocks: WorkspaceContextPanelProps["parseConversationBlocks"];
  // Loads one workspace diff snapshot for the right context panel.
  // 为右侧上下文面板载入单个工作区 diff 快照。
  loadWorkspaceDiffSnapshot: WorkspaceContextPanelProps["loadWorkspaceDiffSnapshot"];
  // Loads one workspace diff file detail for the right context panel.
  // 为右侧上下文面板载入单个工作区 diff 文件详情。
  loadWorkspaceDiffFileDetail: WorkspaceContextPanelProps["loadWorkspaceDiffFileDetail"];
  // Loads compact backend file-level search hits for the right context panel.
  // 为右侧上下文面板载入紧凑的后端文件级搜索命中。
  loadWorkspaceDiffSearch: WorkspaceContextPanelProps["loadWorkspaceDiffSearch"];
  // Emits WebUI click traces from nested console controls.
  // 从嵌套控制台控件发出 WebUI 点击追踪。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}) {
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
  // dockedPanelOpen indicates whether the non-floating panel currently reserves visible width.
  // dockedPanelOpen 表示非浮动面板当前是否保留可见宽度。
  const dockedPanelOpen = rightPanelTransition.visible && !rightPanelTransition.floating;
  // dockedPanelMounted indicates whether the non-floating panel remains in the flex layout.
  // dockedPanelMounted 表示非浮动面板是否仍保留在 flex 布局中。
  const dockedPanelMounted = rightPanelTransition.mounted && !rightPanelTransition.floating;
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
  return (
    <div className={cn("workspace-resizable-body-shell", rightPanelTransition.mounted && rightPanelTransition.floating && "has-floating-context")}>
      <ConsoleHeader
        className={props.headerTitle ? "workspace-custom-header" : undefined}
        draftConversation={props.selectedDraftConversation}
        conversation={props.selectedConversation}
        detail={props.headerDetail}
        headerActions={props.headerActions}
        chromeActions={props.chromeActions}
        titlebarDragRegion={props.titlebarDragRegion}
        onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        hideRightPanelToggle={props.hideRightPanelToggle}
        rightPanelOpen={props.rightPanelOpen}
        subagentNavigation={props.workbench.subagentNavigation}
        title={props.headerTitle}
        onToggleLeftPanel={props.onToggleLeftPanel}
        onToggleRightPanel={props.onToggleRightPanel}
        onOpenSubagentPanel={props.onOpenSubagentPanel}
        onCloseSubagentPanel={props.onCloseSubagentPanel}
        onStopSubagentTask={props.onStopSubagentTask}
        onTrace={props.onTrace}
      />
      <div className="workspace-resizable-body" style={dockedPanelMounted ? dockedLayoutStyle : undefined}>
        <div className="workspace-main-panel">
          <section className="workspace-main-shell">
            {props.children}
          </section>
        </div>
        {dockedPanelMounted ? (
          <>
            <div
              aria-label="调整右侧面板宽度"
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
              rightPanelTransition.floating && "is-floating",
            )}
            id="workspaceContext"
          >
            <WorkspaceContextPanel
              floating={rightPanelTransition.floating}
              selectedDraftConversation={props.selectedDraftConversation}
              selectedConversationId={props.selectedConversationId}
              workbench={props.workbench}
              onClose={props.onToggleRightPanel}
              onCopyRawDiff={props.onCopyRawDiff}
              onRunVmmAction={props.onRunVmmAction}
              parseConversationBlocks={props.parseConversationBlocks}
              loadWorkspaceDiffFileDetail={props.loadWorkspaceDiffFileDetail}
              loadWorkspaceDiffSearch={props.loadWorkspaceDiffSearch}
              loadWorkspaceDiffSnapshot={props.loadWorkspaceDiffSnapshot}
            />
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
  // Optional override title used by embedded workspace views.
  // 内嵌工作台视图使用的可选覆盖标题。
  title?: string;
  // Optional override detail used by embedded workspace views.
  // 内嵌工作台视图使用的可选覆盖详情。
  detail?: string;
  // Optional override actions used by embedded workspace views.
  // 内嵌工作台视图使用的可选覆盖操作区。
  headerActions?: React.ReactNode;
  // App-wide chrome actions rendered at the far right of the shared header.
  // 渲染在共享标题栏最右侧的应用级窗口控制区。
  chromeActions?: React.ReactNode;
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Whether to hide the conversation right-panel toggle.
  // 是否隐藏会话右侧面板切换按钮。
  hideRightPanelToggle?: boolean;
  // Current draft conversation selection.
  // 当前草稿会话选择。
  draftConversation: DraftConversationSelection | null;
  // Whether the right context panel is visible.
  // 右侧上下文面板是否可见。
  rightPanelOpen: boolean;
  // Current subagent navigation snapshot.
  // 当前子代理导航快照。
  subagentNavigation?: WorkbenchSnapshot["subagentNavigation"];
  // Records a manual left sidebar toggle.
  // 记录一次手动左侧栏切换。
  onToggleLeftPanel: () => void;
  // Toggles the right context panel.
  // 切换右侧上下文面板。
  onToggleRightPanel: () => void;
  // Opens one backend-routed subagent surface.
  // 打开单个由后端路由的子代理界面。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Closes the backend-routed subagent surface.
  // 关闭由后端路由的子代理界面。
  onCloseSubagentPanel: () => Promise<void>;
  // Stops one backend-routed subagent task.
  // 停止单个由后端路由的子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Emits WebUI click traces from subagent controls.
  // 从子代理控件发出 WebUI 点击追踪。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}) {
  // t translates stable console chrome labels.
  // t 翻译稳定的控制台外壳标签。
  const { t } = useI18n();
  // rightPanelLabel describes the current right panel toggle action.
  // rightPanelLabel 描述当前右侧面板切换动作。
  const rightPanelLabel = props.rightPanelOpen ? t("contextPanel.close") : t("contextPanel.open");
  // headerTitle chooses an embedded view title before falling back to conversation naming.
  // headerTitle 优先使用内嵌视图标题，否则回退到会话命名。
  const headerTitle = props.title ?? (props.draftConversation ? t("console.newConversation") : props.conversation?.title ?? t("console.title"));

  return (
    <header className={cn("console-header", props.className)}>
      <div className="console-header-main">
        <SidebarTrigger onClick={props.onToggleLeftPanel} />
        <span className="console-header-separator" aria-hidden="true" />
        {props.detail ? (
          <div
            className="workspace-custom-header-title"
            data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
            onDoubleClick={props.onTitlebarDoubleClick}
          >
            <h1>{headerTitle}</h1>
            <small>{props.detail}</small>
          </div>
        ) : (
          <h1
            data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
            onDoubleClick={props.onTitlebarDoubleClick}
          >
            {headerTitle}
          </h1>
        )}
        {!props.title ? (
          <SubagentNavigationMenu
            navigation={props.subagentNavigation}
            onCloseSubagentPanel={props.onCloseSubagentPanel}
            onOpenSubagentPanel={props.onOpenSubagentPanel}
            onStopSubagentTask={props.onStopSubagentTask}
            onTrace={props.onTrace}
          />
        ) : null}
        <span
          aria-hidden="true"
          className="console-header-drag-fill"
          data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
          onDoubleClick={props.onTitlebarDoubleClick}
        />
      </div>
      <div className="console-header-actions">
        {props.headerActions}
        {!props.hideRightPanelToggle ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                aria-label={rightPanelLabel}
                aria-pressed={props.rightPanelOpen}
                className="icon-button"
                onClick={props.onToggleRightPanel}
                size="icon-sm"
                type="button"
                variant={props.rightPanelOpen ? "secondary" : "ghost"}
              >
                {props.rightPanelOpen ? <PanelRightClose size={16} /> : <PanelRightOpen size={16} />}
              </Button>
            </TooltipTrigger>
            <TooltipContent side="bottom">{rightPanelLabel}</TooltipContent>
          </Tooltip>
        ) : null}
        {props.chromeActions}
      </div>
    </header>
  );
}
