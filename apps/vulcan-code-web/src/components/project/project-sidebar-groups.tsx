import React from "react";
import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  pointerWithin,
  useDroppable,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { Boxes, ChevronRight, Ellipsis, FolderPlus, GripVertical, Pencil, Plus, Ungroup } from "lucide-react";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { Input } from "@/ui-kit/primitives/input";
import { Spinner } from "@/ui-kit/primitives/spinner";
import { toast } from "@/ui-kit/primitives/toast";
import { SidebarMenu } from "@/ui-kit/shell/sidebar";
import { formatMessage, useI18n } from "@/i18n";
import type { ProjectGroupSummary, ProjectSummary } from "@/types";
import {
  buildProjectTreeLayout,
  containerOf,
  layoutOf,
  moveGroup,
  moveProject,
  UNGROUPED_CONTAINER,
  type ProjectGroupLayout,
} from "./project-group-layout";
import { useOptimisticOrder, useVerticalSortableItem } from "./project-sidebar-sortable";

// ProjectGroupAction names one project-group change the workbench persists through the service.
// ProjectGroupAction 命名工作台通过服务持久化的一次项目分组变更。
export type ProjectGroupAction =
  | { kind: "create"; name: string; projectIds: string[] }
  | { kind: "rename"; groupId: string; name: string }
  | { kind: "dissolve"; groupId: string }
  | { kind: "layout"; layout: ProjectGroupLayout };

// ProjectRowPlacement lets a project row offer "move to group"; pinned rows have none.
// ProjectRowPlacement 让项目行提供“移动到分组”；置顶行没有该项。
export interface ProjectRowPlacement {
  // Every group the project can move to, in display order.
  // 项目可移入的全部分组，按显示顺序排列。
  groups: ReadonlyArray<{ id: string; name: string }>;
  // Group currently holding the project; null when it is ungrouped.
  // 当前包含该项目的分组；未分组时为 null。
  currentGroupId: string | null;
  // Moves the project into a group, or out of every group with null.
  // 将项目移入某分组；传 null 时移出所有分组。
  onMove: (groupId: string | null) => void;
  // Opens the new-group dialog with this project as its first member.
  // 以此项目为首个成员打开新建分组对话框。
  onMoveToNewGroup: () => void;
}

// GROUP_ID_PREFIX marks sortable ids that belong to groups rather than projects.
// GROUP_ID_PREFIX 标记属于分组而非项目的可排序标识。
const GROUP_ID_PREFIX = "project-group:";

// UNGROUPED_DROP_ID is the drop target of the ungrouped bucket.
// UNGROUPED_DROP_ID 是未分组区的拖放目标。
const UNGROUPED_DROP_ID = "project-group-bucket:ungrouped";

// PINNED_SECTION_ID keys the collapsible pinned-projects heading.
// PINNED_SECTION_ID 是可折叠置顶项目标题的键。
const PINNED_SECTION_ID = "pinned";

// COLLAPSED_GROUPS_STORAGE_KEY keeps which group headings this viewer collapsed; it is a per-browser convenience.
// COLLAPSED_GROUPS_STORAGE_KEY 保存此查看者折叠了哪些分组标题；它只是按浏览器保存的便利设置。
const COLLAPSED_GROUPS_STORAGE_KEY = "vulcan.sidebar.collapsedProjectGroups";

// PENDING_LAYOUT_TIMEOUT_MS bounds how long a dropped layout shows before the authoritative one returns.
// PENDING_LAYOUT_TIMEOUT_MS 限定拖放后的布局在权威布局返回前最多显示多久。
const PENDING_LAYOUT_TIMEOUT_MS = 5000;

// readCollapsedGroups restores collapsed group ids; storage may be unavailable, which means nothing is collapsed.
// readCollapsedGroups 恢复已折叠分组标识；存储可能不可用，此时视为没有折叠。
function readCollapsedGroups(): Set<string> {
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(COLLAPSED_GROUPS_STORAGE_KEY) ?? "[]");
    return new Set(Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === "string") : []);
  } catch {
    return new Set();
  }
}

