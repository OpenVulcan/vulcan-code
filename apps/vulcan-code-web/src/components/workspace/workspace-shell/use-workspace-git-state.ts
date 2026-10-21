import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import type { ConversationSessionCommitBusyState } from "@/components/conversation/session-todolist-panel";
import type { ConversationSessionGitBranchActions } from "@/components/conversation/session-todolist-panel/git-branch-controls";
import { createWorkspaceGitRefreshOwnership } from "@/components/workspace/workspace-shell/git-refresh-ownership";
import {
  buildWorkspaceProjectGitSummary,
  mergeWorkspaceDiffSnapshotIntoProjectGitState,
  type WorkspaceProjectGitRepositoryState,
  type WorkspaceProjectGitStateMap,
} from "@/components/workspace/workspace-shell/git-summary";
import type { WorkspaceShellProps } from "@/components/workspace/workspace-shell/types";
import type {
  DiffReviewSelection,
  WorkspaceDiffSnapshotResponse,
  WorkspaceReviewFocusRequest,
} from "@/diff/types";
import {
  loadWorkspaceGitBranches,
  loadWorkspaceGitCatalog,
  loadWorkspaceGitHistory,
  setWorkspaceGitMonitoring,
  switchWorkspaceGitBranch,
} from "@/service/workbench-snapshot-service";
import { runGitPullAction, type GitPullActionResponse } from "@/service/conversation-actions-service";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { ConversationSummary, ProjectSummary, WorkbenchSnapshot, WorkspaceGitChangedEvent } from "@/types";

// WorkspaceGitStateOptions supplies the selected chat surface and the service loaders used by Git review state.
// WorkspaceGitStateOptions 提供 Git 审查状态使用的当前会话界面与服务加载器。
export interface WorkspaceGitStateOptions {
  // True only for an existing persisted conversation surface.
  // 仅在当前界面是已存在的持久化会话时为真。
  hasSelectedChatConversation: boolean;
  // Selected durable conversation id that scopes Git IO.
  // 限定 Git 输入输出范围的当前持久会话标识。
  selectedConversationId: string | null;
  // Selected conversation summary whose project owns the Git workspace.
  // 当前会话摘要，其所属项目持有 Git 工作区。
  selectedConversation: ConversationSummary | undefined;
  // Workbench projects used to resolve the selected conversation project.
  // 用于解析当前会话所属项目的工作台项目列表。
  projects: WorkbenchSnapshot["projects"];
  // Configured new-branch prefix; the built-in default applies when absent.
  // 已配置的新分支前缀；缺失时使用内置默认值。
  configuredBranchPrefix: string | undefined;
  // Loads one staged or unstaged diff snapshot for a repository.
  // 为单个仓库加载暂存或未暂存差异快照。
  onLoadWorkspaceDiffSnapshot: WorkspaceShellProps["onLoadWorkspaceDiffSnapshot"];
  // Asks the review panel to reload its catalog after a monitoring switch persists.
  // 在监听开关持久化后要求审核面板重新加载目录。
  setReviewFocusRequest: React.Dispatch<React.SetStateAction<WorkspaceReviewFocusRequest>>;
}

