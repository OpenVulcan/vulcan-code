import { describe, expect, it } from "vitest";
import type { WorkspaceDiffSnapshotResponse } from "@/diff/types";
import {
  mergeWorkspaceDiffSnapshotIntoProjectGitState,
  type WorkspaceProjectGitRepositoryState,
  type WorkspaceProjectGitStateMap,
} from "./git-summary";

const PROJECT = "X:/audit-project";

// snapshot builds a worktree snapshot; a branch name marks it as coming from a real repository.
// snapshot 构建工作树快照；带分支名表示其来自真实仓库。
function snapshot(changedFiles: number, currentBranch: string | null): WorkspaceDiffSnapshotResponse {
  return { gitRepository: currentBranch !== null, repositoryRoot: PROJECT, unifiedDiff: "", changedFiles, additions: changedFiles, deletions: 0, currentBranch };
}

// repository builds one catalog-reported repository node.
// repository 构建一个由目录报告的仓库节点。
function repository(repositoryKey: string): WorkspaceProjectGitRepositoryState {
  return {
    repositoryKey,
    isPrimary: repositoryKey === ".",
    repositoryName: repositoryKey === "." ? "audit-project" : repositoryKey,
    relativePath: repositoryKey,
    monitoringEnabled: true,
    summary: null,
    unstagedSnapshot: null,
    stagedSnapshot: null,
    workspacePath: PROJECT,
    projectId: "p1",
    revision: 3,
    loading: false,
    error: null,
  };
}

// catalogState builds the project state as the backend catalog reported it.
// catalogState 构建后端目录报告后的项目状态。
function catalogState(repositoryKeys: string[]): WorkspaceProjectGitStateMap {
  return {
    [PROJECT]: {
      summary: null,
      unstagedSnapshot: null,
      stagedSnapshot: null,
      workspacePath: PROJECT,
      projectId: "p1",
      revision: 5,
      loading: false,
      error: null,
      refreshRequestId: 2,
      repositories: repositoryKeys.map(repository),
      primaryRepositoryKey: repositoryKeys[0] ?? null,
    },
  };
}

describe("mergeWorkspaceDiffSnapshotIntoProjectGitState", () => {
  // Verifies a folder without Git never gains a repository from a review panel snapshot.
  // 验证没有 Git 的目录不会因审核面板快照而凭空出现仓库。
  it.each<[string, WorkspaceProjectGitStateMap]>([
    ["no project state yet", {}],
    ["an empty catalog", catalogState([])],
  ])("does not invent a repository for %s", (_case, current) => {
    const next = mergeWorkspaceDiffSnapshotIntoProjectGitState(current, PROJECT, "p1", { mode: "unstaged" }, snapshot(0, null));
    expect(next).toBe(current);
  });

  // Verifies a snapshot for a repository the catalog does not know is ignored.
  // 验证目录未知仓库的快照会被忽略。
  it("ignores a snapshot for an unknown repository key", () => {
    const current = catalogState(["."]);
    const next = mergeWorkspaceDiffSnapshotIntoProjectGitState(current, PROJECT, "p1", { mode: "staged", repositoryKey: "vendor/lib" }, snapshot(1, "main"));
    expect(next).toBe(current);
  });

  // Verifies a known repository takes the snapshot and keeps its catalog identity.
  // 验证已知仓库接收快照并保留其目录身份。
  it("updates a known repository and the project summary", () => {
    const current = catalogState([".", "vendor/lib"]);
    const next = mergeWorkspaceDiffSnapshotIntoProjectGitState(current, PROJECT, "p1", { mode: "unstaged" }, snapshot(4, "main"));
    const entry = next[PROJECT];
    expect(entry.repositories.map((item) => item.repositoryKey)).toEqual([".", "vendor/lib"]);
    expect(entry.repositories[0]).toMatchObject({ repositoryName: "audit-project", revision: 4, unstagedSnapshot: { changedFiles: 4 } });
    expect(entry.repositories[1]).toBe(current[PROJECT].repositories[1]);
    expect(entry.summary).toMatchObject({ changedFiles: 4, branchName: "main" });
    expect(entry).toMatchObject({ revision: 6, refreshRequestId: 2, primaryRepositoryKey: "." });
  });

  // Verifies branch and history views do not touch the worktree summary.
  // 验证分支与历史视图不会改动工作树摘要。
  it("leaves the state alone for non-worktree views", () => {
    const current = catalogState(["."]);
    const next = mergeWorkspaceDiffSnapshotIntoProjectGitState(current, PROJECT, "p1", { mode: "branch" }, snapshot(2, "main"));
    expect(next).toBe(current);
  });
});