// writeCollapsedGroups saves collapsed group ids when storage is available.
// writeCollapsedGroups 在存储可用时保存已折叠分组标识。
function writeCollapsedGroups(ids: Set<string>): void {
  try {
    window.localStorage.setItem(COLLAPSED_GROUPS_STORAGE_KEY, JSON.stringify([...ids]));
  } catch {
    // The collapsed state is a convenience; losing it only reopens the groups.
    // 折叠状态只是便利设置；丢失时仅会重新展开分组。
  }
}

// usePendingLayout shows a dropped layout at once while it is saved, and clears it when the authoritative groups
// or projects change or after PENDING_LAYOUT_TIMEOUT_MS, so a rejected layout cannot linger.
// usePendingLayout 在保存期间立即显示拖放后的布局；权威分组或项目变化、或超过 PENDING_LAYOUT_TIMEOUT_MS 后清除，
// 使被拒绝的布局不会滞留。
function usePendingLayout(signature: string): [ProjectGroupLayout | null, (layout: ProjectGroupLayout | null) => void] {
  const [pending, setPending] = React.useState<ProjectGroupLayout | null>(null);
  const signatureRef = React.useRef(signature);
  React.useEffect(() => {
    if (signatureRef.current !== signature) {
      signatureRef.current = signature;
      setPending(null);
    }
  }, [signature]);
  React.useEffect(() => {
    if (!pending) return;
    const timer = window.setTimeout(() => setPending(null), PENDING_LAYOUT_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [pending]);
  return [pending, setPending];
}

// ProjectTreeSection renders the code flow's projects: pinned projects, user groups and ungrouped projects. Groups
// collapse, reorder by dragging their grip, and accept projects dragged into them.
// ProjectTreeSection 渲染代码流的项目：置顶项目、用户分组与未分组项目。分组可折叠，拖动手柄可排序，并接受拖入的项目。
export function ProjectTreeSection(props: {
  // Visible projects in service order, pinned first.
  // 按服务顺序排列的可见项目，置顶在前。
  projects: readonly ProjectSummary[];
  // Stored groups projected onto the visible projects.
  // 投影到可见项目上的已保存分组。
  projectGroups: readonly ProjectGroupSummary[];
  // Lower-cased filter is active; sorting is disabled and every group opens.
  // 小写筛选生效；排序被禁用且所有分组展开。
  filtering: boolean;
  // Whether one project matches the active filter.
  // 某项目是否匹配当前筛选。
  isProjectVisible: (project: ProjectSummary) => boolean;
  // Number of listed sessions under one project.
  // 某项目下列出的会话数量。
  sessionCount: (projectId: string) => number;
  // Renders one project row; placement is null for pinned rows.
  // 渲染一个项目行；置顶行的 placement 为 null。
  renderProjectRow: (project: ProjectSummary, placement: ProjectRowPlacement | null) => React.ReactNode;
  // Opens the add-project flow.
  // 打开添加项目流程。
  onCreateProject: () => void;
  // Persists the pinned section order.
  // 持久化置顶分区顺序。
  onReorderPinned: (projectIds: string[]) => void;
  // Persists one group change; rejects with the service error.
  // 持久化一次分组变更；失败时以服务错误拒绝。
  onGroupAction: (action: ProjectGroupAction) => Promise<void>;
  // Reports whether a drag is active so hover-only UI can pause.
  // 报告拖动是否进行中，以便暂停仅悬停显示的界面。
  onSortingChange: (sorting: boolean) => void;
  // Rendered after the rows when the filter hides everything.
  // 当筛选隐藏全部内容时渲染在行之后。
  emptyNotice: React.ReactNode;
}) {
  const { t } = useI18n();
  const [pinnedOrdered, showPinnedOrder] = useOptimisticOrder(props.projects.filter((project) => project.pinned));
  const signature = JSON.stringify([props.projects.map((project) => [project.id, project.pinned]), props.projectGroups]);
  const [pendingLayout, setPendingLayout] = usePendingLayout(signature);
  const tree = React.useMemo(
    () => buildProjectTreeLayout(props.projects, props.projectGroups, pendingLayout),
    [pendingLayout, props.projectGroups, props.projects],
  );
  const layout = React.useMemo(() => layoutOf(tree), [tree]);
  const [collapsed, setCollapsed] = React.useState(readCollapsedGroups);
  const [nameDialog, setNameDialog] = React.useState<GroupNameDialogTarget | null>(null);
  const hasGroups = tree.groups.length > 0;
  const pinnedIds = React.useMemo(() => new Set(pinnedOrdered.map((project) => project.id)), [pinnedOrdered]);
  const regularIds = React.useMemo(
    () => new Set([...tree.groups.flatMap((entry) => entry.projects), ...tree.ungrouped].map((project) => project.id)),
    [tree],
  );
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const toggleCollapsed = (id: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id); else next.add(id);
      writeCollapsedGroups(next);
      return next;
    });
  };

  // saveLayout shows a new layout at once and persists it; a rejection restores the authoritative layout.
  // saveLayout 立即显示新布局并持久化；被拒绝时恢复权威布局。
  const saveLayout = (next: ProjectGroupLayout | null) => {
    if (!next) return;
    setPendingLayout(next);
    props.onGroupAction({ kind: "layout", layout: next }).catch((error: unknown) => {
      setPendingLayout(null);
      toast.error(error instanceof Error ? error.message : String(error));
    });
  };

  // runGroupAction persists a menu action and reports a failure without a dialog to hold it.
  // runGroupAction 持久化菜单动作，并在没有对话框承载时报告失败。
  const runGroupAction = (action: ProjectGroupAction) => {
    props.onGroupAction(action).catch((error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    });
  };

  // collisionDetection keeps each drag inside its own kind: groups reorder among groups, pinned projects among pinned
  // projects, and other projects land on the row nearest the pointer within the group under it, or on the group
  // itself when that group shows no rows.
  // collisionDetection 让每次拖动限定在同类目标内：分组只在分组间排序，置顶项目只在置顶项目间排序；其他项目落在指针所在
  // 分组内最近的行上，若该分组没有显示行则落在分组本身。
  const collisionDetection = React.useCallback<CollisionDetection>((args) => {
    const activeId = String(args.active.id);
    const within = (predicate: (id: string) => boolean) => (
      args.droppableContainers.filter((container) => predicate(String(container.id)))
    );
    if (activeId.startsWith(GROUP_ID_PREFIX)) {
      return closestCenter({ ...args, droppableContainers: within((id) => id.startsWith(GROUP_ID_PREFIX)) });
    }
    if (pinnedIds.has(activeId)) {
      return closestCenter({ ...args, droppableContainers: within((id) => pinnedIds.has(id)) });
    }
    const containerHits = pointerWithin({
      ...args,
      droppableContainers: within((id) => id.startsWith(GROUP_ID_PREFIX) || id === UNGROUPED_DROP_ID),
    });
    const hit = containerHits[0];
    if (hit) {
      const hitId = String(hit.id);
      const members = hitId === UNGROUPED_DROP_ID
        ? layout.ungroupedProjectIds
        : layout.groups.find((entry) => GROUP_ID_PREFIX + entry.groupId === hitId)?.projectIds ?? [];
      const shown = new Set(members.filter((id) => id === activeId || !collapsed.has(groupKeyOf(hitId))));
      const rows = closestCenter({ ...args, droppableContainers: within((id) => shown.has(id)) });
      const nearest = rows[0];
      if (!nearest || (nearest.id === activeId && shown.size === 1)) return [hit];
      // A project from elsewhere dropped below the middle of a group's last row goes to the end of that group.
      // 来自其他位置的项目拖到分组最后一行中线以下时，放到该分组末尾。
      const lastId = members.filter((id) => shown.has(id)).at(-1);
      const lastRect = lastId ? args.droppableRects.get(lastId) : undefined;
      const pointer = args.pointerCoordinates;
      if (lastId === String(nearest.id) && !members.includes(activeId) && lastRect && pointer
        && pointer.y > lastRect.top + lastRect.height / 2) {
        return [hit];
      }
      return rows;
    }
    return closestCenter({ ...args, droppableContainers: within((id) => regularIds.has(id)) });
  }, [collapsed, layout, pinnedIds, regularIds]);

  const handleDragEnd = (event: DragEndEvent) => {
    props.onSortingChange(false);
    if (!event.over || props.filtering) return;
    const activeId = String(event.active.id);
    const overId = String(event.over.id);
    if (activeId === overId) return;
    if (activeId.startsWith(GROUP_ID_PREFIX)) {
      if (!overId.startsWith(GROUP_ID_PREFIX)) return;
      saveLayout(moveGroup(layout, activeId.slice(GROUP_ID_PREFIX.length), overId.slice(GROUP_ID_PREFIX.length)));
      return;
    }
    if (pinnedIds.has(activeId)) {
      const ids = pinnedOrdered.map((project) => project.id);
      const from = ids.indexOf(activeId);
      const to = ids.indexOf(overId);
      if (from < 0 || to < 0) return;
      const next = arrayMove(ids, from, to);
      showPinnedOrder(next);
      props.onReorderPinned(next);
      return;
    }
    if (overId.startsWith(GROUP_ID_PREFIX)) {
      saveLayout(moveProject(layout, activeId, { container: overId.slice(GROUP_ID_PREFIX.length) }));
    } else if (overId === UNGROUPED_DROP_ID) {
      saveLayout(moveProject(layout, activeId, { container: UNGROUPED_CONTAINER }));
    } else {
      const container = containerOf(layout, overId);
      if (container) saveLayout(moveProject(layout, activeId, { container, beforeProjectId: overId }));
    }
  };

  // placementOf builds the move-to-group entries of one unpinned project row.
  // placementOf 为一个未置顶项目行构建“移动到分组”入口。
  const placementOf = (project: ProjectSummary): ProjectRowPlacement => {
    const current = containerOf(layout, project.id);
    return {
      groups: tree.groups.map((entry) => ({ id: entry.group.id, name: entry.group.name })),
      currentGroupId: current === UNGROUPED_CONTAINER ? null : current,
      onMove: (groupId) => saveLayout(moveProject(layout, project.id, { container: groupId ?? UNGROUPED_CONTAINER })),
      onMoveToNewGroup: () => setNameDialog({ mode: "create", projectIds: [project.id] }),
    };
  };

  const visiblePinned = pinnedOrdered.filter(props.isProjectVisible);
  const visibleUngrouped = tree.ungrouped.filter(props.isProjectVisible);
  const pinnedRows = (
    <SortableContext disabled={props.filtering} items={visiblePinned.map((project) => project.id)} strategy={verticalListSortingStrategy}>
      {visiblePinned.map((project) => props.renderProjectRow(project, null))}
    </SortableContext>
  );
  const ungroupedRows = (
    <SortableContext disabled={props.filtering} items={visibleUngrouped.map((project) => project.id)} strategy={verticalListSortingStrategy}>
      {visibleUngrouped.map((project) => props.renderProjectRow(project, placementOf(project)))}
    </SortableContext>
  );
  const shownGroups = props.filtering
    ? tree.groups.filter((entry) => entry.projects.some(props.isProjectVisible))
    : tree.groups;
  const nothingShown = visiblePinned.length === 0 && visibleUngrouped.length === 0 && shownGroups.length === 0;

  return (
    <section>
      <div className="vk-tree-head">
        <span className="vk-label"><Boxes aria-hidden="true" />{t("sidebar.projects")}</span>
        <IconButton aria-label={t("sidebar.newGroup")} onClick={() => setNameDialog({ mode: "create", projectIds: [] })} title={t("sidebar.newGroup")}>
          <FolderPlus aria-hidden="true" />
        </IconButton>
        <IconButton aria-label={t("sidebar.addProject")} onClick={props.onCreateProject} title={t("sidebar.addProject")}>
          <Plus aria-hidden="true" />
        </IconButton>
      </div>
      <SidebarMenu>
        <DndContext
          collisionDetection={collisionDetection}
          onDragCancel={() => props.onSortingChange(false)}
          onDragEnd={handleDragEnd}
          onDragStart={() => props.onSortingChange(true)}
          sensors={sensors}
        >
          {hasGroups && visiblePinned.length > 0 ? (
            <ProjectGroupFrame
              count={visiblePinned.length}
              id={PINNED_SECTION_ID}
              label={t("sidebar.group.pinned")}
              open={props.filtering || !collapsed.has(PINNED_SECTION_ID)}
              onToggle={() => toggleCollapsed(PINNED_SECTION_ID)}
              sessions={visiblePinned.reduce((sum, project) => sum + props.sessionCount(project.id), 0)}
            >
              {pinnedRows}
            </ProjectGroupFrame>
          ) : pinnedRows}
          {hasGroups ? (
            <SortableContext disabled={props.filtering} items={shownGroups.map((entry) => GROUP_ID_PREFIX + entry.group.id)} strategy={verticalListSortingStrategy}>
              {shownGroups.map((entry) => {
                const visible = entry.projects.filter(props.isProjectVisible);
                return (
                  <SortableProjectGroup
                    count={entry.projects.length}
                    disabled={props.filtering}
                    group={entry.group}
                    key={entry.group.id}
                    onDissolve={() => runGroupAction({ kind: "dissolve", groupId: entry.group.id })}
                    onRename={() => setNameDialog({ mode: "rename", group: entry.group })}
                    onToggle={() => toggleCollapsed(entry.group.id)}
                    open={props.filtering || !collapsed.has(entry.group.id)}
                    sessions={entry.projects.reduce((sum, project) => sum + props.sessionCount(project.id), 0)}
                  >
                    <SortableContext disabled={props.filtering} items={visible.map((project) => project.id)} strategy={verticalListSortingStrategy}>
                      {visible.map((project) => props.renderProjectRow(project, placementOf(project)))}
                    </SortableContext>
                  </SortableProjectGroup>
                );
              })}
            </SortableContext>
          ) : null}
          {hasGroups && (visibleUngrouped.length > 0 || !props.filtering) ? (
            <UngroupedBucket
              count={tree.ungrouped.length}
              label={t("sidebar.group.ungrouped")}
              open={props.filtering || !collapsed.has(UNGROUPED_CONTAINER)}
              onToggle={() => toggleCollapsed(UNGROUPED_CONTAINER)}
              sessions={tree.ungrouped.reduce((sum, project) => sum + props.sessionCount(project.id), 0)}
            >
              {ungroupedRows}
            </UngroupedBucket>
          ) : hasGroups ? null : ungroupedRows}
        </DndContext>
        {props.filtering && nothingShown ? props.emptyNotice : null}
      </SidebarMenu>
      <ProjectGroupNameDialog
        target={nameDialog}
        onCancel={() => setNameDialog(null)}
        onConfirm={async (target, name) => {
          await props.onGroupAction(target.mode === "create"
            ? { kind: "create", name, projectIds: target.projectIds }
            : { kind: "rename", groupId: target.group.id, name });
          setNameDialog(null);
        }}
      />
    </section>
  );
}

