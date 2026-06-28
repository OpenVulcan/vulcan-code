import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import type { WorkspaceDiffSnapshotResponse } from "@/diff/types";
// WorkspaceProjectGitSummaryEntry stores shared Git status for one workspace project.
// WorkspaceProjectGitSummaryEntry 保存单个工作区项目共享的 Git 状态。
export interface WorkspaceProjectGitSummaryEntry {
  // Compact review summary derived from live Git snapshots.
  // 从实时 Git 快照派生出的紧凑审核摘要。
  summary: ConversationSessionReviewSummary | null;
  // Latest unstaged diff snapshot used to derive summary counts.
  // 用于派生摘要计数的最近未暂存 diff 快照。
  unstagedSnapshot: WorkspaceDiffSnapshotResponse | null;
  // Latest staged diff snapshot used to derive summary counts.
  // 用于派生摘要计数的最近已暂存 diff 快照。
  stagedSnapshot: WorkspaceDiffSnapshotResponse | null;
  // Stable workspace path that owns this summary.
  // 拥有该摘要的稳定工作区路径。
  workspacePath: string;
  // Optional project id used by backend Git requests.
  // 后端 Git 请求使用的可选项目标识。
  projectId: string | null;
  // Monotonic revision bumped whenever the project Git state changes.
  // 项目 Git 状态变化时递增的单调版本。
  revision: number;
  // Whether a project-level summary refresh is currently running.
  // 项目级摘要刷新当前是否正在运行。
  loading: boolean;
  // Latest refresh error for diagnostics.
  // 最近一次刷新错误，供诊断使用。
  error: string | null;
  // Latest project refresh request id accepted by this entry.
  // 该条目当前接受的最近项目刷新请求标识。
  refreshRequestId: number;
}

// WorkspaceProjectGitStateMap stores shared Git state keyed by workspace path.
// WorkspaceProjectGitStateMap 按工作区路径保存共享 Git 状态。
export type WorkspaceProjectGitStateMap = Record<string, WorkspaceProjectGitSummaryEntry>;

// buildWorkspaceProjectGitSummary creates the shared compact Git summary from live snapshots.
// buildWorkspaceProjectGitSummary 根据实时快照创建共享的紧凑 Git 摘要。
export function buildWorkspaceProjectGitSummary(
  // Unstaged diff snapshot returned by the backend.
  // 后端返回的未暂存 diff 快照。
  unstagedSnapshot: WorkspaceDiffSnapshotResponse | null,
  // Staged diff snapshot returned by the backend.
  // 后端返回的已暂存 diff 快照。
  stagedSnapshot: WorkspaceDiffSnapshotResponse | null,
): ConversationSessionReviewSummary | null {
  const primarySnapshot = unstagedSnapshot ?? stagedSnapshot;
  if (!primarySnapshot) {
    return null;
  }
  if (!workspaceDiffSnapshotHasGitRepository(primarySnapshot)) {
    return null;
  }
  const snapshots = [unstagedSnapshot, stagedSnapshot].filter((snapshot): snapshot is WorkspaceDiffSnapshotResponse => Boolean(snapshot));
  return {
    additions: snapshots.reduce((total, snapshot) => total + snapshot.additions, 0),
    branchName: primarySnapshot.currentBranch ?? null,
    branchNames: (primarySnapshot.branches ?? []).map((branch) => branch.name),
    changedFiles: snapshots.reduce((total, snapshot) => total + snapshot.changedFiles, 0),
    deletions: snapshots.reduce((total, snapshot) => total + snapshot.deletions, 0),
    stagedAdditions: stagedSnapshot?.additions ?? 0,
    stagedChangedFiles: stagedSnapshot?.changedFiles ?? 0,
    stagedDeletions: stagedSnapshot?.deletions ?? 0,
    unstagedAdditions: unstagedSnapshot?.additions ?? 0,
    unstagedChangedFiles: unstagedSnapshot?.changedFiles ?? 0,
    unstagedDeletions: unstagedSnapshot?.deletions ?? 0,
    unpushedCommits: primarySnapshot.currentBranchUnpushedCommits ?? 0,
  };
}

// workspaceDiffSnapshotHasGitRepository checks whether a diff snapshot came from a real Git repository.
// workspaceDiffSnapshotHasGitRepository 检查 diff 快照是否来自真实 Git 仓库。
export function workspaceDiffSnapshotHasGitRepository(
  // Snapshot returned by the workspace diff backend.
  // 工作区 diff 后端返回的快照。
  snapshot: WorkspaceDiffSnapshotResponse,
): boolean {
  return Boolean(
    snapshot.currentBranch
    || snapshot.defaultBranch
    || snapshot.selectedBaseBranch
    || snapshot.selectedCompareBranch
    || snapshot.selectedCommitHash
    || snapshot.branches?.length
    || snapshot.recentCommits?.length
  );
}

