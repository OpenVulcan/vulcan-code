// WorkspaceGitChangedEvent describes a backend file-event invalidation for Git review.
// WorkspaceGitChangedEvent 描述后端文件事件触发的 Git 审查失效通知。
export interface WorkspaceGitChangedEvent {
  // Optional persisted project identity owning the changed repositories.
  // 发生变化的仓库所属的可选持久化项目标识。
  projectId?: string | null;
  // Canonical workspace path used to match the active project.
  // 用于匹配当前项目的规范化工作区路径。
  workspacePath: string;
  // Repository keys whose watched paths contain a changed file.
  // 被监听路径包含变更文件的仓库键集合。
  repositoryKeys: string[];
}
