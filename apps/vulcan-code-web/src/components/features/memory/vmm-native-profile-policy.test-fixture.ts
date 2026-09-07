import type { VmmNativeProfilePolicySummary } from "@/types"

// Tests mirror Core defaults only; production defaults always arrive from the host.
// 测试仅镜像 Core 默认值；生产默认值始终由宿主返回。
export const profileDefaults: VmmNativeProfilePolicySummary = {
  user: {
    transient: { baseMs: 259200000, stepMs: 172800000, maxMs: 1209600000 },
    situational: { baseMs: 1209600000, stepMs: 604800000, maxMs: 7776000000 },
    stable: { baseMs: 7776000000, stepMs: 2592000000, maxMs: 31536000000 },
  },
  project: {
    transient: { baseMs: 259200000, stepMs: 172800000, maxMs: 1209600000 },
    situational: { baseMs: 1209600000, stepMs: 604800000, maxMs: 7776000000 },
    stable: { baseMs: 7776000000, stepMs: 2592000000, maxMs: 31536000000 },
  },
}
