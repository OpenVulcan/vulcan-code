import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import type { WorkspaceDiffSnapshotResponse } from "@/diff/types";

// WorkspaceProjectGitRepositoryState stores independent Git state for one repository node.
// WorkspaceProjectGitRepositoryState 保存单个仓库节点的独立 Git 状态。
export interface WorkspaceProjectGitRepositoryState {
  // Stable repository identity within the selected project.
  // 当前项目内稳定的仓库身份。
  repositoryKey: string;
  // Whether this repository is the deterministic primary repository for the project.
  // 该仓库是否是项目确定性的主要仓库。
  isPrimary: boolean;
  // Display name shown in compact review nodes.
  // 紧凑审查节点显示的名称。
  repositoryName: string;
  // Relative path shown as secondary repository context.
  // 作为仓库次级上下文显示的相对路径。
  relativePath: string;
  // Whether live monitoring is enabled for this repository.
  // 该仓库是否启用实时监听。
  monitoringEnabled: boolean;
  // Repository summary derived from its own staged and unstaged snapshots.
  // 基于该仓库独立暂存与未暂存快照派生的摘要。
  summary: ConversationSessionReviewSummary | null;
  // Latest unstaged snapshot for this repository.
  // 该仓库最近的未暂存快照。
  unstagedSnapshot: WorkspaceDiffSnapshotResponse | null;
  // Latest staged snapshot for this repository.
  // 该仓库最近的已暂存快照。
  stagedSnapshot: WorkspaceDiffSnapshotResponse | null;
  // Stable project path owning this repository node.
  // 拥有该仓库节点的稳定项目路径。
  workspacePath: string;
  // Optional project id used by backend Git requests.
  // 后端 Git 请求使用的可选项目标识。
  projectId: string | null;
  // Monotonic revision bumped whenever this repository changes.
  // 该仓库变化时递增的单调版本。
  revision: number;
  // Whether this repository refresh is running.
  // 该仓库刷新是否正在运行。
  loading: boolean;
  // Latest repository refresh error.
  // 最近一次仓库刷新错误。
  error: string | null;
}
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
  // Independent repository nodes discovered below the project.
  // 项目下发现的独立仓库节点。
  repositories: WorkspaceProjectGitRepositoryState[];
  // Stable primary repository key used by compact collapsed summaries.
  // 紧凑折叠摘要使用的稳定主要仓库键。
  primaryRepositoryKey: string | null;
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

