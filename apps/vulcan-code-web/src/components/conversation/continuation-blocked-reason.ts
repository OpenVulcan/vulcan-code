import type { TranslationKey } from "@/i18n-messages";

// continuationBlockedReasonKeys maps authoritative backend reasons to stable localized keys.
// continuationBlockedReasonKeys 将后端权威原因映射为稳定的本地化键。
const continuationBlockedReasonKeys: Readonly<Record<string, TranslationKey>> = {
  tool_side_effect_in_flight: "conversation.continuation.blocked.toolSideEffect",
  provider_dispatch_outcome_unknown: "conversation.continuation.blocked.providerDispatchUnknown",
  semantic_output_observed: "conversation.continuation.blocked.semanticOutput",
  retry_snapshot_transition_failed: "conversation.continuation.blocked.retrySnapshotTransitionFailed",
  pending_action_executor_unavailable: "conversation.continuation.blocked.pendingActionExecutor",
  non_retryable: "conversation.continuation.blocked.nonRetryable",
  ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING: "conversation.continuation.blocked.retrySnapshotMissing",
  ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_VERSION_UNSUPPORTED: "conversation.continuation.blocked.retrySnapshotVersionUnsupported",
  ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE: "conversation.continuation.blocked.retrySnapshotUnsafe",
  ERR_CONVERSATION_CONTINUE_RETRY_FOLLOWUP_INVALID: "conversation.continuation.blocked.requestInvalid",
  ERR_CONVERSATION_CONTINUE_FOLLOWUP_ALREADY_RETAINED: "conversation.continuation.blocked.claimInFlight",
  ERR_CONVERSATION_CONTINUE_FOLLOWUP_EMPTY: "conversation.continuation.blocked.requestInvalid",
  ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH: "conversation.continuation.blocked.routeChanged",
  ERR_CONVERSATION_CONTINUE_SESSION_SETTINGS_NOT_ALLOWED: "conversation.continuation.blocked.requestInvalid",
  ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE: "conversation.continuation.blocked.retrySnapshotBodyUnreadable",
  ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_OWNER_MISMATCH: "conversation.continuation.blocked.retrySnapshotOwnerMismatch",
  ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN: "conversation.continuation.blocked.retrySnapshotNotFrozen",
  ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED: "conversation.continuation.blocked.routeChanged",
  ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING: "conversation.continuation.blocked.routeMissing",
  ERR_CONVERSATION_CONTINUE_RETRY_PROVIDER_MISSING: "conversation.continuation.blocked.routeMissing",
  ERR_CONVERSATION_CONTINUE_RETRY_CONNECTION_MISSING: "conversation.continuation.blocked.routeMissing",
  ERR_CONVERSATION_CONTINUE_RETRY_MODEL_MISSING: "conversation.continuation.blocked.routeMissing",
  ERR_CONVERSATION_CONTINUE_RETRY_NEXT_PROVIDER_BOUNDARY_MISSING: "conversation.continuation.blocked.nextProviderBoundaryMissing",
  ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING: "conversation.continuation.blocked.toolBoundaryMissing",
  ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH: "conversation.continuation.blocked.toolBindingMismatch",
  ERR_CONVERSATION_CONTINUE_RETRY_TOOL_RESULT_MISMATCH: "conversation.continuation.blocked.toolResultMismatch",
  ERR_CONVERSATION_CONTINUE_RETRY_FINAL_ASSISTANT_BOUNDARY_MISSING: "conversation.continuation.blocked.finalAssistantBoundaryMissing",
  ERR_CONVERSATION_CONTINUE_RETRY_CONTEXT_GENERATION_CHANGED: "conversation.continuation.blocked.contextGenerationChanged",
  ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED: "conversation.continuation.blocked.canonicalStateChanged",
  ERR_CONVERSATION_CONTINUE_RETRY_LOGICAL_TURN_CHANGED: "conversation.continuation.blocked.logicalTurnChanged",
  ERR_CONVERSATION_CONTINUE_CLAIM_IN_FLIGHT: "conversation.continuation.blocked.claimInFlight",
  ERR_CONVERSATION_CONTINUE_ADMISSION_IN_FLIGHT: "conversation.continuation.blocked.admissionInFlight",
  ERR_CONVERSATION_CONTINUE_TERMINAL_PERSISTENCE_IN_FLIGHT: "conversation.continuation.blocked.terminalPersistenceInFlight",
  ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE: "conversation.continuation.blocked.sessionActive",
  ERR_CONVERSATION_CONTINUE_NOT_PROBLEM_TURN: "conversation.continuation.blocked.notProblemTurn",
  ERR_CONVERSATION_CONTINUE_SESSION_BUSY: "conversation.continuation.blocked.sessionBusy",
  ERR_CONVERSATION_CONTINUE_STALE: "conversation.continuation.blocked.stale",
  ERR_CONVERSATION_CONTINUE_CALL_IN_FLIGHT: "conversation.continuation.blocked.callInFlight",
  ERR_CONVERSATION_CONTINUE_PROVIDER_CALL_STILL_ACTIVE: "conversation.continuation.blocked.callInFlight",
  ERR_CONVERSATION_CONTINUE_TERMINAL_TAIL_MISSING: "conversation.continuation.blocked.tailMissing",
  ERR_CONVERSATION_CONTINUE_TERMINAL_TAIL_INVALID: "conversation.continuation.blocked.tailInvalid",
  ERR_CONVERSATION_CONTINUE_RETRY_ACTION_UNSUPPORTED: "conversation.continuation.blocked.actionUnsupported",
  ERR_CONVERSATION_CONTINUE_ATTACHMENTS_NOT_ALLOWED: "conversation.continuation.blocked.requestInvalid",
  ERR_CONVERSATION_CONTINUE_CLIENT_ID_EMPTY: "conversation.continuation.blocked.requestInvalid",
  ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION: "conversation.continuation.blocked.conversationUnavailable",
  ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING: "conversation.continuation.blocked.runtimeUnavailable",
  ERR_CONVERSATION_CONTINUE_EXECUTION_TASK_FAILED: "conversation.continuation.blocked.runtimeUnavailable",
  ERR_CONVERSATION_CONTINUE_LEDGER_REVISION_EXHAUSTED: "conversation.continuation.blocked.ledgerExhausted",
  ERR_CONVERSATION_CONTINUE_TERMINAL_TIMESTAMP_INVALID: "conversation.continuation.blocked.timestampInvalid",
  ERR_CONVERSATION_CONTINUE_TIMESTAMP_OUT_OF_RANGE: "conversation.continuation.blocked.timestampInvalid",
};

