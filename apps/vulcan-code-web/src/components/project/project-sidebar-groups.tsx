import React from "react";
import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  pointerWithin,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
  type KeyboardCoordinateGetter,
  type UniqueIdentifier,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { ChevronRight, Ellipsis, FolderPlus, FolderTree, GripVertical, Pencil, Pin, PinOff, Plus, Trash2 } from "lucide-react";
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
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { Input } from "@/ui-kit/primitives/input";
import { Spinner } from "@/ui-kit/primitives/spinner";
import { toast } from "@/ui-kit/primitives/toast";
import { SidebarMenu } from "@/ui-kit/shell/sidebar";
import { formatMessage, useI18n } from "@/i18n";
import type { ConversationSummary, ProjectSummary, ProjectTreePin, ProjectTreeSummary } from "@/types";
import {
  buildProjectTreeLayout,
  containerOf,
  DEFAULT_GROUP_ID,
  layoutOf,
  moveGroup,
  moveProject,
  type ProjectGroupLayout,
  type TreeGroup,
} from "./project-group-layout";
import { useOptimisticOrder, useSortableRowBinding, useVerticalSortableItem, type SortableRowBinding } from "./project-sidebar-sortable";

// ProjectGroupAction names one project-tree change the workbench persists through the service.
// ProjectGroupAction 命名工作台通过服务持久化的一次项目树变更。
export type ProjectGroupAction =
  | { kind: "create"; name: string; projectIds: string[]; afterGroupId?: string }
  | { kind: "rename"; groupId: string; name: string }
  | { kind: "delete"; groupId: string }
  | { kind: "layout"; layout: ProjectGroupLayout }
  | { kind: "pin"; item: ProjectTreePin; pinned: boolean }
  | { kind: "pins-order"; pins: ProjectTreePin[] };

// ProjectRowPlacement lets a project row offer "move to group"; pinned rows have none.
// ProjectRowPlacement 让项目行提供“移动到分组”；置顶行没有该项。
export interface ProjectRowPlacement {
  // Every group the project can move to, in display order, the default group included.
  // 项目可移入的全部分组（含默认分组），按显示顺序排列。
  groups: ReadonlyArray<{ id: string; name: string }>;
  // Group currently holding the project.
  // 当前包含该项目的分组。
  currentGroupId: string;
  // Moves the project into a group.
  // 将项目移入某分组。
  onMove: (groupId: string) => void;
  // Opens the new-group dialog with this project as its first member.
  // 以此项目为首个成员打开新建分组对话框。
  onMoveToNewGroup: () => void;
}

// GROUP_ID_PREFIX marks sortable ids of unpinned groups.
// GROUP_ID_PREFIX 标记未置顶分组的可排序标识。
const GROUP_ID_PREFIX = "project-group:";

// PIN_ID_PREFIX marks sortable ids of pinned items; a pinned group is PIN_ID_PREFIX + "group:" + its id.
// PIN_ID_PREFIX 标记置顶条目的可排序标识；置顶分组为 PIN_ID_PREFIX + "group:" + 其标识。
const PIN_ID_PREFIX = "pin:";

// PIN_GROUP_PREFIX marks sortable ids of pinned groups, which also accept dropped projects.
// PIN_GROUP_PREFIX 标记置顶分组的可排序标识；置顶分组同样接受拖入的项目。
const PIN_GROUP_PREFIX = `${PIN_ID_PREFIX}group:`;

// COLLAPSED_GROUPS_STORAGE_KEY keeps which group headings this viewer collapsed; it is a per-browser convenience.
// COLLAPSED_GROUPS_STORAGE_KEY 保存此查看者折叠了哪些分组标题；它只是按浏览器保存的便利设置。
const COLLAPSED_GROUPS_STORAGE_KEY = "vulcan.sidebar.collapsedProjectGroups";

// PENDING_LAYOUT_TIMEOUT_MS bounds how long a dropped layout shows before the authoritative one returns.
// PENDING_LAYOUT_TIMEOUT_MS 限定拖放后的布局在权威布局返回前最多显示多久。
const PENDING_LAYOUT_TIMEOUT_MS = 5000;

