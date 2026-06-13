import type {
  ConversationSummary,
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types";

// projectForConversation returns the project that owns one conversation.
// projectForConversation 返回拥有某个会话的项目。
export function projectForConversation(
  // Workbench snapshot containing project and conversation summaries.
  // 包含项目与会话摘要的工作台快照。
  workbench: WorkbenchSnapshot,
  // Conversation summary whose owning project should be resolved.
  // 需要解析所属项目的会话摘要。
  conversation: ConversationSummary,
): ProjectSummary | null {
  // fallbackProject preserves the previous UI behavior when older data lacks project ids.
  // fallbackProject 在旧数据缺少项目标识时保留此前 UI 行为。
  const fallbackProject = workbench.projects[0] ?? null;
  if (!conversation.projectId) {
    return fallbackProject;
  }
  return workbench.projects.find((project) => project.id === conversation.projectId) ?? fallbackProject;
}
