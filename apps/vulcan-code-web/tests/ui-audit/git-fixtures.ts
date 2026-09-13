import type { WorkspaceDiffSnapshotResponse, WorkspaceGitRepositoryCatalog } from "../../src/diff/types";
import type { AuditScenario } from "./fixtures";

// auditRepositoryKeys distinguish duplicate display names, one failing repository and one disabled repository.
// auditRepositoryKeys 区分重名显示、一个读取失败仓库以及一个停用仓库。
const auditRepositoryKeys = [".", "packages/同名模块与较长目录名称", "third_party/unavailable", "third_party/disabled"] as const;

/** Builds a read-only synthetic repository directory for the selected project and scenario.
 * 为指定项目和场景构造只读合成仓库目录。
 * Parameters: projectRoot is the fictional workspace path; selected controls the empty versus populated case.
 * 参数：projectRoot 是虚构工作区路径；selected 控制空目录或有数据场景。
 * Returns the exact production catalog; never discovers or changes a real repository.
 * 返回准确的生产目录契约；绝不发现或修改真实仓库。 */
export function auditGitCatalog(projectRoot: string, selected: AuditScenario): WorkspaceGitRepositoryCatalog {
  return { projectRoot, repositories: selected === "empty" ? [] : auditRepositoryKeys.map((repositoryKey, index) => ({
    repositoryKey, repositoryRoot: repositoryKey === "." ? projectRoot : `${projectRoot}/${repositoryKey}`,
    name: index < 2 ? "同名验收仓库" : `隔离仓库 ${index + 1}`, relativePath: repositoryKey,
    isPrimary: index === 0, monitoringEnabled: index !== 3,
  })) };
}

/** Supplies exact synthetic staged/unstaged patches or an explicit repository-specific failure.
 * 提供准确的合成暂存或未暂存补丁，或明确的仓库专属失败。
 * Parameters: projectRoot and repositoryKey select ownership; mode selects the declared comparison.
 * 参数：projectRoot 和 repositoryKey 指定归属；mode 指定声明的比较方式。
 * Returns a complete synthetic snapshot; unknown repositories and unsupported modes are rejected.
 * 返回完整合成快照；拒绝未知仓库和未支持模式。 */
export function auditGitSnapshot(projectRoot: string, repositoryKey: string, mode: string): WorkspaceDiffSnapshotResponse {
  // repository is resolved from the same catalog authority as the visible selector.
  // repository 从与可见选择器相同的目录权威解析。
  const repository = auditGitCatalog(projectRoot, "long-list").repositories.find(entry => entry.repositoryKey === repositoryKey);
  if (!repository) throw new Error("UI_AUDIT_UNKNOWN_REPOSITORY");
  if (repositoryKey === "third_party/unavailable") throw new Error("隔离验收：该仓库暂时无法读取，可重试；没有连接真实 Git。");
  if (mode !== "unstaged" && mode !== "staged") throw new Error("UI_AUDIT_UNIMPLEMENTED_GIT_COMPARISON");
  // changed distinguishes unstaged synthetic content from a deliberately empty staging area.
  // changed 区分未暂存合成内容与刻意为空的暂存区。
  const changed = mode === "unstaged";
  return { repositoryRoot: repository.repositoryRoot,
    unifiedDiff: changed ? `diff --git a/audit.txt b/audit.txt\n--- a/audit.txt\n+++ b/audit.txt\n@@ -1 +1 @@\n-before\n+${repositoryKey === "." ? "root" : "nested"} synthetic change\n` : "",
    changedFiles: changed ? 1 : 0, additions: changed ? 1 : 0, deletions: changed ? 1 : 0,
    currentBranch: repositoryKey === "." ? "main" : "audit/nested", currentBranchUnpushedCommits: 0,
    notice: "隔离合成 Git 数据；写入操作未实现。", branches: [], recentCommits: [],
  };
}
