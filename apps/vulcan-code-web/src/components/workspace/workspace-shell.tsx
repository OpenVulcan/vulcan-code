import React from "react";
import { ChatArea } from "@/components/conversation/chat-area";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type {
  ComposerAiCapabilityConfigurationSave,
  ComposerSlashCommandResult,
  ComposerSubmission,
} from "@/components/composer/composer-model";
import {
  parseConversationBlocks,
} from "@/components/conversation/conversation-transcript";
import type {
  SkillActionProps,
  SkillSubAreaId,
} from "@/components/features/skills/SkillsArea";
import type {
  MemoryActionProps,
  MemorySubAreaId,
} from "@/components/features/memory/MemoryArea";
import {
  ProjectSidebar,
  type ConversationMenuAction,
  type ProjectMenuAction,
} from "@/components/project/ProjectSidebar";
import {
  WorkspaceSkillsSettings,
  type WorkspaceSkillsHeaderState,
} from "@/components/workspace/workspace-skills-settings";
import {
  WorkspaceMemorySettings,
  type WorkspaceMemoryHeaderState,
} from "@/components/workspace/workspace-memory-settings";
import { SidebarInset, SidebarProvider, useSidebar } from "@/components/ui/sidebar";
import {
  WorkspaceResizableBody,
  type DraftConversationSelection,
} from "@/components/workspace/workspace-resizable-body";
import type {
  DiffReviewSelection,
  WorkspaceDiffFileDetailRequest,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffSearchRequest,
  WorkspaceDiffSearchResponse,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";
import type {
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ConversationSummary,
  MainAreaId,
  PendingPromptSummary,
  ProjectSummary,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";

// WorkspaceMainView identifies the active main surface inside the workbench.
// WorkspaceMainView 标识工作台内部当前激活的主内容界面。
export type WorkspaceMainView = "chat" | "skills" | "memory";

// WorkspaceShellProps contains the visible workspace layout state and injected service edges.
// WorkspaceShellProps 包含可见工作区布局状态和注入的服务边界。
interface WorkspaceShellProps {
  // Startup snapshot shown by nested workspace panels.
  // 嵌套工作区面板展示的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot rendered by the project tree and conversation area.
  // 项目树与会话区渲染使用的工作台快照。
  workbench: WorkbenchSnapshot;
  // Active main surface inside the workbench.
  // 工作台内部当前激活的主内容界面。
  mainView: WorkspaceMainView;
  // Selected embedded skills sub page.
  // 当前选中的内嵌技能子页面。
  activeSkillSubArea: SkillSubAreaId;
  // Selected embedded memory sub page.
  // 当前选中的内嵌记忆子页面。
  activeMemorySubArea: MemorySubAreaId;
  // Draft conversation selected before a persisted session exists.
  // 持久会话创建前选中的草稿会话。
  selectedDraftConversation: DraftConversationSelection | null;
  // Selected conversation summary, when one exists.
  // 当前选中的会话摘要，如果存在。
  selectedConversation?: ConversationSummary;
  // Selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Conversation id currently being hydrated.
  // 当前正在水合的会话标识。
  loadingConversationId: string | null;
  // Last conversation loading error.
  // 最近一次会话加载错误。
  conversationLoadError: string | null;
  // Pinned conversation ids used by the project tree.
  // 项目树使用的置顶会话标识集合。
  pinnedConversationIds: Set<string>;
  // Handles one conversation context-menu action.
  // 处理单个会话右键菜单动作。
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Creates a new conversation in one project.
  // 在单个项目中创建新会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Opens the create-project flow.
  // 打开创建项目流程。
  onCreateProject: () => void;
  // Opens the skills feature area from the workbench sidebar.
  // 从工作台侧栏打开技能功能区。
  onOpenSkills: () => void;
  // Opens the memory feature area from the workbench sidebar.
  // 从工作台侧栏打开记忆功能区。
  onOpenMemory: () => void;
  // Selects one top-level application area from the sidebar brand menu.
  // 从侧栏品牌菜单选择一个顶层应用区域。
  onSelectArea: (area: MainAreaId) => void;
  // Updates the selected embedded skills sub page.
  // 更新当前选中的内嵌技能子页面。
  onSelectSkillSubArea: (subArea: SkillSubAreaId) => void;
  // Updates the selected embedded memory sub page.
  // 更新当前选中的内嵌记忆子页面。
  onSelectMemorySubArea: (subArea: MemorySubAreaId) => void;
  // Persists the expanded project id list.
  // 持久化展开的项目标识列表。
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  // Handles one project context-menu action.
  // 处理单个项目右键菜单动作。
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  // Persists project order changes.
  // 持久化项目排序变化。
  onReorderProjects: (projectIds: string[]) => void;
  // Selects one persisted conversation.
  // 选择一个持久化会话。
  onSelectConversation: (conversationId: string) => void;
  // Selects the active model for the workbench.
  // 选择工作台当前模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Selects the active reasoning effort for the workbench.
  // 选择工作台当前推理强度。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps;
  // Service-backed memory page actions.
  // 服务端支持的记忆页面动作。
  memoryActions: MemoryActionProps;
  // Fixed footer content pinned to the workspace bottom edge.
  // 固定在工作区底边的信息栏内容。
  footer?: React.ReactNode;
  // App-wide chrome actions rendered in the shared content header.
  // 渲染在共享内容标题栏中的应用级窗口控制区。
  chromeActions?: React.ReactNode;
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Routes one composer slash command.
  // 路由一个输入区斜杠命令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Submits one composer prompt.
  // 提交一个输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Persists one session-scoped AI capability draft.
  // 持久化单个会话级 AI 能力草稿。
  onSaveAiCapabilityConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
  // Stops the selected conversation run.
  // 停止当前选中会话运行。
  onStopConversationRun: () => Promise<boolean>;
  // Opens a routed subagent transcript panel.
  // 打开已路由的子代理转录面板。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Closes the routed subagent transcript panel.
  // 关闭已路由的子代理转录面板。
  onCloseSubagentPanel: () => Promise<void>;
  // Stops one subagent task.
  // 停止一个子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Edits one queued prompt.
  // 编辑一个队列提示词。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Inserts one queued prompt into the composer.
  // 将一个队列提示词插入输入区。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one queued prompt.
  // 移除一个队列提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Reorders queued prompts.
  // 重排队列提示词。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
  // Loads a fresh workspace diff snapshot.
  // 加载最新工作区 diff 快照。
  onLoadWorkspaceDiffSnapshot: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
  ) => Promise<WorkspaceDiffSnapshotResponse>;
  // Loads a fresh workspace diff detail for one file.
  // 加载单个文件的最新工作区 diff 详情。
  onLoadWorkspaceDiffFileDetail: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
    file: WorkspaceDiffFileDetailRequest,
    options?: WorkspaceDiffFileDetailLoadOptions,
  ) => Promise<WorkspaceDiffFileDetailResponse>;
  // Loads compact file-level search hits for the current workspace diff.
  // 加载当前工作区 diff 的紧凑文件级搜索命中。
  onLoadWorkspaceDiffSearch: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
    search: WorkspaceDiffSearchRequest,
  ) => Promise<WorkspaceDiffSearchResponse>;
  // Stages one browser-provided attachment through the service.
  // 通过服务暂存一个浏览器提供的附件。
  onStageAttachmentBytes: (
    projectId: string | null,
    conversationId: string | null,
    file: File,
  ) => Promise<ComposerAttachmentSummary | null>;
  // Copies text through the shell-provided clipboard path.
  // 通过外壳提供的剪贴板路径复制文本。
  onCopyText: (text: string) => Promise<void>;
  // Loads hidden tool details for transcript rows.
  // 为转录行加载隐藏工具详情。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Resolves local media paths into renderable URLs.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Emits lightweight click diagnostics.
  // 输出轻量点击诊断信息。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}

