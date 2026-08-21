import type { ConversationSummary, WorkbenchSnapshot } from "@/types";

// ContinuationClaimExpectation identifies the exact eligible boundary accepted by one Continue ACK.
// ContinuationClaimExpectation 标识单个 Continue ACK 接受的精确可领取边界。
export interface ContinuationClaimExpectation {
  // Stable conversation identifier owning the claim.
  // 持有本次领取的稳定会话标识。
  conversationId: string;
  // Frozen execution generation submitted by the client.
  // 客户端提交的冻结执行代际。
  expectedGeneration: number;
  // Provider ledger revision submitted by the client.
  // 客户端提交的 Provider 账本修订号。
  expectedLedgerRevision: number;
  // Workbench revision durably committed before the ACK returned.
  // ACK 返回前已经持久提交的工作台修订号。
  acceptedRevision: number;
}

// conversationStillExposesClaim returns whether one summary still publishes the exact pre-ACK eligible token.
// conversationStillExposesClaim 判断摘要是否仍在发布 ACK 前的同一精确可领取令牌。
//
// Parameters: conversation is the target summary and expectation is the acknowledged claim identity.
// 参数：conversation 是目标摘要，expectation 是已确认的领取身份。
// Returns: true only while the old eligible token remains visible.
// 返回值：仅当旧的可领取令牌仍然可见时返回 true。
function conversationStillExposesClaim(
  conversation: ConversationSummary,
  expectation: ContinuationClaimExpectation,
): boolean {
  // continuation is the backend-owned problem-turn recovery admission projection.
  // continuation 是后端拥有的问题回合恢复准入投影。
  const continuation = conversation.continuation;
  return continuation?.eligible === true
    && continuation.generation === expectation.expectedGeneration
    && continuation.expectedLedgerRevision === expectation.expectedLedgerRevision;
}

// continuationClaimSnapshotHasConverged verifies that a post-ACK snapshot no longer exposes the consumed token.
// continuationClaimSnapshotHasConverged 验证 ACK 后快照不再暴露已经消费的领取令牌。
//
// Parameters: snapshot is an authoritative service snapshot and expectation is the acknowledged claim identity.
// 参数：snapshot 是权威服务快照，expectation 是已确认的领取身份。
// Returns: true only when the snapshot covers the ACK revision, contains the conversation, and has left the old token.
// 返回值：仅当快照覆盖 ACK 修订、包含目标会话且已经离开旧令牌时返回 true。
export function continuationClaimSnapshotHasConverged(
  snapshot: WorkbenchSnapshot,
  expectation: ContinuationClaimExpectation,
): boolean {
  // snapshotRevision proves whether the full projection was built after the durable ACK boundary.
  // snapshotRevision 证明全量投影是否在持久 ACK 边界之后构建。
  const snapshotRevision = snapshot.revision ?? 0;
  if (snapshotRevision < expectation.acceptedRevision) {
    return false;
  }
  // conversation is the exact target projected by the authoritative snapshot.
  // conversation 是权威快照投影出的精确目标会话。
  const conversation = snapshot.conversations.find(
    (candidate) => candidate.id === expectation.conversationId,
  );
  return conversation !== undefined && !conversationStillExposesClaim(conversation, expectation);
}

// reconcileContinuationClaimSnapshot repairs only the consumed continuation projection when its broadcast delta was lost.
// reconcileContinuationClaimSnapshot 仅在领取广播增量丢失时修复已消费令牌对应的 Continue 投影。
//
// Parameters: current is the local projection, incoming is the verified post-ACK snapshot, and expectation identifies the old token.
// 参数：current 是本地投影，incoming 是已验证的 ACK 后快照，expectation 标识旧令牌。
// Returns: current unchanged after prior convergence, or a target-only continuation replacement.
// 返回值：已经收敛时原样返回 current，否则仅替换目标会话的 Continue 投影。
export function reconcileContinuationClaimSnapshot(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  expectation: ContinuationClaimExpectation,
): WorkbenchSnapshot {
  // Target-only repair may borrow one fact only after the local full projection already covers the snapshot revision.
  // 目标级修复只有在本地完整投影已覆盖该快照修订后，才允许借用其中一个事实。
  if ((current.revision ?? 0) < (incoming.revision ?? 0)) {
    return current;
  }
  // currentConversationIndex locates the exact local summary without changing unrelated conversations.
  // currentConversationIndex 定位精确本地摘要，避免修改无关会话。
  const currentConversationIndex = current.conversations.findIndex(
    (candidate) => candidate.id === expectation.conversationId,
  );
  if (currentConversationIndex < 0) {
    return current;
  }
  // currentConversation is replaced only while it still owns the exact consumed token.
  // currentConversation 仅在仍持有已经消费的精确令牌时才会被替换。
  const currentConversation = current.conversations[currentConversationIndex];
  if (!conversationStillExposesClaim(currentConversation, expectation)) {
    return current;
  }
  if (!continuationClaimSnapshotHasConverged(incoming, expectation)) {
    return current;
  }
  // incomingConversation is guaranteed to exist by the convergence validation above.
  // incomingConversation 已由上方收敛校验保证存在。
  const incomingConversation = incoming.conversations.find(
    (candidate) => candidate.id === expectation.conversationId,
  );
  if (!incomingConversation) {
    return current;
  }
  // reconciledConversation preserves newer local title, phase, attention, and unread fields while consuming only the stale token.
  // reconciledConversation 保留较新的本地标题、阶段、注意状态与未读字段，只消费过期令牌。
  const reconciledConversation: ConversationSummary = {
    ...currentConversation,
    continuation: incomingConversation.continuation ?? null,
  };
  // conversations preserves every unrelated local summary and replaces only the acknowledged target projection.
  // conversations 保留所有无关本地摘要，并且只替换已确认的目标投影。
  const conversations = [...current.conversations];
  conversations[currentConversationIndex] = reconciledConversation;
  return {
    ...current,
    conversations,
  };
}
