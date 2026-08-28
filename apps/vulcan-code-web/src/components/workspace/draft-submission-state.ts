import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
// materializeAcknowledgedDraftSubmission keeps the optimistic first prompt visible until the authoritative conversation row arrives.
// materializeAcknowledgedDraftSubmission 在权威会话行到达前保持乐观首条提示词可见。
//
// Current identifies the latest local draft, submittedDraftId and clientSubmissionId identify the exact accepted submission,
// conversationId is the durable service identity, and authoritativeConversationVisible reports whether the ordered row already won the race.
// Current 标识最新本地草稿，submittedDraftId 与 clientSubmissionId 标识精确的已接纳提交，conversationId 是服务端持久身份，authoritativeConversationVisible 表示有序会话行是否已先到达。
//
// Returns the untouched current state for unrelated drafts, null when the authoritative row is already visible,
// or a durable-id transition draft whose optimistic message remains rendered.
// 对无关草稿返回原状态；权威行已可见时返回 null；否则返回仍渲染乐观消息、但已切换到持久标识的过渡草稿。
export function materializeAcknowledgedDraftSubmission(
  current: DraftConversationSelection | null,
  submittedDraftId: string,
  clientSubmissionId: string,
  conversationId: string,
  authoritativeConversationVisible: boolean,
): DraftConversationSelection | null {
  if (
    current?.id !== submittedDraftId
    || current.pendingSubmission?.clientSubmissionId !== clientSubmissionId
  ) {
    return current;
  }
  if (authoritativeConversationVisible) {
    return null;
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

// retireMaterializedDraftSubmission clears only the transition draft replaced by one exact authoritative conversation row.
// retireMaterializedDraftSubmission 仅清理被精确权威会话行替换的过渡草稿。
//
// Current is the latest draft state and conversationId is the materialized service identity.
// Current 是最新草稿状态，conversationId 是已物化的服务端身份。
//
// Returns null only for the matching transition draft; unrelated newer drafts remain unchanged.
// 仅对匹配的过渡草稿返回 null；无关的新草稿保持不变。
export function retireMaterializedDraftSubmission(
  current: DraftConversationSelection | null,
  conversationId: string,
): DraftConversationSelection | null {
  return current?.id === conversationId && current.pendingSubmission ? null : current;
}