// WorkspaceGitState exposes the selected project's repositories, commit dialog target and Git actions.
// WorkspaceGitState 暴露当前项目的仓库、提交弹窗目标与 Git 动作。
export interface WorkspaceGitState {
  // Project of the selected persisted conversation, when it exists.
  // 当前持久会话所属项目（如果存在）。
  selectedChatProject: ProjectSummary | null;
  // Git workspace whose commit state is currently shown.
  // 当前展示提交状态的 Git 工作区。
  selectedGitCommitWorkspaceKey: string | null;
  // Every independent repository node of the selected project.
  // 当前项目的全部独立仓库节点。
  sessionReviewRepositories: WorkspaceProjectGitRepositoryState[];
  // Actively monitored summary shown on action surfaces.
  // 操作界面展示的正在监听的仓库摘要。
  sessionReviewSummary: WorkspaceProjectGitRepositoryState["summary"];
  // Deterministic repository targeted by commit and push.
  // 提交与推送指向的确定性仓库。
  selectedGitRepositoryForActions: string | null;
  // Changes the repository selected in the review surfaces.
  // 修改审查界面中选中的仓库。
  setSelectedGitRepositoryKey: React.Dispatch<React.SetStateAction<string | null>>;
  // Repository frozen by an open or running commit dialog.
  // 已打开或正在运行的提交弹窗固定的仓库。
  activeGitCommitRepositoryKey: string | null;
  // Commit dialog repository that never falls back after capture.
  // 捕获后绝不回退的提交弹窗仓库。
  activeGitCommitRepositoryForActions: string | null;
  // Summary used by the commit controls.
  // 提交控件使用的仓库摘要。
  activeSessionReviewSummary: WorkspaceProjectGitRepositoryState["summary"];
  // Whether the shared commit dialog is open.
  // 共享提交弹窗是否打开。
  gitCommitDialogOpen: boolean;
  // Running Git action of the selected repository.
  // 当前选中仓库正在执行的 Git 动作。
  gitCommitBusyState: ConversationSessionCommitBusyState;
  // Running Git action of one repository in the selected workspace.
  // 当前工作区内单个仓库正在执行的 Git 动作。
  gitCommitBusyStateForRepository: (repositoryKey: string) => ConversationSessionCommitBusyState;
  // Running Git action of the repository frozen by the dialog.
  // 提交弹窗固定仓库正在执行的 Git 动作。
  activeGitCommitBusyState: ConversationSessionCommitBusyState;
  // Publishes or clears the running Git action of one repository.
  // 发布或清除单个仓库正在执行的 Git 动作。
  updateGitCommitBusyState: (repositoryKey: string | null, state: ConversationSessionCommitBusyState) => void;
  // Selects a review repository and freezes the next commit dialog target.
  // 选择审查仓库并固定下一次提交弹窗目标。
  handleSelectReviewRepository: (repositoryKey: string) => void;
  // Opens or closes the commit dialog while preserving its repository identity.
  // 打开或关闭提交弹窗，同时保留其仓库身份。
  handleGitCommitDialogOpenChange: (open: boolean) => void;
  // Opens the commit dialog against the selected repository.
  // 针对当前选中仓库打开提交弹窗。
  handleOpenGitCommitDialog: () => void;
  // Whether commit or push has a runnable target.
  // 提交或推送是否存在可执行目标。
  sessionGitCommitActionAvailable: boolean;
  // Whether the shared commit triggers are visible.
  // 共享提交入口是否可见。
  sessionGitCommitActionVisible: boolean;
  // Reloads the shared Git summary of one project path, optionally for one repository.
  // 重新加载单个项目路径的共享 Git 摘要，可限定单个仓库。
  refreshProjectGitSummary: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    repositoryKey?: string | null,
  ) => Promise<void>;
  // Refreshes only the repository captured by the commit dialog.
  // 只刷新提交弹窗捕获的目标仓库。
  refreshSelectedGitRepositorySummary: () => Promise<void>;
  // Fast-forwards one repository (null for the selected one) from its upstream while showing busyLabel on its
  // triggers, then refreshes that repository; errors are rethrown to the caller.
  // 从上游快进单个仓库（null 表示当前选中仓库），期间在其入口显示 busyLabel，随后刷新该仓库；错误抛回调用方。
  pullGitRepository: (repositoryKey: string | null, busyLabel: string) => Promise<GitPullActionResponse | null>;
  // Prefix shared by commit and branch-creation recommendations.
  // 提交与新建分支推荐共用的前缀。
  commitBranchPrefix: string;
  // Branch listing, switching and history paging bound to the selected project.
  // 绑定到当前项目的分支列表、切换与历史分页。
  sessionGitBranchActions: ConversationSessionGitBranchActions | null;
  // Persists one repository listener switch and refreshes its node.
  // 持久化单个仓库监听开关并刷新该节点。
  handleToggleReviewMonitoring: (repositoryKey: string, enabled: boolean) => Promise<void>;
  // Merges a panel-loaded snapshot into the shared project Git state.
  // 将面板加载的快照合并进共享项目 Git 状态。
  recordWorkspaceDiffSnapshotForProject: (
    workspacePath: string,
    projectId: string | null,
    selection: DiffReviewSelection,
    response: WorkspaceDiffSnapshotResponse,
  ) => void;
}

