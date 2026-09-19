import React from "react";
import {
  closestCenter,
  DndContext,
  PointerSensor,
  KeyboardSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  verticalListSortingStrategy,
  sortableKeyboardCoordinates,
} from "@dnd-kit/sortable";
import {
  Boxes,
  Bot,
  BrainCircuit,
  BookMarked,
  Folder,
  FolderPlus,
  Hammer,
  Pin,
  Puzzle,
  Settings,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
  useSidebarNavigation,
} from "@/components/ui/sidebar";
import {
  SidebarBrandMenu,
  SidebarUserMenu,
} from "@/components/sidebar/sidebar-profile-menus";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ConversationSummary, MainAreaId, ProjectSummary } from "@/types";
import { ConversationTreeRow } from "./project-sidebar-conversation-row";
import { CollapsedFeatureMenu } from "./project-sidebar-menus";
import { SortableProjectRow } from "./project-sidebar-project-row";
import type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";

export type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";

// ProjectSidebar renders projects and conversations.
// ProjectSidebar 渲染项目与会话。
export function ProjectSidebar(props: {
  projects: ProjectSummary[];
  conversations: ConversationSummary[];
  expandedProjectIds?: string[];
  pinnedConversationIds: Set<string>;
  selectedConversationId: string | null;
  // Explicit project that owns project-level floating tools.
  // 持有项目级浮动工具的明确项目。
  activeProjectId: string | null;
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  onCreateConversation: (project: ProjectSummary) => void;
  onCreateProject: () => void;
  // Opens the skills feature area from the workbench sidebar.
  // 从工作台侧栏打开技能功能区。
  onOpenSkills: () => void;
  // Opens the prompt skills feature area from the workbench sidebar.
  // 从工作台侧栏打开提示词技能功能区。
  onOpenAgentSkills: () => void;
  // Opens the agents feature area from the workbench sidebar.
  // 从工作台侧栏打开智能体功能区。
  onOpenAgents: () => void;
  // Opens the memory feature area from the workbench sidebar.
  // 从工作台侧栏打开记忆功能区。
  onOpenMemory: () => void;
  // Opens the knowledge library management area from the workbench sidebar.
  // 从工作台侧栏打开知识库管理区域。
  onOpenKnowledge: () => void;
  // Selects one top-level application area from the sidebar brand menu.
  // 从侧栏品牌菜单选择一个顶层应用区域。
  onSelectArea: (area: MainAreaId) => void;
  // Whether the sidebar brand content should be a native window drag region.
  // 侧栏品牌内容是否作为原生窗口拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Whether the embedded skills settings view is currently active.
  // 内嵌技能设置视图当前是否激活。
  skillsActive: boolean;
  // Whether the prompt skills management view is currently active.
  // 提示词技能管理视图当前是否激活。
  agentSkillsActive: boolean;
  // Whether the embedded agents settings view is currently active.
  // 内嵌智能体设置视图当前是否激活。
  agentsActive: boolean;
  // Whether the embedded memory settings view is currently active.
  // 内嵌记忆设置视图当前是否激活。
  memoryActive: boolean;
  // Whether the embedded knowledge library management view is currently active.
  // 内嵌知识库管理视图当前是否激活。
  knowledgeActive: boolean;
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  // Selects one project without selecting a conversation.
  // 选择一个项目且不选择会话。
  onSelectProject: (projectId: string) => void;
  onSelectConversation: (conversationId: string) => void;
}) {
  // t translates stable sidebar labels while project names remain user data.
  // t 翻译稳定侧栏标签，项目名称仍保留为用户数据。
  const { t, locale } = useI18n();
  // sidebarState selects the expanded tree interaction or the collapsed project pop menu.
  // sidebarState 选择展开树交互或折叠项目弹出菜单交互。
  const { state: sidebarState } = useSidebar();
  // navigate preserves each destination action and closes phone navigation.
  // navigate 保留各导航动作并关闭手机导航。
  const navigate = useSidebarNavigation();
  // openProjectIds stores the project folders currently expanded in the tree.
  // openProjectIds 保存当前在树中展开的项目文件夹。
  const [openProjectIds, setOpenProjectIds] = React.useState<Set<string>>(
    () => initialExpandedProjectIdSet(props.projects, props.expandedProjectIds),
  );
  // openProjectIdsRef lets event handlers serialize rapid toggles without reading state during an updater.
  // openProjectIdsRef 让事件处理器在不于 updater 中读取状态的情况下串行化快速切换。
  const openProjectIdsRef = React.useRef(openProjectIds);
  // activeProjectDragId records when project sorting is active so hover-only UI can pause.
  // activeProjectDragId 记录项目排序激活状态，以便暂停仅悬停显示的界面。
  const [activeProjectDragId, setActiveProjectDragId] = React.useState<string | null>(null);
  // expandedConversationProjectIds stores project ids whose conversation list is fully expanded.
  // expandedConversationProjectIds 保存会话列表已完整展开的项目标识。
  const [expandedConversationProjectIds, setExpandedConversationProjectIds] = React.useState<Set<string>>(() => new Set());
  // projectDragSensors admits pointer movement and keyboard sorting through the dedicated handle.
  // projectDragSensors 通过专用手柄准入指针移动及键盘排序。
  const projectDragSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  // projectIds stores the explicit project identifiers available in this snapshot.
  // projectIds 保存当前快照中明确存在的项目标识。
  const projectIds = React.useMemo(() => new Set(props.projects.map((project) => project.id)), [props.projects]);
  // projectsById resolves conversation project icons without changing conversation data.
  // projectsById 在不改变会话数据的情况下解析会话所属项目图标。
  const projectsById = React.useMemo(() => {
    return new Map(props.projects.map((project) => [project.id, project]));
  }, [props.projects]);
  // conversationsByProject groups conversations beneath their owning project folder.
  // conversationsByProject 将会话归组到所属项目文件夹下。
  const conversationsByProject = React.useMemo(() => {
    return props.conversations.reduce<Record<string, ConversationSummary[]>>((groups, conversation) => {
      const projectId = conversation.projectId;
      if (!projectId || !projectIds.has(projectId)) {
        return groups;
      }
      groups[projectId] = groups[projectId] ?? [];
      groups[projectId].push(conversation);
      return groups;
    }, {});
  }, [projectIds, props.conversations]);
  // unlinkedConversations keeps conversations without a valid project owner in a diagnostic group.
  // unlinkedConversations 将没有有效项目归属的会话保留在诊断分组中。
  const unlinkedConversations = React.useMemo(() => {
    return props.conversations.filter((conversation) => {
      if (props.pinnedConversationIds.has(conversation.id)) {
        return false;
      }
      return !conversation.projectId || !projectIds.has(conversation.projectId);
    });
  }, [projectIds, props.conversations, props.pinnedConversationIds]);
  // sortedConversationsByProject keeps the service-provided recency order for each project.
  // sortedConversationsByProject 保留服务端提供的项目内最近活动顺序。
  const sortedConversationsByProject = React.useMemo(() => {
    return conversationsByProject;
  }, [conversationsByProject]);
  // pinnedConversations renders conversation pins in an independent section above pinned projects.
  // pinnedConversations 将置顶会话渲染到置顶项目上方的独立分区。
  const pinnedConversations = React.useMemo(() => {
    return props.conversations.filter((conversation) => props.pinnedConversationIds.has(conversation.id));
  }, [props.conversations, props.pinnedConversationIds]);
  // pinnedProjects mirrors the WebUI task tree section split.
  // pinnedProjects 对齐 WebUI 任务树中的置顶分区拆分。
  const pinnedProjects = React.useMemo(() => props.projects.filter((project) => project.pinned), [props.projects]);
  // regularProjects keeps non-pinned projects in their service-provided order.
  // regularProjects 保留普通项目在服务端快照中的原始顺序。
  const regularProjects = React.useMemo(() => props.projects.filter((project) => !project.pinned), [props.projects]);
  // featureSurfaceActive tracks whether the global sidebar selection is a non-chat feature page.
  // featureSurfaceActive 跟踪全局侧栏选择是否处于非会话功能页面。
  const featureSurfaceActive = props.skillsActive || props.agentSkillsActive || props.agentsActive || props.memoryActive || props.knowledgeActive;
  // visibleProjectRows keeps pinned projects inline while the sidebar is expanded.
  // visibleProjectRows 在侧栏展开时将置顶项目保留在常规项目列表内。
  const visibleProjectRows = sidebarState === "collapsed" ? regularProjects : props.projects;
  // visiblePinnedProjects exposes project pin state only in collapsed icon navigation.
  // visiblePinnedProjects 仅在折叠图标导航中暴露项目置顶状态。
  const visiblePinnedProjects = sidebarState === "collapsed" ? pinnedProjects : [];
  // nowMs refreshes relative time labels without touching hover or confirmation state.
  // nowMs 刷新相对时间标签且不触碰悬停或确认状态。
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  React.useEffect(() => {
    const intervalId = window.setInterval(() => setNowMs(Date.now()), 60_000);
    return () => window.clearInterval(intervalId);
  }, []);
  React.useEffect(() => {
    // next is the externally authoritative expansion projection for this render.
    // next 是本次渲染对应的外部权威展开投影。
    const next = initialExpandedProjectIdSet(props.projects, props.expandedProjectIds);
    openProjectIdsRef.current = next;
    setOpenProjectIds(next);
  }, [props.expandedProjectIds, props.projects]);
  React.useEffect(() => {
    const visibleProjectIds = new Set(props.projects.map((project) => project.id));
    setExpandedConversationProjectIds((current) => {
      const next = new Set([...current].filter((projectId) => visibleProjectIds.has(projectId)));
      return next.size === current.size ? current : next;
    });
  }, [props.projects]);
  // handleProjectDragEnd persists one dnd-kit same-section reorder result.
  // handleProjectDragEnd 持久化一次 dnd-kit 同分区项目排序结果。
  const handleProjectDragEnd = React.useCallback((sectionProjects: ProjectSummary[], event: DragEndEvent) => {
    setActiveProjectDragId(null);
    if (!event.over || event.active.id === event.over.id) {
      return;
    }
    const sectionIds = sectionProjects.map((project) => project.id);
    const activeIndex = sectionIds.indexOf(String(event.active.id));
    const overIndex = sectionIds.indexOf(String(event.over.id));
    if (activeIndex < 0 || overIndex < 0) {
      return;
    }
    props.onReorderProjects(arrayMove(sectionIds, activeIndex, overIndex));
  }, [props]);
  // toggleProjectOpen toggles one project folder without changing the selected conversation.
  // toggleProjectOpen 切换单个项目文件夹展开状态且不改变当前会话选择。
  const toggleProjectOpen = React.useCallback((projectId: string) => {
    // next is the event-ordered expansion result used for both local state and persistence.
    // next 是按事件顺序得到的展开结果，同时用于本地状态和持久化。
    const next = new Set(openProjectIdsRef.current);
    if (next.has(projectId)) {
      next.delete(projectId);
    } else {
      next.add(projectId);
    }
    // Keep the state updater pure; parent notification belongs to this user event boundary.
    // 保持状态 updater 纯净；父组件通知属于当前用户事件边界。
    openProjectIdsRef.current = next;
    setOpenProjectIds(next);
    props.onExpandedProjectIdsChange(expandedProjectIdsInProjectOrder(props.projects, next));
  }, [props.onExpandedProjectIdsChange, props.projects]);
  // toggleProjectConversations toggles whether one project shows every conversation row.
  // toggleProjectConversations 切换单个项目是否显示全部会话行。
  const toggleProjectConversations = React.useCallback((projectId: string) => {
    setExpandedConversationProjectIds((current) => {
      const next = new Set(current);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      return next;
    });
  }, []);
  // renderProjectGroupLabel renders the pinned/project heading with optional actions.
  // renderProjectGroupLabel 渲染置顶/项目标题，并按需显示操作入口。
  const renderProjectGroupLabel = (label: string, icon: React.ReactNode, action?: React.ReactNode) => (
    <SidebarGroupLabel className="project-group-label">
      <span className="project-group-label-main">{icon}<span className="project-group-label-text">{label}</span></span>
      {action}
    </SidebarGroupLabel>
  );
  // renderProjectRows renders one project section while preserving the shared row behavior.
  // renderProjectRows 渲染单个项目分区并保留共享项目行行为。
  const renderProjectRows = (projects: ProjectSummary[]) => projects.map((project) => {
    const projectConversations = (sortedConversationsByProject[project.id] ?? [])
      .filter((conversation) => !props.pinnedConversationIds.has(conversation.id));
    const projectConversationIds = new Set((sortedConversationsByProject[project.id] ?? []).map((conversation) => conversation.id));
    return (
      <SortableProjectRow
        conversations={projectConversations}
        hasSelectedConversation={!featureSurfaceActive && props.selectedConversationId ? projectConversationIds.has(props.selectedConversationId) : false}
        isActiveProject={props.activeProjectId === project.id}
        isOpen={openProjectIds.has(project.id)}
        isConversationListExpanded={expandedConversationProjectIds.has(project.id)}
        isPinnedConversation={(conversationId) => props.pinnedConversationIds.has(conversationId)}
        key={project.id}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={navigate(props.onCreateConversation)}
        onProjectMenuAction={props.onProjectMenuAction}
        renderConversationRow={renderConversationRow}
        onSelectConversation={navigate(props.onSelectConversation)}
        onSelectProject={props.onSelectProject}
        onToggleConversationList={toggleProjectConversations}
        onToggleOpen={toggleProjectOpen}
        project={project}
        selectedConversationId={props.selectedConversationId}
        sidebarState={sidebarState}
      />
    );
  });
  // renderSortableProjectSection renders one dnd-kit sortable project section.
  // renderSortableProjectSection 渲染一个 dnd-kit 可排序项目分区。
  const renderSortableProjectSection = (projects: ProjectSummary[]) => (
    <DndContext
      collisionDetection={closestCenter}
      onDragCancel={() => setActiveProjectDragId(null)}
      onDragEnd={(event) => handleProjectDragEnd(projects, event)}
      onDragStart={(event) => setActiveProjectDragId(String(event.active.id))}
      sensors={projectDragSensors}
    >
      <SortableContext items={projects.map((project) => project.id)} strategy={verticalListSortingStrategy}>
        {renderProjectRows(projects)}
      </SortableContext>
    </DndContext>
  );
  // renderConversationRow renders a sidebar conversation row outside or inside one project folder.
  // renderConversationRow 渲染项目内外复用的侧栏会话行。
  const renderConversationRow = (conversation: ConversationSummary, compactPinned = false) => (
    <ConversationTreeRow
      compactPinned={compactPinned}
      conversation={conversation}
      isPinned={props.pinnedConversationIds.has(conversation.id)}
      isSelected={!featureSurfaceActive && props.selectedConversationId === conversation.id}
      key={conversation.id}
      locale={locale}
      nowMs={nowMs}
      onAction={props.onConversationMenuAction}
      onSelectConversation={navigate(props.onSelectConversation)}
      project={conversation.projectId ? projectsById.get(conversation.projectId) : undefined}
    />
  );
  const collapsedFeatureMenuItems = [
    { id: "skills", label: t("area.skills"), icon: <Puzzle size={16} />, active: props.skillsActive, onSelect: navigate(props.onOpenSkills) },
    { id: "agent-skills", label: t("area.agentSkills"), icon: <Hammer size={16} />, active: props.agentSkillsActive, onSelect: navigate(props.onOpenAgentSkills) },
    { id: "agents", label: t("area.agents"), icon: <Bot size={16} />, active: props.agentsActive, onSelect: navigate(props.onOpenAgents) },
    { id: "memory", label: t("area.memory"), icon: <BrainCircuit size={16} />, active: props.memoryActive, onSelect: navigate(props.onOpenMemory) },
    { id: "knowledge", label: t("area.knowledge"), icon: <BookMarked size={16} />, active: props.knowledgeActive, onSelect: navigate(props.onOpenKnowledge) },
  ];

  return (
    <Sidebar className={cn(activeProjectDragId && "project-drag-active")} collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarBrandMenu
          activeArea={props.agentSkillsActive ? "agent-skills" : props.agentsActive ? "agents" : props.skillsActive ? "skills" : props.memoryActive ? "memory" : props.knowledgeActive ? "knowledge" : "chat"}
          currentLabel={props.agentSkillsActive ? t("area.agentSkills") : props.agentsActive ? t("area.agents") : props.skillsActive ? t("area.skills") : props.memoryActive ? t("area.memory") : props.knowledgeActive ? t("area.knowledge") : t("sidebar.console")}
          onSelectArea={navigate(props.onSelectArea)}
          titlebarDragRegion={props.titlebarDragRegion}
          onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        />
        {sidebarState === "collapsed" ? (
          <SidebarMenu>
            <SidebarMenuItem>
              <CollapsedFeatureMenu items={collapsedFeatureMenuItems} label={t("area.skills")}>
                <SidebarMenuButton
                  active={featureSurfaceActive}
                  aria-label={t("area.skills")}
                  className="workspace-sidebar-feature-button collapsed-feature-menu-button"
                >
                  <Puzzle size={16} />
                </SidebarMenuButton>
              </CollapsedFeatureMenu>
            </SidebarMenuItem>
          </SidebarMenu>
        ) : <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              active={props.skillsActive}
              className="workspace-sidebar-feature-button"
              onClick={navigate(props.onOpenSkills)}
              tooltip={t("area.skills")}
            >
              <Puzzle size={16} />
              <span className="vcn-sidebar-label workspace-sidebar-feature-label">{t("area.skills")}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              active={props.agentSkillsActive}
              className="workspace-sidebar-feature-button"
              onClick={navigate(props.onOpenAgentSkills)}
              tooltip={t("area.agentSkills")}
            >
              <Hammer size={16} />
              <span className="vcn-sidebar-label workspace-sidebar-feature-label">{t("area.agentSkills")}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              active={props.agentsActive}
              className="workspace-sidebar-feature-button"
              onClick={navigate(props.onOpenAgents)}
              tooltip={t("area.agents")}
            >
              <Bot size={16} />
              <span className="vcn-sidebar-label workspace-sidebar-feature-label">{t("area.agents")}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              active={props.memoryActive}
              className="workspace-sidebar-feature-button"
              onClick={navigate(props.onOpenMemory)}
              tooltip={t("area.memory")}
            >
              <BrainCircuit size={16} />
              <span className="vcn-sidebar-label workspace-sidebar-feature-label">{t("area.memory")}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              active={props.knowledgeActive}
              className="workspace-sidebar-feature-button"
              onClick={navigate(props.onOpenKnowledge)}
              tooltip={t("area.knowledge")}
            >
              <BookMarked size={16} />
              <span className="vcn-sidebar-label workspace-sidebar-feature-label">{t("area.knowledge")}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>}
      </SidebarHeader>
      <SidebarContent>
        {pinnedConversations.length > 0 || visiblePinnedProjects.length > 0 ? (
          <SidebarGroup className="project-section pinned-project-section">
            {renderProjectGroupLabel(t("sidebar.pinned"), <Pin size={15} />)}
            <SidebarMenu className="pinned-tree-list">
              {pinnedConversations.map((conversation) => renderConversationRow(conversation, true))}
              {visiblePinnedProjects.length > 0 ? renderSortableProjectSection(visiblePinnedProjects) : null}
            </SidebarMenu>
          </SidebarGroup>
        ) : null}
        <SidebarGroup>
            {renderProjectGroupLabel(t("sidebar.projects"), <Boxes size={15} />, (
              <Button
                aria-label={t("sidebar.addProject")}
                className="project-add-button"
                onClick={props.onCreateProject}
                size="icon-sm"
                title={t("sidebar.addProject")}
                type="button"
                variant="ghost"
              >
                <FolderPlus size={15} />
              </Button>
            ))}
            <SidebarMenu>
              {renderSortableProjectSection(visibleProjectRows)}
              {props.projects.length === 0 ? (
                <SidebarMenuItem>
                  {/* WorkbenchApp mounts this sidebar only after the authoritative snapshot exists. */}
                  {/* WorkbenchApp 仅在权威快照存在后挂载此侧栏。 */}
                  <SidebarMenuButton className="muted" tooltip={t("sidebar.addProject")} onClick={navigate(props.onCreateProject)}>
                    <FolderPlus size={16} />
                    <span className="vcn-sidebar-label">
                      <strong>{t("sidebar.noProjects")}</strong>
                      <small>{locale === "zh-CN" ? "添加文件夹，开始一个项目" : "Add a folder to start a project"}</small>
                    </span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ) : null}
            </SidebarMenu>
          </SidebarGroup>
        {unlinkedConversations.length > 0 ? (
          <SidebarGroup className="project-section unlinked-conversation-section">
            {renderProjectGroupLabel(t("sidebar.unlinkedConversations"), <Folder size={15} />)}
            <SidebarMenu className="unlinked-conversation-list">
              {unlinkedConversations.map((conversation) => renderConversationRow(conversation))}
            </SidebarMenu>
          </SidebarGroup>
        ) : null}
      </SidebarContent>
      <SidebarFooter>
        {sidebarState === "collapsed" ? (
          <SidebarMenu>
            {/* The collapsed settings shortcut stays immediately above the user menu. */}
            {/* 折叠态设置快捷入口紧贴用户菜单上方。 */}
            <SidebarMenuItem>
              <SidebarMenuButton
                aria-label={t("area.settings")}
                className="workspace-sidebar-feature-button"
                onClick={() => navigate(props.onSelectArea)("settings")}
                tooltip={t("area.settings")}
              >
                <Settings size={16} />
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        ) : null}
        <SidebarUserMenu onOpenSettings={() => navigate(props.onSelectArea)("settings")} />
      </SidebarFooter>
    </Sidebar>
  );
}

// initialExpandedProjectIdSet restores project expansion, defaulting missing state to fully expanded.
// initialExpandedProjectIdSet 恢复项目展开状态，并在缺少状态时默认全部展开。
function initialExpandedProjectIdSet(
  projects: ProjectSummary[],
  expandedProjectIds: string[] | undefined,
): Set<string> {
  if (!expandedProjectIds) {
    return new Set(projects.map((project) => project.id));
  }
  const projectIds = new Set(projects.map((project) => project.id));
  return new Set(expandedProjectIds.filter((projectId) => projectIds.has(projectId)));
}

// expandedProjectIdsInProjectOrder serializes expanded project ids in the visible project order.
// expandedProjectIdsInProjectOrder 按可见项目顺序序列化已展开项目标识。
function expandedProjectIdsInProjectOrder(
  projects: ProjectSummary[],
  openProjectIds: Set<string>,
): string[] {
  return projects
    .filter((project) => openProjectIds.has(project.id))
    .map((project) => project.id);
}
