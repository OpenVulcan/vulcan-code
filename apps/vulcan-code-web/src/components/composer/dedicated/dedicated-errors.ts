import { formatMessage, type TranslationKey } from "@/i18n";

// DEDICATED_ERROR_KEYS maps the stable service error codes onto localized copy.
// DEDICATED_ERROR_KEYS 将稳定服务错误码映射为本地化文案。
const DEDICATED_ERROR_KEYS: ReadonlyArray<[code: string, key: TranslationKey]> = [
  ["ERR_DEDICATED_PROMPT_DRAFT_EMPTY", "composer.dedicated.error.draftEmpty"],
  ["ERR_DEDICATED_PROMPT_DRAFT_INPUT_TOO_LONG", "composer.dedicated.error.draftInputTooLong"],
  ["ERR_DEDICATED_PROMPT_DRAFT_TOO_LONG", "composer.dedicated.error.draftTooLong"],
  ["ERR_DEDICATED_PROMPT_DRAFT_REASONING_INVALID", "composer.dedicated.error.draftReasoning"],
  ["ERR_DEDICATED_PROMPT_DRAFT_FAILED", "composer.dedicated.error.draftFailed"],
  ["ERR_DEDICATED_MEMORY_LOCK_TURN_OPEN", "composer.dedicated.error.lockTurnOpen"],
  ["ERR_DEDICATED_SESSION_REQUIRED", "composer.dedicated.error.lockFailed"],
  ["ERR_DEDICATED_MEMORY_LOCK_FAILED", "composer.dedicated.error.lockFailed"],
];

/** Localizes a dedicated-session service error, keeping the service detail after the code.
 * 本地化专职会话服务错误，并保留错误码之后的服务细节。
 *
 * @param error rejected value of a dedicated-session RPC.
 *              专职会话 RPC 的拒绝值。
 * @param t active translation function.
 *          当前语言翻译函数。
 * @returns localized text; unknown failures keep their original message. */
export function dedicatedErrorMessage(error: unknown, t: (key: TranslationKey) => string): string {
  const message = error instanceof Error ? error.message : String(error);
  for (const [code, key] of DEDICATED_ERROR_KEYS) {
    const index = message.indexOf(code);
    if (index < 0) continue;
    const detail = message.slice(index + code.length).replace(/^:\s*/, "").trim();
    return formatMessage(t(key), { detail });
  }
  return message;
}
