import type { TranslationKey } from "@/i18n-messages";

/** Map stable Core view errors to shared labels; unknown failures disclose no raw diagnostics.
 * 将稳定 Core 查看错误映射到共享文案；未知失败不披露原始诊断。 */
export function htmlAnswerErrorLabel(code: string): TranslationKey {
  switch (code) {
    case "expired": return "htmlAnswers.expired";
    case "stale_source": return "htmlAnswers.stale";
    case "invalid_request": return "htmlAnswers.invalid";
    case "capacity": case "rate_limited": return "htmlAnswers.busy";
    default: return "htmlAnswers.unavailable";
  }
}
