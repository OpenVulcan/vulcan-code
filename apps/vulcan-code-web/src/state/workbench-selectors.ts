import type {
  ConversationSummary,
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types";

// projectForConversation returns the project that explicitly owns one conversation.
// projectForConversation 返回明确拥有某个会话的项目。
export function projectForConversation(
  // Workbench snapshot containing project and conversation summaries.
  // 包含项目与会话摘要的工作台快照。
  workbench: WorkbenchSnapshot,
  // Conversation summary whose owning project should be resolved.
  // 需要解析所属项目的会话摘要。
  conversation: ConversationSummary,
): ProjectSummary | null {
  if (!conversation.projectId) {
    return null;
  }
  return workbench.projects.find((project) => project.id === conversation.projectId) ?? null;
}