// WorkspaceShell renders the shadcn dashboard-like fused sidebar and inset console area.
// WorkspaceShell 渲染类似 shadcn dashboard 的融合侧栏与 inset 控制台区域。
export function WorkspaceShell(props: WorkspaceShellProps) {
  return (
    <SidebarProvider className="console-shell">
      <WorkspaceShellContent {...props} />
    </SidebarProvider>
  );
}

// WorkspaceShellContent renders the workspace shell after sidebar context is available.
// WorkspaceShellContent 在侧栏上下文可用后渲染工作台外壳。
function WorkspaceShellContent(props: WorkspaceShellProps) {
  // sidebar controls the left project tree that can collapse automatically.
  // sidebar 控制可自动折叠的左侧项目树。
  const sidebar = useSidebar();
  // shellRef measures the available workspace width for automatic panel collapse.
  // shellRef 测量工作台可用宽度，用于自动折叠面板。
  const shellRef = React.useRef<HTMLDivElement | null>(null);
  // previousShellWidthRef tracks resize direction so auto-restore only runs while width grows.
  // previousShellWidthRef 跟踪尺寸变化方向，确保自动恢复只在宽度增长时执行。
  const previousShellWidthRef = React.useRef<number | null>(null);
  // rightPanelOpen stores whether the review and memory panel is visible.
  // rightPanelOpen 保存审核与记忆面板是否可见。
  const [rightPanelOpen, setRightPanelOpen] = React.useState(false);
  // rightPanelFloating stores whether the right panel overlays instead of resizing the chat area.
  // rightPanelFloating 保存右侧面板是否以浮动方式覆盖而不是压缩会话区。
  const [rightPanelFloating, setRightPanelFloating] = React.useState(false);
  // leftAutoCollapsed records automatic left-sidebar collapse caused by width pressure.
  // leftAutoCollapsed 记录宽度压力导致的左侧栏自动折叠。
  const [leftAutoCollapsed, setLeftAutoCollapsed] = React.useState(false);
  // skillsHeaderState stores the title projection published by the embedded skills page.
  // skillsHeaderState 保存内嵌技能页发布的标题栏投影。
  const [skillsHeaderState, setSkillsHeaderState] = React.useState<WorkspaceSkillsHeaderState>({
    actions: null,
    detail: "LuaSkills",
    title: "技能设置",
  });
  // memoryHeaderState stores the title projection published by the embedded memory page.
  // memoryHeaderState 保存内嵌记忆页发布的标题栏投影。
  const [memoryHeaderState, setMemoryHeaderState] = React.useState<WorkspaceMemoryHeaderState>({
    actions: null,
    detail: "状态",
    title: "记忆设置",
  });
  // handleLeftPanelToggle clears automatic restore intent when the user toggles the left sidebar manually.
  // handleLeftPanelToggle 在用户手动切换左侧栏时清除自动恢复意图。
  const handleLeftPanelToggle = React.useCallback(() => {
    setLeftAutoCollapsed(false);
  }, []);
  // handleRightPanelToggle clears automatic restore intent when the user toggles the right panel manually.
  // handleRightPanelToggle 在用户手动切换右侧面板时清除自动恢复意图。
  const handleRightPanelToggle = React.useCallback(() => {
    const nextOpen = !rightPanelOpen;
    if (!nextOpen) {
      setRightPanelFloating(false);
      setRightPanelOpen(false);
      return;
    }
    const width = shellRef.current?.getBoundingClientRect().width ?? 0;
    const expandedLeftWidth = 288;
    const collapsedLeftWidth = 46;
    const preferredRightWidth = 480;
    const handleWidth = 9;
    const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
    setRightPanelFloating(width > 0 && width - leftWidth - preferredRightWidth - handleWidth < 800);
    setRightPanelOpen(true);
  }, [rightPanelOpen, sidebar.open]);

  React.useEffect(() => {
    if (props.mainView !== "chat" && rightPanelOpen) {
      setRightPanelOpen(false);
      setRightPanelFloating(false);
    }
  }, [props.mainView, rightPanelOpen]);

  React.useEffect(() => {
    const element = shellRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const width = entry.contentRect.width;
      const previousWidth = previousShellWidthRef.current;
      const isGrowing = previousWidth !== null && width > previousWidth + 1;
      previousShellWidthRef.current = width;
      const expandedLeftWidth = 288;
      const collapsedLeftWidth = 46;
      const preferredRightWidth = 480;
      const handleWidth = 9;
      const leftWidth = sidebar.open ? expandedLeftWidth : collapsedLeftWidth;
      const dockedPanelFits = width - leftWidth - preferredRightWidth - handleWidth >= 800;
      const shouldFloatRightPanel = rightPanelOpen && !dockedPanelFits;
      const dockedRightWidth = rightPanelOpen && !shouldFloatRightPanel ? preferredRightWidth : 0;
      const shellGaps = rightPanelOpen && !shouldFloatRightPanel ? handleWidth : 0;
      const mainWidth = width - leftWidth - dockedRightWidth - shellGaps;
      if (rightPanelOpen) {
        setRightPanelFloating(shouldFloatRightPanel);
      }
      if (mainWidth < 800 && sidebar.open) {
        sidebar.setOpen(false);
        setLeftAutoCollapsed(true);
        return;
      }
      if (!isGrowing) {
        return;
      }
      if (leftAutoCollapsed && !sidebar.open) {
        const restoredRightWidth = rightPanelOpen && !shouldFloatRightPanel ? preferredRightWidth : 0;
        const restoredShellGaps = rightPanelOpen && !shouldFloatRightPanel ? handleWidth : 0;
        const restoredMainWidth = width - expandedLeftWidth - restoredRightWidth - restoredShellGaps;
        if (restoredMainWidth >= 800) {
          sidebar.setOpen(true);
          setLeftAutoCollapsed(false);
          return;
        }
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [leftAutoCollapsed, rightPanelFloating, rightPanelOpen, sidebar]);

  return (
    <div className="workspace-panel-shell" ref={shellRef}>
      <ProjectSidebar
        projects={props.workbench.projects}
        conversations={props.workbench.conversations}
        expandedProjectIds={props.workbench.expandedProjectIds}
        pinnedConversationIds={props.pinnedConversationIds}
        selectedConversationId={props.selectedConversationId}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onCreateProject={props.onCreateProject}
        onOpenSkills={props.onOpenSkills}
        onOpenMemory={props.onOpenMemory}
        onSelectArea={props.onSelectArea}
        memoryActive={props.mainView === "memory"}
        skillsActive={props.mainView === "skills"}
        onExpandedProjectIdsChange={props.onExpandedProjectIdsChange}
        onProjectMenuAction={props.onProjectMenuAction}
        onReorderProjects={props.onReorderProjects}
        onSelectConversation={props.onSelectConversation}
      />
      <SidebarInset className="workspace-inset">
        <WorkspaceResizableBody
          rightPanelFloating={rightPanelFloating}
          rightPanelOpen={rightPanelOpen}
          startup={props.startup}
          workbench={props.workbench}
          selectedDraftConversation={props.selectedDraftConversation}
          selectedConversation={props.selectedConversation}
          selectedConversationId={props.selectedConversationId}
          headerActions={props.mainView === "skills" ? skillsHeaderState.actions : props.mainView === "memory" ? memoryHeaderState.actions : undefined}
          chromeActions={props.chromeActions}
          titlebarDragRegion={props.titlebarDragRegion}
          onTitlebarDoubleClick={props.onTitlebarDoubleClick}
          headerDetail={props.mainView === "skills" ? skillsHeaderState.detail : props.mainView === "memory" ? memoryHeaderState.detail : undefined}
          headerTitle={props.mainView === "skills" ? skillsHeaderState.title : props.mainView === "memory" ? memoryHeaderState.title : undefined}
          hideRightPanelToggle={props.mainView !== "chat"}
          onToggleLeftPanel={handleLeftPanelToggle}
          onToggleRightPanel={handleRightPanelToggle}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
          onCloseSubagentPanel={props.onCloseSubagentPanel}
          onStopSubagentTask={props.onStopSubagentTask}
          onCopyRawDiff={(rawDiff) => void props.onCopyText(rawDiff)}
          parseConversationBlocks={parseConversationBlocks}
          loadWorkspaceDiffFileDetail={props.onLoadWorkspaceDiffFileDetail}
          loadWorkspaceDiffSearch={props.onLoadWorkspaceDiffSearch}
          loadWorkspaceDiffSnapshot={props.onLoadWorkspaceDiffSnapshot}
          onRunVmmAction={props.memoryActions.onRunVmmAction}
          onTrace={props.onTrace}
        >
          {props.mainView === "skills" ? (
            <WorkspaceSkillsSettings
              activeSubArea={props.activeSkillSubArea}
              onActiveSubAreaChange={props.onSelectSkillSubArea}
              onHeaderStateChange={setSkillsHeaderState}
              skillActions={props.skillActions}
              workbench={props.workbench}
            />
          ) : props.mainView === "memory" ? (
            <WorkspaceMemorySettings
              activeSubArea={props.activeMemorySubArea}
              onActiveSubAreaChange={props.onSelectMemorySubArea}
              onHeaderStateChange={setMemoryHeaderState}
              memoryActions={props.memoryActions}
              workbench={props.workbench}
            />
          ) : (
            <ChatArea
              startup={props.startup}
              workbench={props.workbench}
              selectedDraftConversation={props.selectedDraftConversation}
              selectedConversationId={props.selectedConversationId}
              loadingConversationId={props.loadingConversationId}
              conversationLoadError={props.conversationLoadError}
              onSelectWorkbenchModel={props.onSelectWorkbenchModel}
              onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
              onSlashCommand={props.onSlashCommand}
              onSubmitPrompt={props.onSubmitPrompt}
              onSaveAiCapabilityConfiguration={props.onSaveAiCapabilityConfiguration}
              onStopConversationRun={props.onStopConversationRun}
              onOpenSubagentPanel={props.onOpenSubagentPanel}
              onCloseSubagentPanel={props.onCloseSubagentPanel}
              onStopSubagentTask={props.onStopSubagentTask}
              onEditPendingPrompt={props.onEditPendingPrompt}
              onInsertPendingPrompt={props.onInsertPendingPrompt}
              onRemovePendingPrompt={props.onRemovePendingPrompt}
              onReorderPendingPrompts={props.onReorderPendingPrompts}
              onStageAttachmentBytes={props.onStageAttachmentBytes}
              onCopyText={props.onCopyText}
              onLoadToolDetail={props.onLoadToolDetail}
              resolveMediaSourceUrl={props.resolveMediaSourceUrl}
              onTrace={props.onTrace}
            />
          )}
        </WorkspaceResizableBody>
        {props.footer ? (
          <div className="workspace-statusbar-host">
            {props.footer}
          </div>
        ) : null}
      </SidebarInset>
    </div>
  );
}
