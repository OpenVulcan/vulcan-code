import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";

// materializeAcknowledgedDraftSubmission binds an accepted first prompt to its durable session without hiding it.
// materializeAcknowledgedDraftSubmission 将已接纳的首条提示词绑定到持久会话，同时保持其可见。
// Parameters: current is the latest draft; submittedDraftId and clientSubmissionId identify the accepted action; conversationId is the service identity.
// 参数：current 是最新草稿；submittedDraftId 和 clientSubmissionId 标识已接纳动作；conversationId 是服务端身份。
// Returns: the untouched current draft for unrelated actions or the durable-id draft retained until canonical correlation.
// 返回值：无关动作返回原草稿，否则返回保留到规范关联完成的持久标识草稿。
export function materializeAcknowledgedDraftSubmission(
  current: DraftConversationSelection | null,
  submittedDraftId: string,
  clientSubmissionId: string,
  conversationId: string,
): DraftConversationSelection | null {
  if (
    current?.id !== submittedDraftId
    || current.pendingSubmission?.clientSubmissionId !== clientSubmissionId
  ) {
    return current;
  }
  return {
    ...current,
    id: conversationId,
    pendingSubmission: {
      ...current.pendingSubmission,
      stage: "preparing",
    },
  };
}

// retireMaterializedDraftSubmission clears only an optimistic prompt replaced by the exact canonical user input.
// retireMaterializedDraftSubmission 仅清理由精确规范用户输入替换的乐观提示词。
// Parameters: current is the latest draft; conversationId and clientSubmissionId identify the visible canonical replacement.
// 参数：current 是最新草稿；conversationId 和 clientSubmissionId 标识可见的规范替代记录。
// Returns: null for the matching transition draft or the untouched current draft otherwise.
// 返回值：匹配的过渡草稿返回 null，其余情况返回原草稿。
export function retireMaterializedDraftSubmission(
  current: DraftConversationSelection | null,
  conversationId: string,
  clientSubmissionId: string,
): DraftConversationSelection | null {
  return current?.id === conversationId && current.pendingSubmission?.clientSubmissionId === clientSubmissionId ? null : current;
}
