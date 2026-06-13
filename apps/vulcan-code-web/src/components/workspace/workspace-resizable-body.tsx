import React from "react";
import { SubagentNavigationMenu } from "@/components/conversation/conversation-chrome";
import {
  WorkspaceContextPanel,
  type WorkspaceContextPanelProps,
} from "@/components/workspace/WorkspaceContextPanel";
import { Button } from "@/components/ui/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
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
  // Parses conversation messages into display blocks for the right context panel.
  // 为右侧上下文面板将会话消息解析为展示块。
  parseConversationBlocks: WorkspaceContextPanelProps["parseConversationBlocks"];
  // Loads one workspace diff snapshot for the right context panel.
  // 为右侧上下文面板载入单个工作区 diff 快照。
  loadWorkspaceDiffSnapshot: WorkspaceContextPanelProps["loadWorkspaceDiffSnapshot"];
  // Emits WebUI click traces from nested console controls.
  // 从嵌套控制台控件发出 WebUI 点击追踪。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}) {
  return (
    <div className={cn("workspace-resizable-body-shell", props.rightPanelFloating && "has-floating-context")}>
      <ResizablePanelGroup className="workspace-resizable-body" defaultLayout={{ workspaceMain: 68, workspaceContext: 32 }} orientation="horizontal">
        <ResizablePanel
          className="workspace-main-panel"
          defaultSize={props.rightPanelOpen && !props.rightPanelFloating ? 68 : 100}
          id="workspaceMain"
          minSize="800px"
        >
          <section className="workspace-main-shell">
            <ConsoleHeader
              draftConversation={props.selectedDraftConversation}
              conversation={props.selectedConversation}
              rightPanelOpen={props.rightPanelOpen}
              subagentNavigation={props.workbench.subagentNavigation}
              onToggleLeftPanel={props.onToggleLeftPanel}
              onToggleRightPanel={props.onToggleRightPanel}
              onOpenSubagentPanel={props.onOpenSubagentPanel}
              onCloseSubagentPanel={props.onCloseSubagentPanel}
              onStopSubagentTask={props.onStopSubagentTask}
              onTrace={props.onTrace}
            />
            {props.children}
          </section>
        </ResizablePanel>
        {props.rightPanelOpen && !props.rightPanelFloating ? (
          <>
            <ResizableHandle className="workspace-context-resize-handle" withHandle />
            <ResizablePanel
              className="workspace-context-panel"
              defaultSize={32}
              groupResizeBehavior="preserve-pixel-size"
              id="workspaceContext"
              minSize="320px"
            >
              <WorkspaceContextPanel
                selectedDraftConversation={props.selectedDraftConversation}
                selectedConversationId={props.selectedConversationId}
                workbench={props.workbench}
                onClose={props.onToggleRightPanel}
                onCopyRawDiff={props.onCopyRawDiff}
                parseConversationBlocks={props.parseConversationBlocks}
                loadWorkspaceDiffSnapshot={props.loadWorkspaceDiffSnapshot}
              />
            </ResizablePanel>
          </>
        ) : null}
      </ResizablePanelGroup>
      {props.rightPanelOpen && props.rightPanelFloating ? (
        <div className="workspace-context-floating-panel">
          <WorkspaceContextPanel
            floating
            selectedDraftConversation={props.selectedDraftConversation}
            selectedConversationId={props.selectedConversationId}
            workbench={props.workbench}
            onClose={props.onToggleRightPanel}
            onCopyRawDiff={props.onCopyRawDiff}
            parseConversationBlocks={props.parseConversationBlocks}
            loadWorkspaceDiffSnapshot={props.loadWorkspaceDiffSnapshot}
          />
        </div>
      ) : null}
    </div>
  );
}

// ConsoleHeader renders the inset title bar for the current conversation.
// ConsoleHeader 渲染当前会话的 inset 标题栏。
function ConsoleHeader(props: {
  // Current selected conversation summary.
  // 当前选中会话摘要。
  conversation?: ConversationSummary;
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

  return (
    <header className="console-header">
      <div className="console-header-main">
        <SidebarTrigger onClick={props.onToggleLeftPanel} />
        <span className="console-header-separator" aria-hidden="true" />
        <h1>{props.draftConversation ? t("console.newConversation") : props.conversation?.title ?? t("console.title")}</h1>
        <SubagentNavigationMenu
          navigation={props.subagentNavigation}
          onCloseSubagentPanel={props.onCloseSubagentPanel}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
          onStopSubagentTask={props.onStopSubagentTask}
          onTrace={props.onTrace}
        />
      </div>
      <div className="console-header-actions">
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
      </div>
    </header>
  );
}