// conversationContinuationBlockedReasonKey selects the localized key for one backend reason.
// conversationContinuationBlockedReasonKey 为单个后端原因选择本地化键。
export function conversationContinuationBlockedReasonKey(
  reason: string | null | undefined,
): TranslationKey {
  if (!reason) {
    return "conversation.continuation.blocked.default";
  }
  return continuationBlockedReasonKeys[reason] ?? "conversation.continuation.blocked.unknown";
}

// conversationContinuationBlockedReasonText renders a concrete localized safety explanation.
// conversationContinuationBlockedReasonText 渲染具体的本地化安全阻断说明。
export function conversationContinuationBlockedReasonText(
  reason: string | null | undefined,
  translate: (key: TranslationKey) => string,
): string {
  return translate(conversationContinuationBlockedReasonKey(reason));
}

// conversationContinuationBlockedReasonTextFromError localizes known backend codes embedded in an error.
// conversationContinuationBlockedReasonTextFromError 将错误中已知的后端编码本地化。
export function conversationContinuationBlockedReasonTextFromError(
  errorMessage: string,
  translate: (key: TranslationKey) => string,
): string | null {
  const reason = Object.keys(continuationBlockedReasonKeys).find((candidate) => errorMessage.includes(candidate));
  return reason ? conversationContinuationBlockedReasonText(reason, translate) : null;
}
