import type { PluginResourceDiagnostic, PluginResourceDiagnostics } from "@/types/plugin-resource-diagnostics";

// diagnosticResource models a safely retained owner with distinct host, conversation and core identities.
// diagnosticResource 模拟安全保留所有者，区分宿主、对话及核心身份。
export const diagnosticResource: PluginResourceDiagnostic = {
  resourceKind: "pinned_session", resourceId: "host-session-original", pluginGeneration: "9007199254740993", ownerSessionId: "conversation-original",
  originalPoolId: "pool-original", coreSessionId: "core-session-original", corePhase: "closing", activeOperationId: null, finalizationOperationId: "close-operation-original",
  preparing: false, closing: true, retired: false, isDrainBlocking: true, cleanupRecovery: true, queuedCalls: 0, borrowers: null, pendingFinalizations: false,
  finalizationOperationIds: [], operations: [{ operationId: "close-operation-original", phase: "failed", cancellationRequested: false, errors: [{ stage: "finalization", code: "cleanup_failed" }] }],
  errors: [{ stage: "cleanup", code: null }],
};
// diagnosticReport is a non-atomic host report whose pinned count excludes other calls and terminal history.
// diagnosticReport 是非原子宿主报告，固定计数排除其他调用及终态历史。
export const diagnosticReport: PluginResourceDiagnostics = {
  runtimeId: "parent-original", serviceEpoch: "epoch-original", pluginId: "fixture", pluginGeneration: null, nonAtomicSnapshot: true, pinnedSessionCount: 1,
  resources: [diagnosticResource,
    { ...diagnosticResource, resourceId: "retired-host-session", retired: true, isDrainBlocking: false, cleanupRecovery: false, corePhase: "closed" },
    { ...diagnosticResource, resourceKind: "formal_call", resourceId: "internal-call", ownerSessionId: null }],
};