// useWorkspaceGitState owns the project-level Git review state shared by the chat tool panel, review panel and commit dialog.
// useWorkspaceGitState 持有会话工具面板、审核面板与提交弹窗共享的项目级 Git 审查状态。
// Parameters: options supplies the selected chat surface, service loaders and the review focus setter.
// 参数：options 提供当前会话界面、服务加载器与审核聚焦设置器。
// Returns: repository selections, commit dialog state and Git actions for the selected project.
// 返回值：当前项目的仓库选择、提交弹窗状态与 Git 动作。
export function useWorkspaceGitState(options: WorkspaceGitStateOptions): WorkspaceGitState {
  const {
    hasSelectedChatConversation,
    selectedConversationId,
    selectedConversation,
    projects,
    configuredBranchPrefix,
    onLoadWorkspaceDiffSnapshot,
    setReviewFocusRequest,
  } = options;
  // workspaceGitStateByProject stores project-level Git state shared by all conversations.
  // workspaceGitStateByProject 保存所有会话共享的项目级 Git 状态。
  const [workspaceGitStateByProject, setWorkspaceGitStateByProject] = React.useState<WorkspaceProjectGitStateMap>({});
  // workspaceGitStateByProjectRef exposes the latest committed project map to long-running refresh callbacks.
  // workspaceGitStateByProjectRef 向长生命周期刷新回调暴露最近提交的项目状态映射。
  const workspaceGitStateByProjectRef = React.useRef<WorkspaceProjectGitStateMap>({});
  workspaceGitStateByProjectRef.current = workspaceGitStateByProject;
  // selectedGitRepositoryKey stores the repository node used by commit and push actions.
  // selectedGitRepositoryKey 保存提交与推送动作当前使用的仓库节点。
  const [selectedGitRepositoryKey, setSelectedGitRepositoryKey] = React.useState<string | null>(null);
  // activeGitCommitRepositoryKey freezes the repository bound to an open or running commit dialog.
  // activeGitCommitRepositoryKey 固定已打开或正在运行的提交弹窗所绑定的仓库。
  const [activeGitCommitRepositoryKey, setActiveGitCommitRepositoryKey] = React.useState<string | null>(null);
  // gitCommitDialogOpen stores the shared commit dialog state across chat and review surfaces.
  // gitCommitDialogOpen 保存聊天区与审核区共享的提交弹窗状态。
  const [gitCommitDialogOpen, setGitCommitDialogOpen] = React.useState(false);
  // gitCommitBusyStateByWorkspace stores running Git actions by workspace path.
  // gitCommitBusyStateByWorkspace 按工作区路径保存正在执行的 Git 动作状态。
  const [gitCommitBusyStateByWorkspace, setGitCommitBusyStateByWorkspace] = React.useState<Record<string, NonNullable<ConversationSessionCommitBusyState>>>({});
  // workspaceGitRefreshOwner restricts success, failure and selection writes to their current project request.
  // workspaceGitRefreshOwner 将成功、失败与选择写入限制到所属项目的当前请求。
  const workspaceGitRefreshOwner = React.useRef(createWorkspaceGitRefreshOwnership());
  // selectedChatProject resolves the selected persisted conversation project for Git review summaries.
  // selectedChatProject 解析当前持久化会话所属项目以加载 Git 审查摘要。
  const selectedChatProject = hasSelectedChatConversation && selectedConversation?.projectId
    ? projects.find((project) => project.id === selectedConversation?.projectId) ?? null
    : null;
  // selectedGitCommitWorkspaceKey identifies the Git workspace whose commit state is currently shown.
  // selectedGitCommitWorkspaceKey 标识当前展示提交状态的 Git 工作区。
  const selectedGitCommitWorkspaceKey = selectedChatProject?.path ?? null;
  // selectedGitWorkspaceRef prevents delayed refreshes from changing another project's visible selection.
  // selectedGitWorkspaceRef 防止迟到刷新改变另一项目的可见仓库选择。
  const selectedGitWorkspaceRef = React.useRef(selectedGitCommitWorkspaceKey);
  selectedGitWorkspaceRef.current = selectedGitCommitWorkspaceKey;
  // selectedWorkspaceGitState stores the selected project's shared Git state entry.
  // selectedWorkspaceGitState 保存当前选中项目的共享 Git 状态条目。
  const selectedWorkspaceGitState = selectedGitCommitWorkspaceKey
    ? workspaceGitStateByProject[selectedGitCommitWorkspaceKey] ?? null
    : null;
  // sessionReviewRepositories exposes every independent repository node to the compact panel.
  // sessionReviewRepositories 向紧凑面板暴露全部独立仓库节点。
  const sessionReviewRepositories = selectedWorkspaceGitState?.repositories ?? [];
  // selectedGitRepositoryState stores the repository currently targeted by commit and push.
  // selectedGitRepositoryState 保存提交与推送当前指向的仓库。
  const enabledReviewRepositories = sessionReviewRepositories.filter((repository) => repository.monitoringEnabled);
  // sessionReviewSummary exposes only an actively monitored primary summary to action surfaces.
  // sessionReviewSummary 只向操作界面暴露正在监听的主要仓库摘要。
  const sessionReviewSummary = enabledReviewRepositories.length > 0
    ? enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedGitRepositoryKey)?.summary
      ?? enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedWorkspaceGitState?.primaryRepositoryKey)?.summary
      ?? enabledReviewRepositories.find((repository) => repository.summary)?.summary
      ?? null
    : null;
  const selectedGitRepositoryState = selectedGitRepositoryKey
    ? enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedGitRepositoryKey)
      ?? enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedWorkspaceGitState?.primaryRepositoryKey)
      ?? enabledReviewRepositories[0]
      ?? null
    : enabledReviewRepositories.find((repository) => repository.repositoryKey === selectedWorkspaceGitState?.primaryRepositoryKey)
      ?? enabledReviewRepositories[0]
      ?? null;
  // selectedGitRepositoryForActions always has a deterministic repository key when a catalog exists.
  // selectedGitRepositoryForActions 在存在仓库目录时始终具有确定性仓库键。
  const selectedGitRepositoryForActions = selectedGitRepositoryState?.repositoryKey ?? null;
  // selectedGitRepositoryForActionsRef lets a selection made in the same click reach the dialog opener before React re-renders.
  // selectedGitRepositoryForActionsRef 让同一次点击中的仓库选择在 React 重渲染前即可被提交弹窗读取。
  const selectedGitRepositoryForActionsRef = React.useRef(selectedGitRepositoryForActions);
  selectedGitRepositoryForActionsRef.current = selectedGitRepositoryForActions;
  // activeGitCommitRepositoryState keeps the dialog target independent from right-panel selection changes.
  // activeGitCommitRepositoryState 让提交弹窗目标独立于右侧面板选择变化。
  const activeGitCommitRepositoryState = activeGitCommitRepositoryKey
    ? enabledReviewRepositories.find((repository) => repository.repositoryKey === activeGitCommitRepositoryKey) ?? null
    : selectedGitRepositoryState;
  // activeGitCommitRepositoryForActions never falls back after a target has been captured.
  // activeGitCommitRepositoryForActions 在捕获目标后绝不回退到其他仓库，避免弹窗或异步动作漂移。
  const activeGitCommitRepositoryForActions = activeGitCommitRepositoryKey !== null
    ? activeGitCommitRepositoryState?.repositoryKey ?? null
    : selectedGitRepositoryForActions;
  // activeSessionReviewSummary is the independent repository summary used by commit controls.
  // activeSessionReviewSummary 是提交控件使用的独立仓库摘要。
  const activeSessionReviewSummary = activeGitCommitRepositoryState
    ? activeGitCommitRepositoryState.summary
    : sessionReviewSummary;
  // gitCommitBusyState stores the selected workspace's shared Git action phase.
  // gitCommitBusyState 保存当前选中工作区的共享 Git 操作阶段。
  const gitCommitBusyState = selectedGitCommitWorkspaceKey && selectedGitRepositoryForActions
    ? gitCommitBusyStateByWorkspace[`${selectedGitCommitWorkspaceKey}\u0000${selectedGitRepositoryForActions}`] ?? null
    : null;
  // updateGitCommitBusyState updates the running Git action state for the captured workspace.
  // updateGitCommitBusyState 更新当前闭包捕获工作区的 Git 动作状态。
  const updateGitCommitBusyState = React.useCallback((repositoryKey: string | null, state: ConversationSessionCommitBusyState) => {
    const workspaceKey = selectedGitCommitWorkspaceKey && repositoryKey
      ? `${selectedGitCommitWorkspaceKey}\u0000${repositoryKey}`
      : null;
    if (!workspaceKey) {
      return;
    }
    setGitCommitBusyStateByWorkspace((current) => {
      if (state) {
        return { ...current, [workspaceKey]: state };
      }
      if (!Object.prototype.hasOwnProperty.call(current, workspaceKey)) {
        return current;
      }
      const next = { ...current };
      delete next[workspaceKey];
      return next;
    });
  }, [selectedGitCommitWorkspaceKey]);
  // gitCommitBusyStateForRepository exposes independent action phases without cross-repository bleed.
  // gitCommitBusyStateForRepository 暴露独立操作阶段，避免仓库之间串状态。
  const gitCommitBusyStateForRepository = React.useCallback((repositoryKey: string) => {
    if (!selectedGitCommitWorkspaceKey) {
      return null;
    }
    return gitCommitBusyStateByWorkspace[
      `${selectedGitCommitWorkspaceKey}\u0000${repositoryKey}`
    ] ?? null;
  }, [gitCommitBusyStateByWorkspace, selectedGitCommitWorkspaceKey]);
  // activeGitCommitBusyState shows the action phase for the repository frozen by the dialog.
  // activeGitCommitBusyState 显示提交弹窗固定仓库对应的操作阶段。
  const activeGitCommitBusyState = activeGitCommitRepositoryForActions
    ? gitCommitBusyStateForRepository(activeGitCommitRepositoryForActions)
    : gitCommitBusyState;

  // handleSelectReviewRepository updates the visible selection and freezes the next commit dialog target.
  // handleSelectReviewRepository 更新可见选择，并固定下一次提交弹窗目标。
  const handleSelectReviewRepository = React.useCallback((repositoryKey: string) => {
    selectedGitRepositoryForActionsRef.current = repositoryKey;
    setSelectedGitRepositoryKey(repositoryKey);
    // Keep the open or running shared action bound to its original repository.
    // 保持已打开或正在运行的共享动作绑定原始仓库。
    if (!gitCommitDialogOpen && !activeGitCommitBusyState) {
      setActiveGitCommitRepositoryKey(repositoryKey);
    }
  }, [activeGitCommitBusyState, gitCommitDialogOpen]);

  // handleGitCommitDialogOpenChange preserves the selected repository identity across dialog close and reopen.
  // handleGitCommitDialogOpenChange 在弹窗关闭与重开之间保留所选仓库身份。
  const handleGitCommitDialogOpenChange = React.useCallback((open: boolean) => {
    const repositoryKey = selectedGitRepositoryForActionsRef.current;
    if (open && !gitCommitDialogOpen && repositoryKey) {
      setActiveGitCommitRepositoryKey(repositoryKey);
    }
    setGitCommitDialogOpen(open);
  }, [gitCommitDialogOpen]);

  // handleOpenGitCommitDialog opens the shared dialog against the currently selected repository.
  // handleOpenGitCommitDialog 针对当前选中仓库打开共享提交弹窗。
  const handleOpenGitCommitDialog = React.useCallback(() => {
    if (!selectedGitRepositoryForActions) {
      return;
    }
    setActiveGitCommitRepositoryKey(selectedGitRepositoryForActions);
    setGitCommitDialogOpen(true);
  }, [selectedGitRepositoryForActions]);

  // sessionGitRepositoryAvailable tracks whether the current chat project resolved as a Git repository.
  // sessionGitRepositoryAvailable 标记当前聊天项目是否已解析为 Git 仓库。
  const sessionGitRepositoryAvailable = Boolean(sessionReviewRepositories.length || sessionReviewSummary || selectedWorkspaceGitState?.loading);
  // sessionGitCommitActionAvailable tracks whether commit or push has a runnable target.
  // sessionGitCommitActionAvailable 标记提交或推送是否存在可执行目标。
  const sessionGitCommitActionAvailable = Boolean(activeSessionReviewSummary && (
    activeSessionReviewSummary.changedFiles > 0
    || activeSessionReviewSummary.unpushedCommits > 0
  ));
  // sessionGitCommitActionVisible keeps shared commit triggers visible for Git repositories or running Git work.
  // sessionGitCommitActionVisible 在 Git 仓库或正在执行 Git 工作时保持共享提交入口可见。
  const sessionGitCommitActionVisible = sessionGitRepositoryAvailable || Boolean(gitCommitBusyState);

  // refreshProjectGitSummary reloads the shared Git summary for one project path.
  // refreshProjectGitSummary 重新加载单个项目路径的共享 Git 摘要。
  // Parameters: workspacePath is the cache owner; projectId/conversationId scope IO; optional repositoryKey limits diff refreshes.
  // 参数：workspacePath 是缓存归属；projectId/conversationId 限定输入输出；可选 repositoryKey 限制差异刷新范围。
  // Returns after the current request publishes a summary/error or loses ownership; it never writes another project's selection.
  // 当前请求发布摘要/错误或失去归属后返回；绝不写入另一项目的选择。
  const refreshProjectGitSummary = React.useCallback(async (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    repositoryKey?: string | null,
  ) => {
    // ticket gates every terminal result, including failed catalog reads and visible selection updates.
    // ticket 对所有终态结果设门，包括目录读取失败及可见选择更新。
    const ticket = workspaceGitRefreshOwner.current.begin(workspacePath);
    const requestId = ticket.requestId;
    setWorkspaceGitStateByProject((current) => {
      const previous = current[workspacePath];
      return {
        ...current,
        [workspacePath]: {
          error: null,
          loading: true,
          projectId,
          refreshRequestId: requestId,
          revision: previous?.revision ?? 0,
          stagedSnapshot: previous?.stagedSnapshot ?? null,
          summary: previous?.summary ?? null,
          unstagedSnapshot: previous?.unstagedSnapshot ?? null,
          workspacePath,
          repositories: previous?.repositories.map((repository) => ({
            ...repository,
            error: repositoryKey && repository.repositoryKey !== repositoryKey ? repository.error : null,
            loading: repository.monitoringEnabled
              && (!repositoryKey || repository.repositoryKey === repositoryKey),
          })) ?? [],
          primaryRepositoryKey: previous?.primaryRepositoryKey ?? null,
        },
      };
    });
    let catalog;
    try {
      catalog = await loadWorkspaceGitCatalog(workspacePath, projectId, conversationId);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setWorkspaceGitStateByProject((current) => workspaceGitRefreshOwner.current.owns(ticket) ? ({
        ...current,
        [workspacePath]: {
          ...(current[workspacePath] ?? {
            projectId,
            revision: 0,
            stagedSnapshot: null,
            summary: null,
            unstagedSnapshot: null,
            workspacePath,
            repositories: [],
            primaryRepositoryKey: null,
          }),
          error: message,
          loading: false,
          projectId,
          refreshRequestId: requestId,
        },
      }) : current);
      return;
    }
    if (!workspaceGitRefreshOwner.current.owns(ticket)) return;
    const previousEntry = workspaceGitStateByProjectRef.current[workspacePath];
    const repositoryStates = await Promise.all(catalog.repositories.map(async (repository) => {
      const previousRepository = previousEntry?.repositories.find((entry) => entry.repositoryKey === repository.repositoryKey);
      if (repositoryKey && repository.repositoryKey !== repositoryKey) {
        return previousRepository ?? {
          error: null,
          isPrimary: repository.isPrimary,
          loading: false,
          monitoringEnabled: repository.monitoringEnabled,
          projectId,
          relativePath: repository.relativePath,
          repositoryKey: repository.repositoryKey,
          repositoryName: repository.name,
          revision: 0,
          stagedSnapshot: null,
          summary: null,
          unstagedSnapshot: null,
          workspacePath,
        } satisfies WorkspaceProjectGitRepositoryState;
      }
      if (!repository.monitoringEnabled) {
        return {
          error: null,
          isPrimary: repository.isPrimary,
          loading: false,
          monitoringEnabled: false,
          projectId,
          relativePath: repository.relativePath,
          repositoryKey: repository.repositoryKey,
          repositoryName: repository.name,
          revision: (previousRepository?.revision ?? 0) + 1,
          // Disabled repositories remain visible for settings but must not expose stale review counts.
          // 已关闭监听的仓库仍保留设置入口，但不能继续展示过期审查统计。
          stagedSnapshot: null,
          summary: null,
          unstagedSnapshot: null,
          workspacePath,
        } satisfies WorkspaceProjectGitRepositoryState;
      }
      const [unstagedResult, stagedResult] = await Promise.allSettled([
        onLoadWorkspaceDiffSnapshot(workspacePath, projectId, conversationId, {
          mode: "unstaged",
          repositoryKey: repository.repositoryKey,
        }),
        onLoadWorkspaceDiffSnapshot(workspacePath, projectId, conversationId, {
          mode: "staged",
          repositoryKey: repository.repositoryKey,
        }),
      ]);
      const rejectedError = [unstagedResult, stagedResult]
        .find((result): result is PromiseRejectedResult => result.status === "rejected")
        ?.reason;
      const unstagedSnapshot = unstagedResult.status === "fulfilled"
        ? unstagedResult.value
        : previousRepository?.unstagedSnapshot ?? null;
      const stagedSnapshot = stagedResult.status === "fulfilled"
        ? stagedResult.value
        : previousRepository?.stagedSnapshot ?? null;
      return {
        error: rejectedError instanceof Error ? rejectedError.message : rejectedError ? String(rejectedError) : null,
        isPrimary: repository.isPrimary,
        loading: false,
        monitoringEnabled: true,
        projectId,
        relativePath: repository.relativePath,
        repositoryKey: repository.repositoryKey,
        repositoryName: repository.name,
        revision: (previousRepository?.revision ?? 0) + 1,
        stagedSnapshot,
        summary: buildWorkspaceProjectGitSummary(unstagedSnapshot, stagedSnapshot),
        unstagedSnapshot,
        workspacePath,
      } satisfies WorkspaceProjectGitRepositoryState;
    }));
    const primaryRepository = repositoryStates.find((repository) => (
      catalog.repositories.find((candidate) => candidate.repositoryKey === repository.repositoryKey)?.isPrimary
    )) ?? repositoryStates[0] ?? null;
    setWorkspaceGitStateByProject((current) => {
      const previous = current[workspacePath];
      if (!workspaceGitRefreshOwner.current.owns(ticket)) {
        return current;
      }
      return {
        ...current,
        [workspacePath]: {
          error: repositoryStates.find((repository) => repository.error)?.error ?? null,
          loading: repositoryStates.some((repository) => repository.loading),
          projectId,
          refreshRequestId: requestId,
          revision: (previous?.revision ?? 0) + 1,
          stagedSnapshot: primaryRepository?.stagedSnapshot ?? null,
          summary: primaryRepository?.summary ?? null,
          unstagedSnapshot: primaryRepository?.unstagedSnapshot ?? null,
          workspacePath,
          repositories: repositoryStates,
          primaryRepositoryKey: primaryRepository?.repositoryKey ?? null,
        },
      };
    });
    setSelectedGitRepositoryKey((current) => (
      !workspaceGitRefreshOwner.current.ownsSelection(ticket, selectedGitWorkspaceRef.current) ? current
        : current && repositoryStates.some((repository) => repository.repositoryKey === current && repository.monitoringEnabled)
        ? current
        : repositoryStates.find((repository) => repository.monitoringEnabled && repository.repositoryKey === primaryRepository?.repositoryKey)?.repositoryKey
          ?? repositoryStates.find((repository) => repository.monitoringEnabled)?.repositoryKey
          ?? null
    ));
  }, [onLoadWorkspaceDiffSnapshot]);

  // refreshSessionReviewSummary reloads Git state for the currently selected project.
  // refreshSessionReviewSummary 重新加载当前选中项目的 Git 状态。
  const refreshSessionReviewSummary = React.useCallback(async () => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path) {
      return;
    }
    await refreshProjectGitSummary(
      selectedChatProject.path,
      selectedChatProject.id ?? null,
      selectedConversationId,
    );
  }, [
    hasSelectedChatConversation,
    selectedConversationId,
    refreshProjectGitSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  // refreshSelectedGitRepositorySummary refreshes only the repository captured by the commit dialog.
  // refreshSelectedGitRepositorySummary 只刷新提交弹窗捕获的目标仓库。
  const refreshSelectedGitRepositorySummary = React.useCallback(async () => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path || !activeGitCommitRepositoryForActions) {
      return;
    }
    await refreshProjectGitSummary(
      selectedChatProject.path,
      selectedChatProject.id ?? null,
      selectedConversationId,
      activeGitCommitRepositoryForActions,
    );
  }, [
    hasSelectedChatConversation,
    selectedConversationId,
    refreshProjectGitSummary,
    activeGitCommitRepositoryForActions,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  // pullGitRepository fast-forwards one repository and refreshes only that node.
  // pullGitRepository 快进单个仓库并只刷新该节点。
  const pullGitRepository = React.useCallback(async (repositoryKey: string | null, busyLabel: string) => {
    const targetRepositoryKey = repositoryKey ?? selectedGitRepositoryForActions;
    if (!hasSelectedChatConversation || !selectedChatProject?.path || !selectedConversationId || !targetRepositoryKey) {
      return null;
    }
    const workspacePath = selectedChatProject.path;
    updateGitCommitBusyState(targetRepositoryKey, { label: busyLabel });
    try {
      const response = await runGitPullAction({
        conversationId: selectedConversationId,
        workspacePath,
        repositoryKey: targetRepositoryKey,
      });
      await refreshProjectGitSummary(workspacePath, selectedChatProject.id ?? null, selectedConversationId, targetRepositoryKey);
      return response;
    } finally {
      updateGitCommitBusyState(targetRepositoryKey, null);
    }
  }, [
    hasSelectedChatConversation,
    refreshProjectGitSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
    selectedConversationId,
    selectedGitRepositoryForActions,
    updateGitCommitBusyState,
  ]);

  // commitBranchPrefix is the configured prefix shared by commit and branch-creation recommendations.
  // commitBranchPrefix 是提交与新建分支推荐共用的已配置前缀。
  const commitBranchPrefix = configuredBranchPrefix ?? "vulcan/";

  // sessionGitBranchActions binds branch listing, switching, and history paging to the selected project.
  // sessionGitBranchActions 将分支列表、切换与历史分页绑定到当前选中项目。
  const sessionGitBranchActions = React.useMemo<ConversationSessionGitBranchActions | null>(() => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path) {
      return null;
    }
    const workspacePath = selectedChatProject.path;
    const projectId = selectedChatProject.id ?? null;
    const conversationId = selectedConversationId;
    return {
      commitBranchPrefix,
      loadBranches: (repositoryKey) => loadWorkspaceGitBranches(workspacePath, projectId, conversationId, repositoryKey),
      loadHistory: (repositoryKey, skip) => loadWorkspaceGitHistory(workspacePath, projectId, conversationId, repositoryKey, skip),
      switchBranch: async (repositoryKey, branchName, create) => {
        await switchWorkspaceGitBranch(workspacePath, projectId, conversationId, repositoryKey, branchName, create);
        await refreshProjectGitSummary(workspacePath, projectId, conversationId, repositoryKey);
      },
    };
  }, [
    commitBranchPrefix,
    hasSelectedChatConversation,
    selectedConversationId,
    refreshProjectGitSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  React.useEffect(() => {
    if (!hasSelectedChatConversation || !selectedChatProject?.path || !hasServiceRuntime()) {
      return undefined;
    }
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    const workspacePath = selectedChatProject.path;
    const projectId = selectedChatProject.id ?? null;
    const conversationId = selectedConversationId;
    const normalizeWorkspacePath = (value: string): string => value.replaceAll("\\", "/").replace(/\/+$/, "").toLocaleLowerCase();
    // pendingRepositoryKeys coalesces bursts of file events so each repository refreshes at most once per drain round.
    // pendingRepositoryKeys 合并突发文件事件，使每个仓库在一轮排空中最多刷新一次。
    const pendingRepositoryKeys = new Set<string>();
    let draining = false;
    // drainPendingRefreshes serializes refreshes and reports one failure per drain instead of one per event.
    // drainPendingRefreshes 串行执行刷新，每轮排空只报告一次失败而不是每个事件都报告。
    const drainPendingRefreshes = async (): Promise<void> => {
      if (draining) {
        return;
      }
      draining = true;
      let firstError: unknown = null;
      try {
        while (!cancelled && pendingRepositoryKeys.size > 0) {
          const repositoryKey = pendingRepositoryKeys.values().next().value as string;
          pendingRepositoryKeys.delete(repositoryKey);
          try {
            await refreshProjectGitSummary(workspacePath, projectId, conversationId, repositoryKey);
          } catch (error: unknown) {
            firstError ??= error;
          }
        }
      } finally {
        draining = false;
      }
      if (firstError !== null && !cancelled) {
        toast.error(firstError instanceof Error ? firstError.message : String(firstError));
      }
    };
    const subscription = serviceClient.subscribe<WorkspaceGitChangedEvent>("workspace.git.changed", (event) => {
      if (cancelled || normalizeWorkspacePath(event.workspacePath) !== normalizeWorkspacePath(workspacePath)) {
        return;
      }
      if (event.projectId && projectId && event.projectId !== projectId) {
        return;
      }
      const repositoryKeys = [...new Set(event.repositoryKeys)].filter((key) => key.trim().length > 0);
      if (repositoryKeys.length === 0) {
        return;
      }
      for (const repositoryKey of repositoryKeys) {
        pendingRepositoryKeys.add(repositoryKey);
      }
      void drainPendingRefreshes();
    });
    void subscription.then((nextUnsubscribe) => {
      if (cancelled) {
        nextUnsubscribe();
      } else {
        unsubscribe = nextUnsubscribe;
      }
    }).catch((error: unknown) => {
      // A failed subscription must remain visible; stale surfaces never report a new connection's failure.
      // 订阅失败必须可见；失效表面不会报告新连接的错误。
      if (!cancelled) toast.error(error instanceof Error ? error.message : String(error));
    });
    return () => {
      cancelled = true;
      unsubscribe?.();
      unsubscribe = null;
      pendingRepositoryKeys.clear();
    };
  }, [
    hasSelectedChatConversation,
    selectedConversationId,
    refreshProjectGitSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
  ]);

  // handleToggleReviewMonitoring persists one repository listener switch and refreshes its own node.
  // handleToggleReviewMonitoring 持久化单个仓库监听开关并刷新该仓库节点。
  // Parameters: repositoryKey identifies the exact node and enabled is the requested persisted state. Returns a rejecting promise on failure.
  // 参数：repositoryKey 标识精确节点，enabled 是请求持久化的状态。失败时返回拒绝的 Promise。
  const handleToggleReviewMonitoring = React.useCallback(async (repositoryKey: string, enabled: boolean) => {
    if (!selectedChatProject?.path || !selectedChatProject.id || !selectedConversationId) {
      return;
    }
    await setWorkspaceGitMonitoring(
      selectedChatProject.path,
      selectedChatProject.id,
      selectedConversationId,
      repositoryKey,
      enabled,
    );
    await refreshProjectGitSummary(
      selectedChatProject.path,
      selectedChatProject.id,
      selectedConversationId,
    );
    // Only the project still displayed may request a panel catalog refresh after persistence.
    // 持久化结束后，仅仍在显示的项目可以请求面板目录刷新。
    if (selectedGitWorkspaceRef.current === selectedChatProject.path) {
      setReviewFocusRequest((current) => ({
        repositoryKey: current.repositoryKey,
        sequence: current.sequence + 1,
      }));
    }
  }, [selectedConversationId, refreshProjectGitSummary, selectedChatProject?.id, selectedChatProject?.path]);

  // recordWorkspaceDiffSnapshotForProject merges a panel-loaded snapshot into the shared project Git state.
  // recordWorkspaceDiffSnapshotForProject 将面板加载的快照合并进共享项目 Git 状态。
  const recordWorkspaceDiffSnapshotForProject = React.useCallback((
    workspacePath: string,
    projectId: string | null,
    selection: DiffReviewSelection,
    response: WorkspaceDiffSnapshotResponse,
  ) => {
    setWorkspaceGitStateByProject((current) => mergeWorkspaceDiffSnapshotIntoProjectGitState(
      current,
      workspacePath,
      projectId,
      selection,
      response,
    ));
  }, []);

  React.useEffect(() => {
    if (!hasSelectedChatConversation || (gitCommitDialogOpen && !activeSessionReviewSummary)) {
      setGitCommitDialogOpen(false);
    }
  }, [activeSessionReviewSummary, gitCommitDialogOpen, hasSelectedChatConversation]);

  React.useEffect(() => {
    let cancelled = false;
    if (
      hasSelectedChatConversation
      && selectedChatProject?.path
      && !selectedWorkspaceGitState
    ) {
      void refreshSessionReviewSummary().catch((error) => {
        if (!cancelled) {
          const message = error instanceof Error ? error.message : String(error);
          setWorkspaceGitStateByProject((current) => ({
            ...current,
            [selectedChatProject.path]: {
              error: message,
              loading: false,
              projectId: selectedChatProject.id ?? null,
              refreshRequestId: current[selectedChatProject.path]?.refreshRequestId ?? 0,
              revision: current[selectedChatProject.path]?.revision ?? 0,
              stagedSnapshot: current[selectedChatProject.path]?.stagedSnapshot ?? null,
              summary: current[selectedChatProject.path]?.summary ?? null,
              unstagedSnapshot: current[selectedChatProject.path]?.unstagedSnapshot ?? null,
              workspacePath: selectedChatProject.path,
              repositories: current[selectedChatProject.path]?.repositories ?? [],
              primaryRepositoryKey: current[selectedChatProject.path]?.primaryRepositoryKey ?? null,
            },
          }));
        }
      });
    }
    return () => {
      cancelled = true;
    };
  }, [
    hasSelectedChatConversation,
    refreshSessionReviewSummary,
    selectedChatProject?.id,
    selectedChatProject?.path,
    selectedWorkspaceGitState,
  ]);

  return {
    selectedChatProject,
    selectedGitCommitWorkspaceKey,
    sessionReviewRepositories,
    sessionReviewSummary,
    selectedGitRepositoryForActions,
    setSelectedGitRepositoryKey,
    activeGitCommitRepositoryKey,
    activeGitCommitRepositoryForActions,
    activeSessionReviewSummary,
    gitCommitDialogOpen,
    gitCommitBusyState,
    gitCommitBusyStateForRepository,
    activeGitCommitBusyState,
    updateGitCommitBusyState,
    handleSelectReviewRepository,
    handleGitCommitDialogOpenChange,
    handleOpenGitCommitDialog,
    sessionGitCommitActionAvailable,
    sessionGitCommitActionVisible,
    refreshProjectGitSummary,
    refreshSelectedGitRepositorySummary,
    pullGitRepository,
    commitBranchPrefix,
    sessionGitBranchActions,
    handleToggleReviewMonitoring,
    recordWorkspaceDiffSnapshotForProject,
  };
}
