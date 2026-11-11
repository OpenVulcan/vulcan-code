// Selects regular-expression matching in the persisted retry rule.
// 在持久化的重试规则中选择正则表达式匹配。
export const PROVIDER_RETRY_REGEX_PREFIX = "regex:"

// Matches the host's response-header diagnostic across endpoints and timeout values.
// 跨不同端点和超时值匹配宿主响应头诊断。
export const PROVIDER_RESPONSE_HEADER_TIMEOUT_PATTERN = /^Provider request to `[^`\r\n]+` exceeded the registered [0-9]+ ms response-header timeout\.$/