// groupKeyOf maps a group or bucket drop id back to its collapsed-state key.
// groupKeyOf 将分组或未分组区的拖放标识映射回其折叠状态键。
function groupKeyOf(dropId: string): string {
  return dropId === UNGROUPED_DROP_ID ? UNGROUPED_CONTAINER : dropId.slice(GROUP_ID_PREFIX.length);
}

// ProjectGroupFrameProps describes one collapsible group heading and its body.
// ProjectGroupFrameProps 描述一个可折叠分组标题及其内容。
interface ProjectGroupFrameProps {
  // Collapsed-state key and DOM marker of the group.
  // 分组的折叠状态键与 DOM 标记。
  id: string;
  // Group label shown on the heading.
  // 标题上显示的分组名称。
  label: string;
  // Number of projects in the group.
  // 分组中的项目数量。
  count: number;
  // Number of listed sessions across the group's projects.
  // 分组内各项目列出的会话总数。
  sessions: number;
  // Whether the body is shown.
  // 是否显示内容。
  open: boolean;
  // Toggles the body.
  // 切换内容显示。
  onToggle: () => void;
  // Trailing heading actions.
  // 标题尾部操作。
  actions?: React.ReactNode;
  // Rows shown when open.
  // 展开时显示的行。
  children: React.ReactNode;
}

