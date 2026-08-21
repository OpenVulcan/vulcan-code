import { describe, expect, it } from "vitest";
import type { TranslationKey } from "@/i18n-messages";
import {
  conversationContinuationBlockedReasonKey,
  conversationContinuationBlockedReasonText,
  conversationContinuationBlockedReasonTextFromError,
} from "./continuation-blocked-reason";

// continuationReasonTestTranslator makes localized-key assertions deterministic.
// continuationReasonTestTranslator 让本地化键断言保持确定性。
const continuationReasonTestTranslator = (key: TranslationKey): string => `translated:${key}`;

describe("conversation continuation blocked reason", () => {
  it("maps side effects to a concrete localized key", () => {
    expect(conversationContinuationBlockedReasonKey("tool_side_effect_in_flight")).toBe(
      "conversation.continuation.blocked.toolSideEffect",
    );
  });

  it("maps provider uncertainty and resolver errors", () => {
    expect(conversationContinuationBlockedReasonKey("provider_dispatch_outcome_unknown")).toBe(
      "conversation.continuation.blocked.providerDispatchUnknown",
    );
    expect(conversationContinuationBlockedReasonKey("retry_snapshot_transition_failed")).toBe(
      "conversation.continuation.blocked.retrySnapshotTransitionFailed",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING")).toBe(
      "conversation.continuation.blocked.retrySnapshotMissing",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_VERSION_UNSUPPORTED")).toBe(
      "conversation.continuation.blocked.retrySnapshotVersionUnsupported",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED")).toBe(
      "conversation.continuation.blocked.routeChanged",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING")).toBe(
      "conversation.continuation.blocked.routeMissing",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE")).toBe(
      "conversation.continuation.blocked.retrySnapshotBodyUnreadable",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED")).toBe(
      "conversation.continuation.blocked.canonicalStateChanged",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_RETRY_LOGICAL_TURN_CHANGED")).toBe(
      "conversation.continuation.blocked.logicalTurnChanged",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_CLAIM_IN_FLIGHT")).toBe(
      "conversation.continuation.blocked.claimInFlight",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_ADMISSION_IN_FLIGHT")).toBe(
      "conversation.continuation.blocked.admissionInFlight",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_TERMINAL_PERSISTENCE_IN_FLIGHT")).toBe(
      "conversation.continuation.blocked.terminalPersistenceInFlight",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_ATTACHMENTS_NOT_ALLOWED")).toBe(
      "conversation.continuation.blocked.requestInvalid",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION")).toBe(
      "conversation.continuation.blocked.conversationUnavailable",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING")).toBe(
      "conversation.continuation.blocked.runtimeUnavailable",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_LEDGER_REVISION_EXHAUSTED")).toBe(
      "conversation.continuation.blocked.ledgerExhausted",
    );
    expect(conversationContinuationBlockedReasonKey("ERR_CONVERSATION_CONTINUE_TIMESTAMP_OUT_OF_RANGE")).toBe(
      "conversation.continuation.blocked.timestampInvalid",
    );
  });

  it("localizes every stable backend Continue error code", () => {
    // backendContinueCodes mirrors the complete stable error set emitted by the current service flow.
    // backendContinueCodes 镜像当前服务流程发出的完整稳定错误集合。
    const backendContinueCodes = [
      "ERR_CONVERSATION_CONTINUE_ATTACHMENTS_NOT_ALLOWED",
      "ERR_CONVERSATION_CONTINUE_ADMISSION_IN_FLIGHT",
      "ERR_CONVERSATION_CONTINUE_CALL_IN_FLIGHT",
      "ERR_CONVERSATION_CONTINUE_CLAIM_IN_FLIGHT",
      "ERR_CONVERSATION_CONTINUE_CLIENT_ID_EMPTY",
      "ERR_CONVERSATION_CONTINUE_EXECUTION_TASK_FAILED",
      "ERR_CONVERSATION_CONTINUE_FOLLOWUP_ALREADY_RETAINED",
      "ERR_CONVERSATION_CONTINUE_FOLLOWUP_EMPTY",
      "ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION",
      "ERR_CONVERSATION_CONTINUE_LEDGER_REVISION_EXHAUSTED",
      "ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH",
      "ERR_CONVERSATION_CONTINUE_NOT_PROBLEM_TURN",
      "ERR_CONVERSATION_CONTINUE_PROVIDER_CALL_STILL_ACTIVE",
      "ERR_CONVERSATION_CONTINUE_RETRY_ACTION_UNSUPPORTED",
      "ERR_CONVERSATION_CONTINUE_RETRY_CANONICAL_STATE_CHANGED",
      "ERR_CONVERSATION_CONTINUE_RETRY_CONNECTION_MISSING",
      "ERR_CONVERSATION_CONTINUE_RETRY_CONTEXT_GENERATION_CHANGED",
      "ERR_CONVERSATION_CONTINUE_RETRY_FINAL_ASSISTANT_BOUNDARY_MISSING",
      "ERR_CONVERSATION_CONTINUE_RETRY_FOLLOWUP_INVALID",
      "ERR_CONVERSATION_CONTINUE_RETRY_LOGICAL_TURN_CHANGED",
      "ERR_CONVERSATION_CONTINUE_RETRY_MODEL_MISSING",
      "ERR_CONVERSATION_CONTINUE_RETRY_NEXT_PROVIDER_BOUNDARY_MISSING",
      "ERR_CONVERSATION_CONTINUE_RETRY_PROVIDER_MISSING",
      "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_CHANGED",
      "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
      "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_BODY_UNREADABLE",
      "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_MISSING",
      "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_NOT_FROZEN",
      "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_OWNER_MISMATCH",
      "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_UNSAFE",
      "ERR_CONVERSATION_CONTINUE_RETRY_SNAPSHOT_VERSION_UNSUPPORTED",
      "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH",
      "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING",
      "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_RESULT_MISMATCH",
      "ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING",
      "ERR_CONVERSATION_CONTINUE_SESSION_ACTIVE",
      "ERR_CONVERSATION_CONTINUE_SESSION_BUSY",
      "ERR_CONVERSATION_CONTINUE_SESSION_SETTINGS_NOT_ALLOWED",
      "ERR_CONVERSATION_CONTINUE_STALE",
      "ERR_CONVERSATION_CONTINUE_TERMINAL_TAIL_INVALID",
      "ERR_CONVERSATION_CONTINUE_TERMINAL_TAIL_MISSING",
      "ERR_CONVERSATION_CONTINUE_TERMINAL_PERSISTENCE_IN_FLIGHT",
      "ERR_CONVERSATION_CONTINUE_TERMINAL_TIMESTAMP_INVALID",
      "ERR_CONVERSATION_CONTINUE_TIMESTAMP_OUT_OF_RANGE",
    ] as const;

    for (const code of backendContinueCodes) {
      expect(conversationContinuationBlockedReasonKey(code)).not.toBe(
        "conversation.continuation.blocked.unknown",
      );
    }
  });

  it("uses a localized generic explanation for unknown reasons", () => {
    expect(conversationContinuationBlockedReasonText("future_internal_reason", continuationReasonTestTranslator)).toBe(
      "translated:conversation.continuation.blocked.unknown",
    );
    expect(conversationContinuationBlockedReasonText(null, continuationReasonTestTranslator)).toBe(
      "translated:conversation.continuation.blocked.default",
    );
  });

  it("localizes known codes embedded in claim errors", () => {
    expect(conversationContinuationBlockedReasonTextFromError(
      "request failed: tool_side_effect_in_flight",
      continuationReasonTestTranslator,
    )).toBe("translated:conversation.continuation.blocked.toolSideEffect");
    expect(conversationContinuationBlockedReasonTextFromError(
      "ERR_CONVERSATION_CONTINUE_ACK_MISMATCH",
      continuationReasonTestTranslator,
    )).toBeNull();
  });
});