// treeKeyboardCoordinates steps a keyboard drag of a group or pinned item only across items of the same kind. The
// sortable default steps to the nearest droppable of any kind, so a group would land on its own first project row
// and never move.
// treeKeyboardCoordinates 让分组或置顶条目的键盘拖动只在同类条目之间移动。可排序默认实现会移到任意类型中最近的可放置
// 目标，分组因此会落到自己的第一个项目行上而无法移动。
const treeKeyboardCoordinates: KeyboardCoordinateGetter = (event, args) => {
  const activeId = String(args.context.active?.id ?? "");
  const prefix = [PIN_ID_PREFIX, GROUP_ID_PREFIX].find((candidate) => activeId.startsWith(candidate));
  if (!prefix) return sortableKeyboardCoordinates(event, args);
  const all = args.context.droppableContainers;
  const scoped = Object.assign(Object.create(all) as typeof all, {
    get: (id: UniqueIdentifier) => all.get(id),
    getEnabled: () => all.getEnabled().filter((entry) => String(entry.id).startsWith(prefix)),
  });
  return sortableKeyboardCoordinates(event, { ...args, context: { ...args.context, droppableContainers: scoped } });
};

// pinKey is the sortable id of one pinned item.
// pinKey 是一个置顶条目的可排序标识。
function pinKey(pin: ProjectTreePin): string {
  return `${PIN_ID_PREFIX}${pin.kind}:${pin.id}`;
}

// groupOfDropId maps an unpinned or pinned group drop id to its group id; null for other ids.
// groupOfDropId 将未置顶或置顶分组的拖放标识映射为分组标识；其他标识返回 null。
function groupOfDropId(dropId: string): string | null {
  if (dropId.startsWith(GROUP_ID_PREFIX)) return dropId.slice(GROUP_ID_PREFIX.length);
  if (dropId.startsWith(PIN_GROUP_PREFIX)) return dropId.slice(PIN_GROUP_PREFIX.length);
  return null;
}

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

// usePendingLayout shows a dropped layout at once while it is saved, and clears it when the authoritative tree
// or projects change or after PENDING_LAYOUT_TIMEOUT_MS, so a rejected layout cannot linger.
// usePendingLayout 在保存期间立即显示拖放后的布局；权威项目树或项目变化、或超过 PENDING_LAYOUT_TIMEOUT_MS 后清除，
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

// PinnedEntry is one shown item of the pinned section with what it renders.
// PinnedEntry 是置顶分区中一个已显示的条目及其渲染内容。
type PinnedEntry =
  | { id: string; pin: ProjectTreePin; kind: "session"; conversation: ConversationSummary }
  | { id: string; pin: ProjectTreePin; kind: "project"; project: ProjectSummary }
  | { id: string; pin: ProjectTreePin; kind: "group"; group: TreeGroup };