// ProjectGroupFrame renders a group heading: a caret, the name, the project count and its actions, with the rows
// below while open. An open group without rows says where projects come from.
// ProjectGroupFrame 渲染分组标题：展开箭头、名称、项目数量与操作，展开时其下显示各行。没有行的展开分组会说明项目从何而来。
const ProjectGroupFrame = React.forwardRef<HTMLLIElement, ProjectGroupFrameProps & {
  // Sortable transform applied to the whole group while it is dragged.
  // 拖动分组时应用于整个分组的排序变换。
  style?: React.CSSProperties;
  // Marks the group being dragged.
  // 标记正在拖动的分组。
  dragging?: boolean;
  // Marks a group a project is being dragged over.
  // 标记有项目正被拖到其上的分组。
  dropTarget?: boolean;
}>(function ProjectGroupFrame(props, ref) {
  const { t } = useI18n();
  const summary = formatMessage(t("sidebar.group.summary"), { projects: props.count, sessions: props.sessions });
  return (
    <li
      className="vk-tree-group"
      data-dragging={props.dragging ? "" : undefined}
      data-drop={props.dropTarget ? "" : undefined}
      data-group={props.id}
      ref={ref}
      style={props.style}
    >
      <div className="vk-tree-item" data-kind="group">
        <button aria-expanded={props.open} className="vk-group-row" onClick={props.onToggle} title={summary} type="button">
          <ChevronRight aria-hidden="true" className="vk-tree-caret" />
          <span className="vk-label">{props.label}</span>
          <small className="vk-num">{formatMessage(t("sidebar.group.projectCount"), { count: props.count })}</small>
        </button>
        {props.actions ? <span className="vk-tree-actions">{props.actions}</span> : null}
      </div>
      {props.open ? (
        <ul className="vk-group-body">
          {props.count === 0 ? <li className="vk-tree-blank">{t("sidebar.group.dropHere")}</li> : props.children}
        </ul>
      ) : null}
    </li>
  );
});

