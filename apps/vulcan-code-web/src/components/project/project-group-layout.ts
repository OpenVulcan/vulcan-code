import { arrayMove } from "@dnd-kit/sortable";
import type { ProjectGroupLayoutEntry, ProjectGroupSummary, ProjectSummary, ProjectTreeSummary } from "@/types";

// DEFAULT_GROUP_ID names the default group, which holds every project outside the user groups.
// DEFAULT_GROUP_ID 命名默认分组；该分组容纳不属于任何用户分组的项目。
export const DEFAULT_GROUP_ID = "default";

// ProjectGroupLayout is the complete unpinned layout the service stores: every group in order, the default group
// included, with its members.
// ProjectGroupLayout 是服务保存的完整未置顶布局：按顺序排列的所有分组（含默认分组）及其成员。
export interface ProjectGroupLayout {
  // Every group in display order with its members.
  // 按显示顺序排列的每个分组及其成员。
  groups: ProjectGroupLayoutEntry[];
}

// TreeGroup is one group as the code flow shows it; the default group has no stored name.
// TreeGroup 是代码流显示的一个分组；默认分组没有保存的名称。
export interface TreeGroup {
  // Group identifier; DEFAULT_GROUP_ID for the default group.
  // 分组标识；默认分组为 DEFAULT_GROUP_ID。
  id: string;
  // User group, or null for the default group.
  // 用户分组；默认分组为 null。
  group: ProjectGroupSummary | null;
  // Unpinned member projects in display order.
  // 按显示顺序排列的未置顶成员项目。
  projects: ProjectSummary[];
}

// ProjectTreeLayout is every group of the code flow in display order with its resolved unpinned members.
// ProjectTreeLayout 是代码流按显示顺序排列的所有分组及其解析后的未置顶成员。
export interface ProjectTreeLayout {
  groups: TreeGroup[];
}

// EMPTY_PROJECT_TREE stands in before the service reports a tree: no user groups and nothing pinned.
// EMPTY_PROJECT_TREE 在服务报告项目树之前使用：没有用户分组，也没有置顶。
export const EMPTY_PROJECT_TREE: ProjectTreeSummary = { groups: [], defaultGroupIndex: 0, pins: [] };

// storedGroupOrder lists every group id in display order, the default group at its stored position.
// storedGroupOrder 按显示顺序列出每个分组标识，默认分组位于其保存的位置。
export function storedGroupOrder(tree: ProjectTreeSummary): string[] {
  const ids = tree.groups.map((group) => group.id);
  ids.splice(Math.min(tree.defaultGroupIndex, ids.length), 0, DEFAULT_GROUP_ID);
  return ids;
}

// buildProjectTreeLayout resolves the stored tree onto the visible unpinned projects. User groups take their stored
// members; the default group takes every other unpinned project in service order. A pending layout, saved but not
// yet confirmed, takes the place of the stored one while it still names exactly the stored groups.
// buildProjectTreeLayout 将已保存的项目树解析到可见的未置顶项目上。用户分组取其保存的成员；默认分组按服务顺序取其余
// 所有未置顶项目。已提交但尚未确认的待定布局在仍恰好包含已保存分组时取代它。
export function buildProjectTreeLayout(
  projects: readonly ProjectSummary[],
  tree: ProjectTreeSummary,
  pending: ProjectGroupLayout | null = null,
): ProjectTreeLayout {
  const byId = new Map(projects.map((project) => [project.id, project]));
  const regular = projects.filter((project) => !project.pinned);
  const regularIds = new Set(regular.map((project) => project.id));
  const groupsById = new Map(tree.groups.map((group) => [group.id, group]));
  const storedOrder = storedGroupOrder(tree);
  const usePending = pending !== null
    && pending.groups.length === storedOrder.length
    && pending.groups.every((entry) => entry.groupId === DEFAULT_GROUP_ID || groupsById.has(entry.groupId));
  const entries: ProjectGroupLayoutEntry[] = usePending
    ? pending.groups
    : storedOrder.map((id) => ({ groupId: id, projectIds: groupsById.get(id)?.projectIds ?? [] }));
  const placed = new Set<string>();
  const resolve = (projectIds: readonly string[]) => projectIds.flatMap((projectId) => {
    if (!regularIds.has(projectId) || placed.has(projectId)) return [];
    placed.add(projectId);
    return [byId.get(projectId)!];
  });
  // User groups resolve first so the default group can take every project they leave.
  // 先解析用户分组，使默认分组能够取得它们留下的所有项目。
  const members = new Map(entries
    .filter((entry) => entry.groupId !== DEFAULT_GROUP_ID)
    .map((entry) => [entry.groupId, resolve(entry.projectIds)]));
  const defaultEntry = entries.find((entry) => entry.groupId === DEFAULT_GROUP_ID);
  members.set(DEFAULT_GROUP_ID, [
    ...resolve(defaultEntry?.projectIds ?? []),
    ...resolve(regular.map((project) => project.id)),
  ]);
  return {
    groups: entries.map((entry) => ({
      id: entry.groupId,
      group: groupsById.get(entry.groupId) ?? null,
      projects: members.get(entry.groupId) ?? [],
    })),
  };
}

