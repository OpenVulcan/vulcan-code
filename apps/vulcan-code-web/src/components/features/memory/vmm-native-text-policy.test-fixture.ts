import type { VmmNativeTextPolicySummary } from "@/types"

// textDefaults mirrors the Core-owned text policy solely for frontend response fixtures.
// textDefaults 仅为前端响应夹具镜像 Core 持有的文本策略。
export const textDefaults: VmmNativeTextPolicySummary = { foldMachineText: true, maxFragmentTokens: 1200 }