// ProjectTreeSection renders the code flow's tree: the pinned section of sessions, projects and groups, then every
// unpinned group, the default "Projects" group included. Groups collapse, reorder by their grip, accept dropped
// projects, and add projects or groups from their [+] menu; the pinned section has its own order.
// ProjectTreeSection 渲染代码流的项目树：由会话、项目与分组组成的置顶分区，以及所有未置顶分组（含默认“项目”分组）。
// 分组可折叠、可通过手柄排序、接受拖入的项目，并可从 [+] 菜单新建项目或分组；置顶分区有独立顺序。
export function ProjectTreeSection(props: {
  // Visible projects in service order, pinned first.
  // 按服务顺序排列的可见项目，置顶在前。
  projects: readonly ProjectSummary[];
  // Listed conversations, the source of pinned sessions.
  // 列出的会话，置顶会话由此而来。
  conversations: readonly ConversationSummary[];
  // Stored tree projected onto the visible projects and sessions.
  // 投影到可见项目与会话上的已保存项目树。
  projectTree: ProjectTreeSummary;
  // Lower-cased filter is active; sorting is disabled and every group opens.
  // 小写筛选生效；排序被禁用且所有分组展开。
  filtering: boolean;
  // Whether one project matches the active filter.
  // 某项目是否匹配当前筛选。
  isProjectVisible: (project: ProjectSummary) => boolean;
  // Whether one session matches the active filter.
  // 某会话是否匹配当前筛选。
  isConversationVisible: (conversation: ConversationSummary) => boolean;
  // Number of listed sessions under one project.
  // 某项目下列出的会话数量。
  sessionCount: (projectId: string) => number;
  // Renders one project row; placement is null for pinned rows, which sort under sortableId.
  // 渲染一个项目行；置顶行的 placement 为 null，并以 sortableId 排序。
  renderProjectRow: (project: ProjectSummary, placement: ProjectRowPlacement | null, sortableId?: string) => React.ReactNode;
  // Renders one pinned session row bound to the pinned order.
  // 渲染一个绑定置顶顺序的置顶会话行。
  renderPinnedConversation: (conversation: ConversationSummary, sortable: SortableRowBinding) => React.ReactNode;
  // Opens the add-project flow targeting one group.
  // 打开以某分组为目标的添加项目流程。
  onCreateProject: (groupId: string) => void;
  // Persists one tree change; rejects with the service error.
  // 持久化一次项目树变更；失败时以服务错误拒绝。
  onGroupAction: (action: ProjectGroupAction) => Promise<void>;
  // Reports whether a drag is active so hover-only UI can pause.
  // 报告拖动是否进行中，以便暂停仅悬停显示的界面。
  onSortingChange: (sorting: boolean) => void;
  // Rendered after the rows when the filter hides everything.
  // 当筛选隐藏全部内容时渲染在行之后。
  emptyNotice: React.ReactNode;
}) {
  const { t } = useI18n();
  const signature = JSON.stringify([props.projects.map((project) => [project.id, project.pinned]), props.projectTree.groups, props.projectTree.defaultGroupIndex]);
  const [pendingLayout, setPendingLayout] = usePendingLayout(signature);
  const tree = React.useMemo(
    () => buildProjectTreeLayout(props.projects, props.projectTree, pendingLayout),
    [pendingLayout, props.projectTree, props.projects],
  );
  const layout = React.useMemo(() => layoutOf(tree), [tree]);
  const [collapsed, setCollapsed] = React.useState(readCollapsedGroups);
  const [nameDialog, setNameDialog] = React.useState<GroupNameDialogTarget | null>(null);
  const defaultLabel = t("sidebar.projects");
  const groupName = (entry: TreeGroup) => entry.group?.name ?? defaultLabel;
  const pinnedGroupIds = React.useMemo(
    () => new Set(props.projectTree.pins.filter((pin) => pin.kind === "group").map((pin) => pin.id)),
    [props.projectTree.pins],
  );
  // pinnedEntries resolves the shown pins in their stored order; pins whose item is not listed are skipped.
  // pinnedEntries 按保存顺序解析已显示的置顶；条目未列出的置顶会被跳过。
  const pinnedEntries = React.useMemo(() => {
    const conversationsById = new Map(props.conversations.map((conversation) => [conversation.id, conversation]));
    const projectsById = new Map(props.projects.map((project) => [project.id, project]));
    const groupsById = new Map(tree.groups.map((entry) => [entry.id, entry]));
    return props.projectTree.pins.flatMap((pin): PinnedEntry[] => {
      const id = pinKey(pin);
      if (pin.kind === "session") {
        const conversation = conversationsById.get(pin.id);
        return conversation ? [{ id, pin, kind: "session", conversation }] : [];
      }
      if (pin.kind === "project") {
        const project = projectsById.get(pin.id);
        return project?.pinned ? [{ id, pin, kind: "project", project }] : [];
      }
      const group = groupsById.get(pin.id);
      return group ? [{ id, pin, kind: "group", group }] : [];
    });
  }, [props.conversations, props.projectTree.pins, props.projects, tree.groups]);
  const [pinnedOrdered, showPinnedOrder] = useOptimisticOrder(pinnedEntries);
  const regularIds = React.useMemo(
    () => new Set(tree.groups.flatMap((entry) => entry.projects.map((project) => project.id))),
    [tree],
  );
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: treeKeyboardCoordinates }),
  );

  const toggleCollapsed = (id: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id); else next.add(id);
      writeCollapsedGroups(next);
      return next;
    });
  };

  // runAction persists a change and reports a failure without a dialog to hold it.
  // runAction 持久化变更，并在没有对话框承载时报告失败。
  const runAction = (action: ProjectGroupAction, onFailure?: () => void) => {
    props.onGroupAction(action).catch((error: unknown) => {
      onFailure?.();
      toast.error(error instanceof Error ? error.message : String(error));
    });
  };

  // saveLayout shows a new layout at once and persists it; a rejection restores the authoritative layout.
  // saveLayout 立即显示新布局并持久化；被拒绝时恢复权威布局。
  const saveLayout = (next: ProjectGroupLayout | null) => {
    if (!next) return;
    setPendingLayout(next);
    runAction({ kind: "layout", layout: next }, () => setPendingLayout(null));
  };

  // collisionDetection keeps each drag inside its own kind: pinned items reorder among pinned items and groups among
  // groups, both by the nearest top or bottom edge; projects land on the row nearest the pointer within the group
  // under it, or on the group itself when that group shows no rows.
  // collisionDetection 让每次拖动限定在同类目标内：置顶条目只在置顶条目间排序，分组只在分组间排序，两者都以最近的上边缘或
  // 下边缘为准；项目落在指针所在分组内最近的行上，若该分组没有显示行则落在分组本身。
  const collisionDetection = React.useCallback<CollisionDetection>((args) => {
    const activeId = String(args.active.id);
    const within = (predicate: (id: string) => boolean) => (
      args.droppableContainers.filter((container) => predicate(String(container.id)))
    );
    // byEdge picks the candidate whose top edge or bottom edge lies nearest the dragged item's matching edge. Unlike
    // centre distance it does not depend on height, so a tall group moves past a short one in either direction:
    // moving up lines up top edges and moving down lines up bottom edges.
    // byEdge 选取上边缘或下边缘与拖动条目对应边缘最近的候选项。与中心距离不同，它不受高度影响，因此高大的分组可以向任一
    // 方向越过矮小的分组：上移时对齐上边缘，下移时对齐下边缘。
    const byEdge = (candidates: typeof args.droppableContainers) => {
      const dragged = args.collisionRect;
      let best: { container: (typeof candidates)[number]; distance: number } | null = null;
      for (const container of candidates) {
        const rect = args.droppableRects.get(container.id);
        if (!rect) continue;
        const distance = Math.min(Math.abs(rect.top - dragged.top), Math.abs(rect.bottom - dragged.bottom));
        if (!best || distance < best.distance) best = { container, distance };
      }
      return best ? [{ id: best.container.id, data: { droppableContainer: best.container, value: best.distance } }] : [];
    };
    if (activeId.startsWith(PIN_ID_PREFIX)) {
      return byEdge(within((id) => id.startsWith(PIN_ID_PREFIX)));
    }
    if (activeId.startsWith(GROUP_ID_PREFIX)) {
      return byEdge(within((id) => id.startsWith(GROUP_ID_PREFIX)));
    }
    const containerHits = pointerWithin({ ...args, droppableContainers: within((id) => groupOfDropId(id) !== null) });
    const hit = containerHits[0];
    if (hit) {
      const groupId = groupOfDropId(String(hit.id))!;
      const members = layout.groups.find((entry) => entry.groupId === groupId)?.projectIds ?? [];
      const shown = new Set(members.filter((id) => id === activeId || !collapsed.has(groupId)));
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
  }, [collapsed, layout, regularIds]);

  const handleDragEnd = (event: DragEndEvent) => {
    props.onSortingChange(false);
    if (!event.over || props.filtering) return;
    const activeId = String(event.active.id);
    const overId = String(event.over.id);
    if (activeId === overId) return;
    if (activeId.startsWith(PIN_ID_PREFIX)) {
      const ids = pinnedOrdered.map((entry) => entry.id);
      const from = ids.indexOf(activeId);
      const to = ids.indexOf(overId);
      if (from < 0 || to < 0) return;
      const next = arrayMove([...pinnedOrdered], from, to);
      showPinnedOrder(next.map((entry) => entry.id));
      runAction({ kind: "pins-order", pins: next.map((entry) => entry.pin) });
      return;
    }
    if (activeId.startsWith(GROUP_ID_PREFIX)) {
      if (!overId.startsWith(GROUP_ID_PREFIX)) return;
      saveLayout(moveGroup(layout, activeId.slice(GROUP_ID_PREFIX.length), overId.slice(GROUP_ID_PREFIX.length)));
      return;
    }
    const overGroup = groupOfDropId(overId);
    if (overGroup) {
      saveLayout(moveProject(layout, activeId, { container: overGroup }));
      return;
    }
    const container = containerOf(layout, overId);
    if (container) saveLayout(moveProject(layout, activeId, { container, beforeProjectId: overId }));
  };

  // placementOf builds the move-to-group entries of one unpinned project row.
  // placementOf 为一个未置顶项目行构建“移动到分组”入口。
  const placementOf = (project: ProjectSummary): ProjectRowPlacement => {
    const current = containerOf(layout, project.id) ?? DEFAULT_GROUP_ID;
    return {
      groups: tree.groups.map((entry) => ({ id: entry.id, name: groupName(entry) })),
      currentGroupId: current,
      onMove: (groupId) => saveLayout(moveProject(layout, project.id, { container: groupId })),
      onMoveToNewGroup: () => setNameDialog({ mode: "create", projectIds: [project.id], afterGroupId: current }),
    };
  };

  // renderGroup renders one group frame with its rows; pinned groups sort among the pinned items.
  // renderGroup 渲染一个分组框及其各行；置顶分组在置顶条目之间排序。
  const renderGroup = (entry: TreeGroup, sortableId: string) => {
    const visible = entry.projects.filter(props.isProjectVisible);
    const pinned = pinnedGroupIds.has(entry.id);
    return (
      <SortableProjectGroup
        count={entry.projects.length}
        disabled={props.filtering}
        isDefault={entry.group === null}
        key={sortableId}
        label={groupName(entry)}
        onAddGroup={() => setNameDialog({ mode: "create", projectIds: [], afterGroupId: entry.id })}
        onAddProject={() => props.onCreateProject(entry.id)}
        onDelete={() => runAction({ kind: "delete", groupId: entry.id })}
        onRename={() => entry.group && setNameDialog({ mode: "rename", groupId: entry.id, name: entry.group.name })}
        onTogglePin={() => runAction({ kind: "pin", item: { kind: "group", id: entry.id }, pinned: !pinned })}
        onToggle={() => toggleCollapsed(entry.id)}
        open={props.filtering || !collapsed.has(entry.id)}
        pinned={pinned}
        sessions={entry.projects.reduce((sum, project) => sum + props.sessionCount(project.id), 0)}
        sortableId={sortableId}
        stateKey={entry.id}
      >
        <SortableContext disabled={props.filtering} items={visible.map((project) => project.id)} strategy={verticalListSortingStrategy}>
          {visible.map((project) => props.renderProjectRow(project, placementOf(project)))}
        </SortableContext>
      </SortableProjectGroup>
    );
  };

  const groupShown = (entry: TreeGroup) => !props.filtering || entry.projects.some(props.isProjectVisible);
  const shownPinned = pinnedOrdered.filter((entry) => (
    entry.kind === "session" ? props.isConversationVisible(entry.conversation)
      : entry.kind === "project" ? props.isProjectVisible(entry.project)
        : groupShown(entry.group)
  ));
  const shownGroups = tree.groups.filter((entry) => !pinnedGroupIds.has(entry.id) && groupShown(entry));
  const nothingShown = shownPinned.length === 0 && shownGroups.length === 0;

  return (
    <DndContext
      collisionDetection={collisionDetection}
      onDragCancel={() => props.onSortingChange(false)}
      onDragEnd={handleDragEnd}
      onDragStart={() => props.onSortingChange(true)}
      sensors={sensors}
    >
      {shownPinned.length > 0 ? (
        <section className="vk-tree-pins">
          <div className="vk-tree-head">
            <span className="vk-label"><Pin aria-hidden="true" />{t("sidebar.pinned")}</span>
          </div>
          <SidebarMenu>
            <SortableContext disabled={props.filtering} items={shownPinned.map((entry) => entry.id)} strategy={verticalListSortingStrategy}>
              {shownPinned.map((entry) => (
                entry.kind === "session" ? (
                  <PinnedConversation
                    conversation={entry.conversation}
                    disabled={props.filtering}
                    key={entry.id}
                    render={props.renderPinnedConversation}
                    sortableId={entry.id}
                  />
                ) : entry.kind === "project" ? (
                  <React.Fragment key={entry.id}>{props.renderProjectRow(entry.project, null, entry.id)}</React.Fragment>
                ) : renderGroup(entry.group, entry.id)
              ))}
            </SortableContext>
          </SidebarMenu>
        </section>
      ) : null}
      <section>
        <SidebarMenu>
          <SortableContext disabled={props.filtering} items={shownGroups.map((entry) => GROUP_ID_PREFIX + entry.id)} strategy={verticalListSortingStrategy}>
            {shownGroups.map((entry) => renderGroup(entry, GROUP_ID_PREFIX + entry.id))}
          </SortableContext>
          {props.filtering && nothingShown ? props.emptyNotice : null}
        </SidebarMenu>
      </section>
      <ProjectGroupNameDialog
        target={nameDialog}
        onCancel={() => setNameDialog(null)}
        onConfirm={async (target, name) => {
          await props.onGroupAction(target.mode === "create"
            ? { kind: "create", name, projectIds: target.projectIds, afterGroupId: target.afterGroupId }
            : { kind: "rename", groupId: target.groupId, name });
          setNameDialog(null);
        }}
      />
    </DndContext>
  );
}

