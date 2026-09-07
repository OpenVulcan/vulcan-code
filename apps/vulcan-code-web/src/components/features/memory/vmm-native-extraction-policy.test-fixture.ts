import type { VmmNativeRecallPolicySummary } from "@/types"

// Exact extraction defaults from MemoryPolicy::default, shared by sibling policy response fixtures.
// 来自 MemoryPolicy::default 的准确提炼默认值，由相邻策略响应夹具共用。
export const extractionDefaults: Pick<VmmNativeRecallPolicySummary, "postactionMemoryPriority" | "postactionMemoryStability" | "postactionMemoryLevel" | "postactionMemoryRefreshWeight" | "postactionMemoryExpiryMs"> = {
  postactionMemoryPriority: 20,
  postactionMemoryStability: "stable",
  postactionMemoryLevel: "stable",
  postactionMemoryRefreshWeight: 1,
  postactionMemoryExpiryMs: 180 * 86400000,
}
