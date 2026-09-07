import type { VmmNativeRetrievalPolicySummary } from "@/types"

// rankingDefaults mirrors the current Core v1 defaults for UI-only response fixtures.
// rankingDefaults 在仅界面响应夹具中镜像当前 Core v1 默认值。
export const rankingDefaults: VmmNativeRetrievalPolicySummary = {
  hybridEnabled: true, lexicalCandidateFloor: 8, rrfK: "60",
  replaceMinSimilarity: 0.8, hardDedupeCosineThreshold: 0.99,
  hardDedupePoolTopK: 16, replacementTopK: 5, rerankTopN: 32,
  mmrEnabled: true, mmrLambda: 0.75, weibullEnabled: true,
  weibullShape: 1.35, weibullScaleHours: 2160, weibullMinMultiplier: 0.4,
  weibullReinforceWeight: 0.18, weibullCrossSessionBoost: 0.12,
}