// SortableProjectGroup is a user group: it reorders by its grip, accepts dropped projects, and has a menu.
// SortableProjectGroup 是用户分组：可通过手柄排序、接受拖入的项目，并带有菜单。
function SortableProjectGroup(props: Omit<ProjectGroupFrameProps, "id" | "label" | "actions"> & {
  // Group rendered by this heading.
  // 此标题渲染的分组。
  group: ProjectGroupSummary;
  // Disables sorting while the list is filtered.
  // 列表被筛选时禁用排序。
  disabled: boolean;
  // Opens the rename dialog.
  // 打开重命名对话框。
  onRename: () => void;
  // Dissolves the group; its projects become ungrouped.
  // 解散分组；其项目变为未分组。
  onDissolve: () => void;
}) {
  const { t } = useI18n();
  const sortable = useVerticalSortableItem(GROUP_ID_PREFIX + props.group.id, props.disabled);
  const draggingProject = sortable.active !== null && !String(sortable.active.id).startsWith(GROUP_ID_PREFIX);
  const reorderLabel = formatMessage(t("sidebar.group.reorder"), { name: props.group.name });
  return (
    <ProjectGroupFrame
      {...props}
      actions={(
        <>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <IconButton aria-label={t("sidebar.group.menu")} size="xs" title={t("sidebar.group.menu")}>
                <Ellipsis aria-hidden="true" />
              </IconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={props.onRename}>
                <Pencil aria-hidden="true" />
                <span>{t("sidebar.group.rename")}</span>
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={props.onDissolve}>
                <Ungroup aria-hidden="true" />
                <span>{t("sidebar.group.dissolve")}</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <button
            ref={sortable.setActivatorNodeRef}
            {...sortable.attributes}
            {...sortable.listeners}
            aria-label={reorderLabel}
            className="vk-icon-btn vk-tree-grip"
            data-size="sm"
            title={reorderLabel}
            type="button"
          >
            <GripVertical aria-hidden="true" />
          </button>
        </>
      )}
      dragging={sortable.isDragging}
      dropTarget={draggingProject && sortable.isOver}
      id={props.group.id}
      label={props.group.name}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
    />
  );
}

