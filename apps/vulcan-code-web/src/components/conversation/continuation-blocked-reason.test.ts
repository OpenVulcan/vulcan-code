import { describe, expect, it } from "vitest";
import type { TranslationKey } from "@/i18n-messages";
import {
  conversationContinuationBlockedReasonKey,
  conversationContinuationBlockedReasonText,
  conversationContinuationBlockedReasonTextFromError,
} from "./continuation-blocked-reason";

// Produces deterministic localized values for continuation-reason assertions.
// 为 Continue 原因断言生成确定性的本地化值。
const continuationReasonTestTranslator = (key: TranslationKey): string => `translated:${key}`;

describe("conversation continuation blocked reason", () => {
  it("maps every current V1 domain rejection reason", () => {
    const domainReasons = {
      active_api: "conversation.continuation.blocked.activeApi",
      missing_api_started: "conversation.continuation.blocked.missingApiStarted",
      duplicate_api_terminal: "conversation.continuation.blocked.duplicateApiTerminal",
      missing_api_terminal: "conversation.continuation.blocked.missingApiTerminal",
      api_terminal_order: "conversation.continuation.blocked.apiTerminalOrder",
      terminal_boundary_superseded: "conversation.continuation.blocked.terminalBoundarySuperseded",
      terminal_policy_forbidden: "conversation.continuation.blocked.terminalPolicyForbidden",
    } as const;

    for (const [reason, key] of Object.entries(domainReasons)) {
      expect(conversationContinuationBlockedReasonKey(reason)).toBe(key);
    }
  });

  it("maps every stable service Continue error emitted by the current flow", () => {
    const serviceErrors = {
      ERR_CONVERSATION_CONTINUE_INVALID_CONVERSATION: "conversation.continuation.blocked.conversationUnavailable",
      ERR_CONVERSATION_CONTINUE_RUNTIME_STATE_MISSING: "conversation.continuation.blocked.runtimeUnavailable",
      ERR_CONVERSATION_CONTINUE_MODEL_SELECTION_MISMATCH: "conversation.continuation.blocked.routeChanged",
      ERR_CONVERSATION_CONTINUE_CANNOT_CREATE_SESSION: "conversation.continuation.blocked.conversationUnavailable",
      ERR_CONVERSATION_CONTINUE_SESSION_BUSY: "conversation.continuation.blocked.sessionBusy",
      ERR_TERMINAL_SETTLEMENT_REPAIR_REQUIRED:
        "conversation.continuation.blocked.terminalSettlementRepair",
    } as const;

    for (const [error, key] of Object.entries(serviceErrors)) {
      expect(conversationContinuationBlockedReasonKey(error)).toBe(key);
    }
  });

  it("maps invalid client action identities to the exact request error", () => {
    expect(conversationContinuationBlockedReasonKey("invalid Continue client action id")).toBe(
      "conversation.continuation.blocked.requestInvalid",
    );
  });

  it("uses localized generic explanations for absent and unknown reasons", () => {
    expect(conversationContinuationBlockedReasonText("future_internal_reason", continuationReasonTestTranslator)).toBe(
      "translated:conversation.continuation.blocked.unknown",
    );
    expect(conversationContinuationBlockedReasonText(null, continuationReasonTestTranslator)).toBe(
      "translated:conversation.continuation.blocked.default",
    );
  });

  it("localizes current V1 reasons embedded in service errors", () => {
    expect(conversationContinuationBlockedReasonTextFromError(
      "open turn is not eligible to continue: terminal_policy_forbidden",
      continuationReasonTestTranslator,
    )).toBe("translated:conversation.continuation.blocked.terminalPolicyForbidden");
    expect(conversationContinuationBlockedReasonTextFromError(
      "ERR_CONVERSATION_CONTINUE_ACK_MISMATCH",
      continuationReasonTestTranslator,
    )).toBeNull();
  });

  it("localizes the durable terminal settlement rejection carried by admission errors", () => {
    // settlementError keeps the stable code prefix and the exact service diagnostic identity.
    // settlementError 保留稳定编码前缀与精确的服务诊断身份。
    const settlementError =
      "ERR_TERMINAL_SETTLEMENT_REPAIR_REQUIRED: session=8f14e45f-ceea-467a-9f0e-1d2b3c4d5e6f error=failed to persist V1 stream failure";
    expect(conversationContinuationBlockedReasonTextFromError(
      settlementError,
      continuationReasonTestTranslator,
    )).toBe("translated:conversation.continuation.blocked.terminalSettlementRepair");
  });
});
