import type { VmmNativeRetentionSettingsSummary } from "@/types"

// retentionDefaults mirrors the authoritative host response for UI-only regressions.
// retentionDefaults 镜像权威宿主响应，仅用于界面回归。
export const retentionDefaults: VmmNativeRetentionSettingsSummary = {
  enabled: true, scanIntervalMinutes: 30, profileExpiryIntervalSeconds: 30,
  turnHotWindowSize: 8, sessionIdleMs: 1296000000, trashRetentionMs: 2592000000,
  protectedPriorityFloor: 60, protectedMemoryLevelFloor: 3, skipProtectedSharedMemories: true,
}
