import { arrayMove } from "@dnd-kit/sortable";
import type { ProjectGroupLayoutEntry, ProjectGroupSummary, ProjectSummary } from "@/types";

// UNGROUPED_CONTAINER names the pseudo-group that holds unpinned projects outside every group.
// UNGROUPED_CONTAINER 命名容纳不属于任何分组的未置顶项目的伪分组。
export const UNGROUPED_CONTAINER = "ungrouped";

// ProjectGroupLayout is the complete unpinned layout the service stores: group order, members, ungrouped order.
// ProjectGroupLayout 是服务保存的完整未置顶布局：分组顺序、成员与未分组顺序。
export interface ProjectGroupLayout {
  // Every group in display order with its members.
  // 按显示顺序排列的每个分组及其成员。
  groups: ProjectGroupLayoutEntry[];
  // Unpinned projects outside every group, in display order.
  // 不属于任何分组的未置顶项目，按显示顺序排列。
  ungroupedProjectIds: string[];
}

// ProjectTreeLayout is what the code flow renders: pinned projects, then groups, then ungrouped projects.
// ProjectTreeLayout 是代码流渲染的内容：置顶项目、分组、未分组项目。
export interface ProjectTreeLayout {
  // Pinned projects; they stay in their own section and never appear inside a group.
  // 置顶项目；它们留在自己的分区，不会出现在分组中。
  pinned: ProjectSummary[];
  // Groups in display order with their resolved unpinned members.
  // 按显示顺序排列的分组及其解析后的未置顶成员。
  groups: Array<{ group: ProjectGroupSummary; projects: ProjectSummary[] }>;
  // Unpinned projects outside every group.
  // 不属于任何分组的未置顶项目。
  ungrouped: ProjectSummary[];
}

// buildProjectTreeLayout resolves the stored groups onto the visible projects. A pending layout, saved but not yet
// confirmed, takes the place of the stored one while it still names exactly the stored groups.
// buildProjectTreeLayout 将已保存分组解析到可见项目上。已提交但尚未确认的待定布局在仍恰好包含已保存分组时取代它。
export function buildProjectTreeLayout(
  projects: readonly ProjectSummary[],
  groups: readonly ProjectGroupSummary[],
  pending: ProjectGroupLayout | null = null,
): ProjectTreeLayout {
  const byId = new Map(projects.map((project) => [project.id, project]));
  const regular = projects.filter((project) => !project.pinned);
  const regularIds = new Set(regular.map((project) => project.id));
  const groupsById = new Map(groups.map((group) => [group.id, group]));
  const usePending = pending !== null
    && pending.groups.length === groups.length
    && pending.groups.every((entry) => groupsById.has(entry.groupId));
  const entries: ProjectGroupLayoutEntry[] = usePending
    ? pending.groups
    : groups.map((group) => ({ groupId: group.id, projectIds: group.projectIds }));
  const placed = new Set<string>();
  const resolve = (projectIds: readonly string[]) => projectIds.flatMap((projectId) => {
    if (!regularIds.has(projectId) || placed.has(projectId)) return [];
    placed.add(projectId);
    return [byId.get(projectId)!];
  });
  const resolvedGroups = entries.map((entry) => ({
    group: groupsById.get(entry.groupId)!,
    projects: resolve(entry.projectIds),
  }));
  // Ungrouped projects follow the pending order first, then the service order for anything it does not name.
  // 未分组项目先按待定顺序排列，待定顺序未包含的再按服务顺序排列。
  const ungrouped = [
    ...resolve(usePending ? pending.ungroupedProjectIds : []),
    ...resolve(regular.map((project) => project.id)),
  ];
  return {
    pinned: projects.filter((project) => project.pinned),
    groups: resolvedGroups,
    ungrouped,
  };
}

// layoutOf converts a rendered tree back into the layout request the service accepts.
// layoutOf 将已渲染的项目树转换回服务接受的布局请求。
export function layoutOf(tree: ProjectTreeLayout): ProjectGroupLayout {
  return {
    groups: tree.groups.map((entry) => ({
      groupId: entry.group.id,
      projectIds: entry.projects.map((project) => project.id),
    })),
    ungroupedProjectIds: tree.ungrouped.map((project) => project.id),
  };
}

// containerOf finds the group holding one project, or UNGROUPED_CONTAINER; null when the layout lacks it.
// containerOf 查找包含某项目的分组，或 UNGROUPED_CONTAINER；布局中没有该项目时返回 null。
export function containerOf(layout: ProjectGroupLayout, projectId: string): string | null {
  if (layout.ungroupedProjectIds.includes(projectId)) return UNGROUPED_CONTAINER;
  return layout.groups.find((entry) => entry.projectIds.includes(projectId))?.groupId ?? null;
}

// moveProject moves one project into a container, before another project or at its end. Within one container it
// behaves like a sortable move, so dropping onto a later row places the project after that row.
// moveProject 将一个项目移入某容器，放在另一项目之前或容器末尾。同一容器内的行为与排序移动相同，因此拖到后面的行会放在该行之后。
//
// Returns the new layout, or null when nothing changes or the project or target is unknown.
// 返回新布局；无变化或项目、目标未知时返回 null。
export function moveProject(
  layout: ProjectGroupLayout,
  projectId: string,
  target: { container: string; beforeProjectId?: string },
): ProjectGroupLayout | null {
  const source = containerOf(layout, projectId);
  const lists = new Map<string, string[]>([
    [UNGROUPED_CONTAINER, [...layout.ungroupedProjectIds]],
    ...layout.groups.map((entry): [string, string[]] => [entry.groupId, [...entry.projectIds]]),
  ]);
  const destination = lists.get(target.container);
  if (!source || !destination) return null;
  if (source === target.container) {
    const from = destination.indexOf(projectId);
    // Dropping onto its own container rather than a row leaves the project where it is.
    // 拖放到自身所在容器而非某一行时，项目保持原位。
    const to = target.beforeProjectId ? destination.indexOf(target.beforeProjectId) : from;
    if (to < 0 || from === to) return null;
    lists.set(source, arrayMove(destination, from, to));
  } else {
    lists.set(source, lists.get(source)!.filter((id) => id !== projectId));
    const before = target.beforeProjectId ? destination.indexOf(target.beforeProjectId) : -1;
    destination.splice(before < 0 ? destination.length : before, 0, projectId);
  }
  return {
    groups: layout.groups.map((entry) => ({ groupId: entry.groupId, projectIds: lists.get(entry.groupId)! })),
    ungroupedProjectIds: lists.get(UNGROUPED_CONTAINER)!,
  };
}

// moveGroup places one group at another group's position. Returns null when nothing changes.
// moveGroup 将一个分组放到另一分组的位置。无变化时返回 null。
export function moveGroup(layout: ProjectGroupLayout, groupId: string, overGroupId: string): ProjectGroupLayout | null {
  const from = layout.groups.findIndex((entry) => entry.groupId === groupId);
  const to = layout.groups.findIndex((entry) => entry.groupId === overGroupId);
  if (from < 0 || to < 0 || from === to) return null;
  return { ...layout, groups: arrayMove(layout.groups, from, to) };
}