// PinnedConversation binds one pinned session row to the pinned order.
// PinnedConversation 将一个置顶会话行绑定到置顶顺序。
function PinnedConversation(props: {
  conversation: ConversationSummary;
  sortableId: string;
  disabled: boolean;
  render: (conversation: ConversationSummary, sortable: SortableRowBinding) => React.ReactNode;
}) {
  const { t } = useI18n();
  const sortable = useSortableRowBinding(
    props.sortableId,
    formatMessage(t("sidebar.pinReorder"), { name: props.conversation.title }),
    props.disabled,
  );
  return <>{props.render(props.conversation, sortable)}</>;
}

// ProjectGroupFrameProps describes one collapsible group heading and its body.
// ProjectGroupFrameProps 描述一个可折叠分组标题及其内容。
interface ProjectGroupFrameProps {
  // Collapsed-state key and DOM marker of the group.
  // 分组的折叠状态键与 DOM 标记。
  stateKey: string;
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
  // Rows shown when open.
  // 展开时显示的行。
  children: React.ReactNode;
}

// SortableProjectGroup renders a group heading: a caret, the name, the project count, a [+] menu that adds a
// project or a group, a menu to pin, rename or delete it, and a grip, with the rows below while open. The default
// group can only be pinned. An open group without rows says where projects come from.
// SortableProjectGroup 渲染分组标题：展开箭头、名称、项目数量、用于新建项目或分组的 [+] 菜单、用于置顶、重命名或删除的菜单
// 以及拖动手柄，展开时其下显示各行。默认分组只能置顶。没有行的展开分组会说明项目从何而来。
function SortableProjectGroup(props: ProjectGroupFrameProps & {
  // Sortable id: an unpinned group id or a pinned item id.
  // 可排序标识：未置顶分组标识或置顶条目标识。
  sortableId: string;
  // Disables sorting while the list is filtered.
  // 列表被筛选时禁用排序。
  disabled: boolean;
  // Whether this is the default group, which cannot be renamed or deleted.
  // 是否为默认分组；默认分组不能重命名或删除。
  isDefault: boolean;
  // Whether the group is pinned.
  // 分组是否已置顶。
  pinned: boolean;
  onAddProject: () => void;
  onAddGroup: () => void;
  onTogglePin: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  const { t } = useI18n();
  const sortable = useVerticalSortableItem(props.sortableId, props.disabled);
  const activeId = sortable.active ? String(sortable.active.id) : null;
  const draggingProject = activeId !== null && !activeId.startsWith(GROUP_ID_PREFIX) && !activeId.startsWith(PIN_ID_PREFIX);
  const reorderLabel = formatMessage(t(props.pinned ? "sidebar.pinReorder" : "sidebar.group.reorder"), { name: props.label });
  const addLabel = formatMessage(t("sidebar.group.add"), { name: props.label });
  const summary = formatMessage(t("sidebar.group.summary"), { projects: props.count, sessions: props.sessions });
  return (
    <li
      className="vk-tree-group"
      data-default={props.isDefault ? "" : undefined}
      data-dragging={sortable.isDragging ? "" : undefined}
      data-drop={draggingProject && sortable.isOver ? "" : undefined}
      data-group={props.stateKey}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
    >
      <div className="vk-tree-item" data-kind="group">
        <button aria-expanded={props.open} className="vk-group-row" onClick={props.onToggle} title={summary} type="button">
          <ChevronRight aria-hidden="true" className="vk-tree-caret" />
          <span className="vk-label">{props.label}</span>
          <small className="vk-num">{formatMessage(t("sidebar.group.projectCount"), { count: props.count })}</small>
        </button>
        <span className="vk-tree-actions">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <IconButton aria-label={addLabel} size="xs" title={addLabel}>
                <Plus aria-hidden="true" />
              </IconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={props.onAddProject}>
                <FolderPlus aria-hidden="true" />
                <span>{t("sidebar.newProject")}</span>
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={props.onAddGroup}>
                <FolderTree aria-hidden="true" />
                <span>{t("sidebar.newGroup")}</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <IconButton aria-label={t("sidebar.group.menu")} size="xs" title={t("sidebar.group.menu")}>
                <Ellipsis aria-hidden="true" />
              </IconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={props.onTogglePin}>
                {props.pinned ? <PinOff aria-hidden="true" /> : <Pin aria-hidden="true" />}
                <span>{t(props.pinned ? "sidebar.group.unpin" : "sidebar.group.pin")}</span>
              </DropdownMenuItem>
              {props.isDefault ? null : (
                <>
                  <DropdownMenuItem onSelect={props.onRename}>
                    <Pencil aria-hidden="true" />
                    <span>{t("sidebar.group.rename")}</span>
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onSelect={props.onDelete} variant="destructive">
                    <Trash2 aria-hidden="true" />
                    <span>{t("sidebar.group.delete")}</span>
                  </DropdownMenuItem>
                </>
              )}
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
        </span>
      </div>
      {props.open ? (
        <ul className="vk-group-body">
          {props.count === 0 ? <li className="vk-tree-blank">{t("sidebar.group.dropHere")}</li> : props.children}
        </ul>
      ) : null}
    </li>
  );
}

// GroupNameDialogTarget is the pending create or rename request of the group name dialog.
// GroupNameDialogTarget 是分组名称对话框待处理的新建或重命名请求。
type GroupNameDialogTarget =
  | { mode: "create"; projectIds: string[]; afterGroupId?: string }
  | { mode: "rename"; groupId: string; name: string };

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
    setValue(props.target?.mode === "rename" ? props.target.name : "");
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
