import { describe, expect, it } from "vitest";
import type { ProjectSummary, ProjectTreeSummary } from "@/types";
import { buildProjectTreeLayout, DEFAULT_GROUP_ID, layoutOf, moveGroup, moveProject } from "./project-group-layout";

// project builds one visible project row; pinned rows stay outside groups.
// project 构建一个可见项目行；置顶行不进入分组。
const project = (id: string, pinned = false): ProjectSummary => ({ id, name: id, path: `D:/${id}`, pinned });

const projects = [project("pin", true), project("a"), project("b"), project("c"), project("d")];
const tree: ProjectTreeSummary = {
  groups: [
    { id: "g1", name: "One", projectIds: ["b", "pin", "gone"] },
    { id: "g2", name: "Two", projectIds: ["c"] },
  ],
  defaultGroupIndex: 1,
  pins: [],
};

// membersOf lists each group id with its member ids.
// membersOf 列出每个分组标识及其成员标识。
const membersOf = (layout: ReturnType<typeof layoutOf> | null) => layout?.groups.map((entry) => [entry.groupId, entry.projectIds]);

describe("project group layout", () => {
  it("places the default group at its position and gives it every unpinned project outside the user groups", () => {
    expect(membersOf(layoutOf(buildProjectTreeLayout(projects, tree)))).toEqual([
      ["g1", ["b"]],
      [DEFAULT_GROUP_ID, ["a", "d"]],
      ["g2", ["c"]],
    ]);
  });

  it("prefers a pending layout only while it names exactly the stored groups", () => {
    const pending = {
      groups: [
        { groupId: DEFAULT_GROUP_ID, projectIds: ["d", "b"] },
        { groupId: "g2", projectIds: ["a", "c"] },
        { groupId: "g1", projectIds: [] },
      ],
    };
    expect(layoutOf(buildProjectTreeLayout(projects, tree, pending))).toEqual(pending);
    const stale = { groups: [{ groupId: "g1", projectIds: [] }, { groupId: DEFAULT_GROUP_ID, projectIds: [] }] };
    expect(layoutOf(buildProjectTreeLayout(projects, tree, stale))).toEqual(layoutOf(buildProjectTreeLayout(projects, tree)));
  });

  it("moves projects between groups and sorts within one", () => {
    const layout = layoutOf(buildProjectTreeLayout(projects, tree));
    expect(moveProject(layout, "a", { container: "g2" })?.groups[2].projectIds).toEqual(["c", "a"]);
    expect(moveProject(layout, "a", { container: "g2", beforeProjectId: "c" })?.groups[2].projectIds).toEqual(["a", "c"]);
    const out = moveProject(layout, "b", { container: DEFAULT_GROUP_ID });
    expect(membersOf(out)?.slice(0, 2)).toEqual([["g1", []], [DEFAULT_GROUP_ID, ["a", "d", "b"]]]);
    expect(moveProject(layout, "d", { container: DEFAULT_GROUP_ID, beforeProjectId: "a" })?.groups[1].projectIds).toEqual(["d", "a"]);
    expect(moveProject(layout, "a", { container: DEFAULT_GROUP_ID })).toBeNull();
  });

  it("reorders groups, the default group included", () => {
    const layout = layoutOf(buildProjectTreeLayout(projects, tree));
    expect(moveGroup(layout, DEFAULT_GROUP_ID, "g1")?.groups.map((entry) => entry.groupId)).toEqual([DEFAULT_GROUP_ID, "g1", "g2"]);
    expect(moveGroup(layout, "g1", "g1")).toBeNull();
  });
});
