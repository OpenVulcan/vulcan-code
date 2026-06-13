import React from "react";
import {
  closestCenter,
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import {
  Boxes,
  Folder,
  FolderPlus,
  LayoutDashboard,
  Pin,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ConversationSummary, ProjectSummary } from "@/types";
import { ConversationTreeRow } from "./project-sidebar-conversation-row";
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
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  onCreateConversation: (project: ProjectSummary) => void;
  onCreateProject: () => void;
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  onReorderProjects: (projectIds: string[]) => void;
  onSelectConversation: (conversationId: string) => void;
}) {
  // t translates stable sidebar labels while project names remain user data.
  // t 翻译稳定侧栏标签，项目名称仍保留为用户数据。
  const { t, locale } = useI18n();
  // sidebarState selects the expanded tree interaction or the collapsed project pop menu.
  // sidebarState 选择展开树交互或折叠项目弹出菜单交互。
  const { state: sidebarState } = useSidebar();
  // openProjectIds stores the project folders currently expanded in the tree.
  // openProjectIds 保存当前在树中展开的项目文件夹。
  const [openProjectIds, setOpenProjectIds] = React.useState<Set<string>>(
    () => initialExpandedProjectIdSet(props.projects, props.expandedProjectIds),
  );
  // activeProjectDragId records when project sorting is active so hover-only UI can pause.
  // activeProjectDragId 记录项目排序激活状态，以便暂停仅悬停显示的界面。
  const [activeProjectDragId, setActiveProjectDragId] = React.useState<string | null>(null);
  // expandedConversationProjectIds stores project ids whose conversation list is fully expanded.
  // expandedConversationProjectIds 保存会话列表已完整展开的项目标识。
  const [expandedConversationProjectIds, setExpandedConversationProjectIds] = React.useState<Set<string>>(() => new Set());
  // projectDragSensors enables pointer dragging only after a small movement threshold.
  // projectDragSensors 仅在达到小幅移动阈值后启用指针拖拽。
  const projectDragSensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  // conversationsByProject groups conversations beneath their owning project folder.
  // conversationsByProject 将会话归组到所属项目文件夹下。
  const conversationsByProject = React.useMemo(() => {
    const fallbackProjectId = props.projects[0]?.id ?? "default-project";
    return props.conversations.reduce<Record<string, ConversationSummary[]>>((groups, conversation) => {
      const projectId = conversation.projectId ?? fallbackProjectId;
      groups[projectId] = groups[projectId] ?? [];
      groups[projectId].push(conversation);
      return groups;
    }, {});
  }, [props.conversations, props.projects]);
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
  // projectsById resolves pinned conversation project icons without changing conversation data.
  // projectsById 在不改变会话数据的情况下解析置顶会话所属项目图标。
  const projectsById = React.useMemo(() => {
    return new Map(props.projects.map((project) => [project.id, project]));
  }, [props.projects]);
  // pinnedProjects mirrors the iced task tree section split.
  // pinnedProjects 对齐 iced 任务树中的置顶分区拆分。
  const pinnedProjects = React.useMemo(() => props.projects.filter((project) => project.pinned), [props.projects]);
  // regularProjects keeps non-pinned projects in their service-provided order.
  // regularProjects 保留普通项目在服务端快照中的原始顺序。
  const regularProjects = React.useMemo(() => props.projects.filter((project) => !project.pinned), [props.projects]);
  // nowMs refreshes relative time labels without touching hover or confirmation state.
  // nowMs 刷新相对时间标签且不触碰悬停或确认状态。
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  React.useEffect(() => {
    const intervalId = window.setInterval(() => setNowMs(Date.now()), 60_000);
    return () => window.clearInterval(intervalId);
  }, []);
  React.useEffect(() => {
    setOpenProjectIds(initialExpandedProjectIdSet(props.projects, props.expandedProjectIds));
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
    setOpenProjectIds((current) => {
      const next = new Set(current);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      props.onExpandedProjectIdsChange(expandedProjectIdsInProjectOrder(props.projects, next));
      return next;
    });
  }, [props]);
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
        hasSelectedConversation={props.selectedConversationId ? projectConversationIds.has(props.selectedConversationId) : false}
        isOpen={openProjectIds.has(project.id)}
        isConversationListExpanded={expandedConversationProjectIds.has(project.id)}
        isPinnedConversation={(conversationId) => props.pinnedConversationIds.has(conversationId)}
        key={project.id}
        locale={locale}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={props.onCreateConversation}
        onProjectMenuAction={props.onProjectMenuAction}
        renderConversationRow={renderConversationRow}
        onSelectConversation={props.onSelectConversation}
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
      isSelected={props.selectedConversationId === conversation.id}
      key={conversation.id}
      locale={locale}
      nowMs={nowMs}
      onAction={props.onConversationMenuAction}
      onSelectConversation={props.onSelectConversation}
      project={conversation.projectId ? projectsById.get(conversation.projectId) : undefined}
    />
  );

  return (
    <Sidebar className={cn(activeProjectDragId && "project-drag-active")} collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <div aria-current="page" className="vcn-sidebar-static-item">
              <LayoutDashboard size={16} />
              <span className="vcn-sidebar-label strong">{t("sidebar.console")}</span>
            </div>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        {pinnedConversations.length > 0 || pinnedProjects.length > 0 ? (
          <SidebarGroup className="project-section pinned-project-section">
            {renderProjectGroupLabel(t("sidebar.pinned"), <Pin size={15} />)}
            <SidebarMenu className="pinned-tree-list">
              {pinnedConversations.map((conversation) => renderConversationRow(conversation, true))}
              {pinnedProjects.length > 0 ? renderSortableProjectSection(pinnedProjects) : null}
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
              {renderSortableProjectSection(regularProjects)}
              {props.projects.length === 0 ? (
                <SidebarMenuItem>
                  <SidebarMenuButton className="muted" tooltip={t("sidebar.noProjects")}>
                    <Folder size={16} />
                    <span className="vcn-sidebar-label">
                      <strong>{t("sidebar.noProjects")}</strong>
                      <small>{t("sidebar.waitingWorkspace")}</small>
                    </span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ) : null}
            </SidebarMenu>
          </SidebarGroup>
      </SidebarContent>
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