// layoutOf converts a rendered tree back into the layout request the service accepts.
// layoutOf 将已渲染的项目树转换回服务接受的布局请求。
export function layoutOf(tree: ProjectTreeLayout): ProjectGroupLayout {
  return {
    groups: tree.groups.map((entry) => ({ groupId: entry.id, projectIds: entry.projects.map((project) => project.id) })),
  };
}

// containerOf finds the group holding one project; null when the layout lacks it.
// containerOf 查找包含某项目的分组；布局中没有该项目时返回 null。
export function containerOf(layout: ProjectGroupLayout, projectId: string): string | null {
  return layout.groups.find((entry) => entry.projectIds.includes(projectId))?.groupId ?? null;
}

// moveProject moves one project into a group, before another project or at its end. Within one group it behaves
// like a sortable move, so dropping onto a later row places the project after that row.
// moveProject 将一个项目移入某分组，放在另一项目之前或分组末尾。同一分组内的行为与排序移动相同，因此拖到后面的行会
// 放在该行之后。
//
// Returns the new layout, or null when nothing changes or the project or target is unknown.
// 返回新布局；无变化或项目、目标未知时返回 null。
export function moveProject(
  layout: ProjectGroupLayout,
  projectId: string,
  target: { container: string; beforeProjectId?: string },
): ProjectGroupLayout | null {
  const source = containerOf(layout, projectId);
  const lists = new Map(layout.groups.map((entry): [string, string[]] => [entry.groupId, [...entry.projectIds]]));
  const destination = lists.get(target.container);
  if (!source || !destination) return null;
  if (source === target.container) {
    const from = destination.indexOf(projectId);
    // Dropping onto its own group rather than a row leaves the project where it is.
    // 拖放到自身所在分组而非某一行时，项目保持原位。
    const to = target.beforeProjectId ? destination.indexOf(target.beforeProjectId) : from;
    if (to < 0 || from === to) return null;
    lists.set(source, arrayMove(destination, from, to));
  } else {
    lists.set(source, lists.get(source)!.filter((id) => id !== projectId));
    const before = target.beforeProjectId ? destination.indexOf(target.beforeProjectId) : -1;
    destination.splice(before < 0 ? destination.length : before, 0, projectId);
  }
  return { groups: layout.groups.map((entry) => ({ groupId: entry.groupId, projectIds: lists.get(entry.groupId)! })) };
}

// moveGroup places one group at another group's position. Returns null when nothing changes.
// moveGroup 将一个分组放到另一分组的位置。无变化时返回 null。
export function moveGroup(layout: ProjectGroupLayout, groupId: string, overGroupId: string): ProjectGroupLayout | null {
  const from = layout.groups.findIndex((entry) => entry.groupId === groupId);
  const to = layout.groups.findIndex((entry) => entry.groupId === overGroupId);
  if (from < 0 || to < 0 || from === to) return null;
  return { groups: arrayMove(layout.groups, from, to) };
}
