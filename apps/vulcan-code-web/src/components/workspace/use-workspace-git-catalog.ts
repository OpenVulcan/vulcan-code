import React from "react";
import type { DiffReviewSelection, WorkspaceGitRepositorySummary } from "@/diff/types";
import { loadWorkspaceGitCatalog } from "@/service/workbench-snapshot-service";

/** Repository discovery state for the review panel.
 * 审核面板的仓库发现状态。 */
export interface WorkspaceGitCatalogState {
  // Repositories available to the right-panel selector.
  // 右侧面板选择器可用的仓库。
  repositories: WorkspaceGitRepositorySummary[];
  // Discovery or monitoring-setting error.
  // 发现或监听设置错误。
  error: string | null;
  // Discovery runs before the first diff request, so the panel shows it as loading too.
  // 发现阶段位于首个 diff 请求之前，因此面板也将其显示为加载中。
  loading: boolean;
}

/** Loads the project's Git repositories and keeps the selection on an enabled one, preferring the primary.
 * 加载项目的 Git 仓库，并使所选仓库保持为已启用仓库，优先主仓库。
 * Reloads when the project, conversation or an explicit review focus request changes. */
export function useWorkspaceGitCatalog(
  workspacePath: string | null,
  projectId: string | null,
  conversationId: string | null,
  focusSequence: number,
  setSelection: React.Dispatch<React.SetStateAction<DiffReviewSelection>>,
): WorkspaceGitCatalogState {
  const [state, setState] = React.useState<WorkspaceGitCatalogState>({ repositories: [], error: null, loading: false });
  React.useEffect(() => {
    let cancelled = false;
    if (!workspacePath) {
      setState({ repositories: [], error: null, loading: false });
      return undefined;
    }
    setState((current) => ({ ...current, loading: true }));
    void loadWorkspaceGitCatalog(workspacePath, projectId, conversationId)
      .then((catalog) => {
        if (cancelled) return;
        setState({ repositories: catalog.repositories, error: null, loading: false });
        setSelection((current) => {
          const enabledRepositories = catalog.repositories.filter((repository) => repository.monitoringEnabled);
          if (current.repositoryKey && enabledRepositories.some((repository) => repository.repositoryKey === current.repositoryKey)) {
            return current;
          }
          const primaryRepository = enabledRepositories.find((repository) => repository.isPrimary) ?? enabledRepositories[0];
          return { ...current, repositoryKey: primaryRepository?.repositoryKey ?? null };
        });
      })
      .catch((error) => {
        if (!cancelled) setState({ repositories: [], error: error instanceof Error ? error.message : String(error), loading: false });
      });
    return () => {
      cancelled = true;
    };
  }, [conversationId, focusSequence, projectId, setSelection, workspacePath]);
  return state;
}
