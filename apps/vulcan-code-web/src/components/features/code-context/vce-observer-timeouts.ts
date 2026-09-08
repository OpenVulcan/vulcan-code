// VCE_OBSERVER_READ_DEADLINE_MS bounds one read-only observer request.
// VCE_OBSERVER_READ_DEADLINE_MS 限制一次只读观察请求的最长等待时间。
export const VCE_OBSERVER_READ_DEADLINE_MS = 8_000;

// VCE_OBSERVER_READ_TIMEOUT_MESSAGE gives the user a retryable diagnostic after a deadline.
// VCE_OBSERVER_READ_TIMEOUT_MESSAGE 在达到期限后给出可重试的诊断信息。
export const VCE_OBSERVER_READ_TIMEOUT_MESSAGE = "语义索引状态读取超时，请稍后重试。";

// VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS bounds one optional management overview read.
// VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS 限制一次可选管理概览读取的最长等待时间。
export const VCE_PROJECT_OVERVIEW_READ_DEADLINE_MS = 8_000;

// VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE gives the management panel a retryable diagnostic.
// VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE 为管理面板提供可重试诊断。
export const VCE_PROJECT_OVERVIEW_READ_TIMEOUT_MESSAGE = "项目概览读取超时，请稍后重试。";
