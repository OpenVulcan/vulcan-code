import { type TranslationKey } from "@/i18n"

// VCE_OBSERVER_READ_DEADLINE_MS bounds one read-only observer request.
// VCE_OBSERVER_READ_DEADLINE_MS 限制一次只读观察请求的最长等待时间。
export const VCE_OBSERVER_READ_DEADLINE_MS = 8_000;

// VCE_OBSERVER_READ_TIMEOUT_MESSAGE_KEY names the retryable diagnostic shown after a deadline.
// VCE_OBSERVER_READ_TIMEOUT_MESSAGE_KEY 指向达到期限后显示的可重试诊断文案。
export const VCE_OBSERVER_READ_TIMEOUT_MESSAGE_KEY: TranslationKey = "codeContext.observer.readTimeout";

// VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS bounds one optional management overview read.
// VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS 限制一次可选管理概览读取的最长等待时间。
export const VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS = 8_000;

// VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY names the management panel's retryable diagnostic.
// VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY 指向管理面板的可重试诊断文案。
export const VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE_KEY: TranslationKey = "codeContext.observer.overviewReadTimeout";
