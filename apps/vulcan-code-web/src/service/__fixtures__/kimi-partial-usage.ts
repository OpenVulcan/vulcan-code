import type { ConversationEventSummary, StatBucket, UsageSnapshot } from "../conversation-event-window-store";

/** Redacted token-only samples from session 5ffbbeaa-2936-4af1-bad9-192b0ba3586e; no prompts, endpoints or credentials.
 * 来自会话 5ffbbeaa-2936-4af1-bad9-192b0ba3586e 的脱敏 Token 样本；无提示词、端点或凭据。 */
const KIMI_USAGE: readonly UsageSnapshot[] = [
  { input_tokens: 18142, output_tokens: 70, total_tokens: 18212 },
  { input_tokens: 23729, cache_read_tokens: 17920, uncached_input_tokens: 5809, output_tokens: 140, total_tokens: 23869 },
  { input_tokens: 24468, cache_read_tokens: 23552, uncached_input_tokens: 916, output_tokens: 184, total_tokens: 24652 },
  { input_tokens: 26225, cache_read_tokens: 24320, uncached_input_tokens: 1905, output_tokens: 71, total_tokens: 26296 },
  { input_tokens: 26614, cache_read_tokens: 26112, uncached_input_tokens: 502, output_tokens: 187, total_tokens: 26801 },
  { input_tokens: 29223, cache_read_tokens: 26624, uncached_input_tokens: 2599, output_tokens: 191, total_tokens: 29414 },
  { input_tokens: 31753, cache_read_tokens: 29184, uncached_input_tokens: 2569, output_tokens: 89, total_tokens: 31842 },
  { input_tokens: 33534, cache_read_tokens: 31744, uncached_input_tokens: 1790, output_tokens: 157, total_tokens: 33691 },
];

/** Builds an isolated old-format event window whose terminal cache fields are missing.
 * 构建终态缓存字段缺失的隔离旧格式事件窗口。
 * Returns a fresh copy for mutation/isolation tests; no production store is accessed.
 * 返回供变更/隔离测试使用的新副本；不访问生产存储。 */
export function kimiPartialUsageEvents(): ConversationEventSummary[] {
  // Each API sample is a fresh canonical event with the original request identity.
  // 每笔 API 样本均为保留原请求身份的新规范事件。
  const events: ConversationEventSummary[] = KIMI_USAGE.map((usage, index) => ({
    seq: index + 1, turn_id: 2, event_revision: 1, created_at_ms: 1, updated_at_ms: 1, visibility: "ui_only",
    event: { kind: "api_stats", api_call_id: index * 2 + 3, purpose: "main_generation", provider: "kimi", profile: "openai-chat-completions.v1", model: "k3-256k", usage: { ...usage }, status: "completed", continuation_eligibility: "forbidden", timing: {} },
  }));
  // The old terminal summary intentionally lacks cache dimensions although seven APIs reported them.
  // 旧终态摘要故意缺少缓存维度，尽管七次 API 已报告该数据。
  const direct: StatBucket = {
    api_call_count: 8, unknown_usage_api_call_count: 0, tool_call_count: 7, subagent_return_count: 0,
    usage: { input_tokens: 213688, output_tokens: 1089, total_tokens: 214777 },
  };
  events.push({
    seq: 9, turn_id: 2, event_revision: 1, created_at_ms: 1, updated_at_ms: 1, visibility: "ui_only",
    event: { kind: "turn_stats", start_seq: 1, end_seq: 9, outcome: "completed", buckets: {
      direct, total: { ...direct, usage: { ...direct.usage } },
      subagent: { api_call_count: 0, unknown_usage_api_call_count: 0, tool_call_count: 0, subagent_return_count: 0, usage: {} },
    } },
  });
  return events;
}
