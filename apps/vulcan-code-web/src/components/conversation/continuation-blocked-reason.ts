import type { TranslationKey } from "@/i18n-messages";

// Maps current V1 continuation domain reasons and service errors to stable localized keys.
// 将当前 V1 Continue 领域原因与服务错误映射为稳定的本地化键。
const continuationBlockedReasonKeys: Readonly<Record<string, TranslationKey>> = {
  active_api: "conversation.continuation.blocked.activeApi",
  missing_api_started: "conversation.continuation.blocked.missingApiStarted",
  duplicate_api_terminal: "conversation.continuation.blocked.duplicateApiTerminal",
  missing_api_terminal: "conversation.continuation.blocked.missingApiTerminal",
  api_terminal_order: "conversation.continuation.blocked.apiTerminalOrder",
  terminal_boundary_superseded: "conversation.continuation.blocked.terminalBoundarySuperseded",
  terminal_policy_forbidden: "conversation.continuation.blocked.terminalPolicyForbidden",
  "invalid Continue client action id": "conversation.continuation.blocked.requestInvalid",
  ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION: "conversation.continuation.blocked.conversationUnavailable",
  ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING: "conversation.continuation.blocked.runtimeUnavailable",
  ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH: "conversation.continuation.blocked.routeChanged",
  ERR_CONVERSATION_CONTINUE_CANNOT_CREATE_SESSION: "conversation.continuation.blocked.conversationUnavailable",
  ERR_CONVERSATION_CONTINUE_SESSION_BUSY: "conversation.continuation.blocked.sessionBusy",
};

// Selects the localized key for one current V1 continuation rejection.
// 为一条当前 V1 Continue 拒绝原因选择本地化键。
export function conversationContinuationBlockedReasonKey(
  reason: string | null | undefined,
): TranslationKey {
  if (!reason) {
    return "conversation.continuation.blocked.default";
  }
  return continuationBlockedReasonKeys[reason] ?? "conversation.continuation.blocked.unknown";
}

// Renders one concrete localized V1 continuation rejection.
// 渲染一条具体的本地化 V1 Continue 拒绝说明。
export function conversationContinuationBlockedReasonText(
  reason: string | null | undefined,
  translate: (key: TranslationKey) => string,
): string {
  return translate(conversationContinuationBlockedReasonKey(reason));
}

// Localizes a current V1 continuation reason embedded in a service error.
// 本地化服务错误中携带的当前 V1 Continue 原因。
export function conversationContinuationBlockedReasonTextFromError(
  errorMessage: string,
  translate: (key: TranslationKey) => string,
): string | null {
  const reason = Object.keys(continuationBlockedReasonKeys).find((candidate) => errorMessage.includes(candidate));
  return reason ? conversationContinuationBlockedReasonText(reason, translate) : null;
}
