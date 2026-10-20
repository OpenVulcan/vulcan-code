import { describe, expect, it } from "vitest";
import type { ProjectSummary } from "@/types";
import { buildProjectTreeLayout, layoutOf, moveGroup, moveProject, UNGROUPED_CONTAINER } from "./project-group-layout";

// project builds one visible project row; pinned rows stay outside groups.
// project 构建一个可见项目行；置顶行不进入分组。
const project = (id: string, pinned = false): ProjectSummary => ({ id, name: id, path: `D:/${id}`, pinned });

const projects = [project("pin", true), project("a"), project("b"), project("c"), project("d")];
const groups = [
  { id: "g1", name: "One", projectIds: ["b", "pin", "gone"] },
  { id: "g2", name: "Two", projectIds: ["c"] },
];

describe("project group layout", () => {
  it("keeps pinned projects out of groups and lists the rest as ungrouped in service order", () => {
    const tree = buildProjectTreeLayout(projects, groups);
    expect(tree.pinned.map((entry) => entry.id)).toEqual(["pin"]);
    expect(tree.groups.map((entry) => entry.projects.map((member) => member.id))).toEqual([["b"], ["c"]]);
    expect(tree.ungrouped.map((entry) => entry.id)).toEqual(["a", "d"]);
  });

  it("prefers a pending layout only while it names exactly the stored groups", () => {
    const pending = { groups: [{ groupId: "g2", projectIds: ["a", "c"] }, { groupId: "g1", projectIds: [] }], ungroupedProjectIds: ["d", "b"] };
    expect(layoutOf(buildProjectTreeLayout(projects, groups, pending))).toEqual(pending);
    const stale = { groups: [{ groupId: "g1", projectIds: [] }], ungroupedProjectIds: [] };
    expect(layoutOf(buildProjectTreeLayout(projects, groups, stale))).toEqual(layoutOf(buildProjectTreeLayout(projects, groups)));
  });

  it("moves projects between containers and sorts within one", () => {
    const layout = layoutOf(buildProjectTreeLayout(projects, groups));
    expect(moveProject(layout, "a", { container: "g2" })?.groups[1].projectIds).toEqual(["c", "a"]);
    expect(moveProject(layout, "a", { container: "g2", beforeProjectId: "c" })?.groups[1].projectIds).toEqual(["a", "c"]);
    const out = moveProject(layout, "b", { container: UNGROUPED_CONTAINER });
    expect(out?.groups[0].projectIds).toEqual([]);
    expect(out?.ungroupedProjectIds).toEqual(["a", "d", "b"]);
    expect(moveProject(layout, "d", { container: UNGROUPED_CONTAINER, beforeProjectId: "a" })?.ungroupedProjectIds).toEqual(["d", "a"]);
    expect(moveProject(layout, "a", { container: UNGROUPED_CONTAINER })).toBeNull();
  });

  it("reorders groups", () => {
    const layout = layoutOf(buildProjectTreeLayout(projects, groups));
    expect(moveGroup(layout, "g1", "g2")?.groups.map((entry) => entry.groupId)).toEqual(["g2", "g1"]);
    expect(moveGroup(layout, "g1", "g1")).toBeNull();
  });
});
