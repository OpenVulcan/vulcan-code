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
  Folder,
  FolderPlus,
  Pin,
  SquarePen,
} from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebarNavigation,
} from "@/ui-kit/shell/sidebar";
import { SidebarBrandMenu } from "@/components/sidebar/sidebar-profile-menus";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ConversationSummary, ProjectSummary } from "@/types";
import { ConversationTreeRow } from "./project-sidebar-conversation-row";
import { SortableProjectRow } from "./project-sidebar-project-row";
import type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";

export type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";

// ProjectSessionTreeProps describes the session tree shared by the session panel and the rail's session flyout.
// ProjectSessionTreeProps 描述会话面板与轨道会话浮层共用的会话树。
export type ProjectSessionTreeProps = {
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
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  // Selects one project without selecting a conversation.
  // 选择一个项目且不选择会话。
  onSelectProject: (projectId: string) => void;
  onSelectConversation: (conversationId: string) => void;
};

// ProjectSidebar renders the session panel: the brand strip above the shared session tree.
// ProjectSidebar 渲染会话面板：品牌条及其下方的共用会话树。
export function ProjectSidebar(props: ProjectSessionTreeProps & {
  // Always-visible activity rail rendered beside this panel.
  // 渲染在此面板旁、始终可见的活动轨道。
  rail: React.ReactNode;
  // Whether the sidebar brand content should be a native window drag region.
  // 侧栏品牌内容是否作为原生窗口拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
}) {
  const { t } = useI18n();
  const { rail, titlebarDragRegion, onTitlebarDoubleClick, ...treeProps } = props;
  return (
    <Sidebar collapsible="offcanvas" rail={rail} variant="inset">
      <SidebarHeader>
        <SidebarBrandMenu
          currentLabel={t("sidebar.console")}
          titlebarDragRegion={titlebarDragRegion}
          onTitlebarDoubleClick={onTitlebarDoubleClick}
        />
      </SidebarHeader>
      <SidebarContent>
        <ProjectSessionTree {...treeProps} />
      </SidebarContent>
    </Sidebar>
  );
}

// ProjectSessionTree renders pinned conversations, projects, and their conversations with the panel's own rows,
// icons and expansion, so every surface that lists sessions looks and behaves the same.
// ProjectSessionTree 使用面板自身的行、图标与展开状态渲染置顶会话、项目及其会话，
// 使所有列出会话的界面外观与行为一致。
export function ProjectSessionTree(props: ProjectSessionTreeProps) {
  // t translates stable sidebar labels while project names remain user data.
  // t 翻译稳定侧栏标签，项目名称仍保留为用户数据。
  const { t, locale } = useI18n();
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
  // listedConversations excludes sessions floating as assistants; they live in the top-bar assistant dock.
  // listedConversations 排除作为助手浮动的会话；它们位于顶部助手停靠中。
  const listedConversations = React.useMemo(
    () => props.conversations.filter((conversation) => !conversation.assistantName),
    [props.conversations],
  );
  // conversationsByProject groups conversations beneath their owning project folder.
  // conversationsByProject 将会话归组到所属项目文件夹下。
  const conversationsByProject = React.useMemo(() => {
    return listedConversations.reduce<Record<string, ConversationSummary[]>>((groups, conversation) => {
      const projectId = conversation.projectId;
      if (!projectId || !projectIds.has(projectId)) {
        return groups;
      }
      groups[projectId] = groups[projectId] ?? [];
      groups[projectId].push(conversation);
      return groups;
    }, {});
  }, [projectIds, listedConversations]);
  // unlinkedConversations keeps conversations without a valid project owner in a diagnostic group.
  // unlinkedConversations 将没有有效项目归属的会话保留在诊断分组中。
  const unlinkedConversations = React.useMemo(() => {
    return listedConversations.filter((conversation) => {
      if (props.pinnedConversationIds.has(conversation.id)) {
        return false;
      }
      return !conversation.projectId || !projectIds.has(conversation.projectId);
    });
  }, [projectIds, listedConversations, props.pinnedConversationIds]);
  // sortedConversationsByProject keeps the service-provided recency order for each project.
  // sortedConversationsByProject 保留服务端提供的项目内最近活动顺序。
  const sortedConversationsByProject = React.useMemo(() => {
    return conversationsByProject;
  }, [conversationsByProject]);
  // pinnedConversations renders conversation pins in an independent section above pinned projects.
  // pinnedConversations 将置顶会话渲染到置顶项目上方的独立分区。
  const pinnedConversations = React.useMemo(() => {
    return listedConversations.filter((conversation) => props.pinnedConversationIds.has(conversation.id));
  }, [listedConversations, props.pinnedConversationIds]);
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
  // globalDraftProject opens a project-agnostic draft whose owner is then chosen from the composer chip.
  // globalDraftProject 打开与项目无关的草稿，其归属随后由输入区标签选择。
  const globalDraftProject = props.projects.find((project) => project.id === props.activeProjectId)
    ?? props.projects[0]
    ?? null;
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
        hasSelectedConversation={props.selectedConversationId ? projectConversationIds.has(props.selectedConversationId) : false}
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
      isSelected={props.selectedConversationId === conversation.id}
      key={conversation.id}
      locale={locale}
      nowMs={nowMs}
      onAction={props.onConversationMenuAction}
      onSelectConversation={navigate(props.onSelectConversation)}
      project={conversation.projectId ? projectsById.get(conversation.projectId) : undefined}
    />
  );

  return (
    <div className={cn("project-session-tree", activeProjectDragId && "project-drag-active")}>
        {pinnedConversations.length > 0 ? (
          <SidebarGroup className="project-section pinned-project-section">
            {renderProjectGroupLabel(t("sidebar.pinned"), <Pin size={15} />)}
            <SidebarMenu className="pinned-tree-list">
              {pinnedConversations.map((conversation) => renderConversationRow(conversation, true))}
            </SidebarMenu>
          </SidebarGroup>
        ) : null}
        <SidebarGroup>
            {renderProjectGroupLabel(t("sidebar.projects"), <Boxes size={15} />, (
              <span className="inline-flex items-center gap-0.5">
              {globalDraftProject ? (
                <Button
                  aria-label={t("sidebar.newSession")}
                  className="project-add-button"
                  onClick={() => props.onCreateConversation(globalDraftProject)}
                  size="icon-sm"
                  title={t("sidebar.newSession")}
                  type="button"
                  variant="ghost"
                >
                  <SquarePen size={15} />
                </Button>
              ) : null}
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
              </span>
            ))}
            <SidebarMenu>
              {renderSortableProjectSection(props.projects)}
              {props.projects.length === 0 ? (
                <SidebarMenuItem>
                  {/* WorkbenchApp mounts this sidebar only after the authoritative snapshot exists. */}
                  {/* WorkbenchApp 仅在权威快照存在后挂载此侧栏。 */}
                  <SidebarMenuButton className="muted" tooltip={t("sidebar.addProject")} onClick={navigate(props.onCreateProject)}>
                    <FolderPlus size={16} />
                    <span className="vk-side-text">
                      <strong>{t("sidebar.noProjects")}</strong>
                      <small>{t("project.sidebar.addAFolderTo")}</small>
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
    </div>
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