// UngroupedBucket is the pseudo-group of projects outside every group; projects dropped on it leave their group.
// UngroupedBucket 是不属于任何分组的项目所在的伪分组；拖到其上的项目会离开原分组。
function UngroupedBucket(props: Omit<ProjectGroupFrameProps, "id">) {
  const droppable = useDroppable({ id: UNGROUPED_DROP_ID });
  const draggingProject = droppable.active !== null && !String(droppable.active.id).startsWith(GROUP_ID_PREFIX);
  return (
    <ProjectGroupFrame
      {...props}
      dropTarget={draggingProject && droppable.isOver}
      id={UNGROUPED_CONTAINER}
      ref={droppable.setNodeRef}
    />
  );
}

// GroupNameDialogTarget is the pending create or rename request of the group name dialog.
// GroupNameDialogTarget 是分组名称对话框待处理的新建或重命名请求。
type GroupNameDialogTarget =
  | { mode: "create"; projectIds: string[] }
  | { mode: "rename"; group: ProjectGroupSummary };

// ProjectGroupNameDialog asks for a group name, keeping the draft and showing the service error when saving fails.
// ProjectGroupNameDialog 询问分组名称；保存失败时保留草稿并显示服务错误。
function ProjectGroupNameDialog(props: {
  // Pending request, or null when closed.
  // 待处理请求；关闭时为 null。
  target: GroupNameDialogTarget | null;
  // Closes the dialog without saving.
  // 不保存并关闭对话框。
  onCancel: () => void;
  // Saves the entered name; rejects with the service error.
  // 保存输入的名称；失败时以服务错误拒绝。
  onConfirm: (target: GroupNameDialogTarget, name: string) => Promise<void>;
}) {
  const { t } = useI18n();
  const [value, setValue] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    setValue(props.target?.mode === "rename" ? props.target.group.name : "");
    setError(null);
  }, [props.target]);
  const creating = props.target?.mode === "create";
  const title = t(creating ? "sidebar.group.createTitle" : "sidebar.group.renameTitle");
  const submit = async () => {
    if (!props.target || busy || !value.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await props.onConfirm(props.target, value);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={Boolean(props.target)} onOpenChange={(open) => !open && !busy && props.onCancel()}>
      <DialogContent size="sm" showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{t(creating ? "sidebar.group.createDescription" : "sidebar.group.renameDescription")}</DialogDescription>
        </DialogHeader>
        <form
          className="dialog-form"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <Input
            aria-label={title}
            autoFocus
            disabled={busy}
            onChange={(event) => setValue(event.target.value)}
            placeholder={t("sidebar.group.namePlaceholder")}
            value={value}
          />
          {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
          <DialogFooter>
            <Button disabled={busy} onClick={props.onCancel} type="button" variant="ghost">{t("conversation.action.cancel")}</Button>
            <Button disabled={busy || !value.trim()} type="submit">{busy ? <Spinner /> : null}{t("projectDialog.confirm")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
