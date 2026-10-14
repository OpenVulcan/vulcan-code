import type { VmmMemoryActionResult, VmmNativeRecycleBatchSummary } from "@/types"

// batch retains explicit synthetic identities, independent of its mutable display name.
// batch 保留明确合成身份，与可变显示名称无关。
export const maintenanceBatch: VmmNativeRecycleBatchSummary = {
  batchId: "audit-batch-19", projectId: 7, reason: "manual_session_recycle",
  manualSelection: { kind: "sessions", ids: ["audit-original-session"] },
  recycledAtUnixMs: 1789200000000, expiresAtUnixMs: 1789804800000,
  turnCount: 1, memoryCount: 1, profileNodeCount: 0, contextEvidenceCount: 0,
}

/** Returns a typed synthetic maintenance reply without contacting storage or inference.
 * 返回强类型合成维护响应，不访问存储或推理。
 * action selects a declared endpoint; payload retains its request; longName exercises wrapping.
 * action 选择已声明端点；payload 保留请求；longName 验证换行。
 * Returns an exact action envelope, throwing for undeclared fixture actions.
 * 返回准确动作信封；未声明夹具动作明确抛错。
 */
export function maintenanceReply(action: string, payload?: Record<string, unknown>, longName = false): VmmMemoryActionResult {
  // data uses the production discriminated union to check every fixture's fields.
  // data 使用生产判别联合校验每种夹具字段。
  let data: VmmMemoryActionResult["data"]
  switch (action) {
    case "native_retention_status":
      data = { kind: action, value: { enabled: true, running: false, profileExpiryRunning: false, intervalMinutes: 15,
        profileExpiryIntervalSeconds: 60, pendingColdJobs: 0, recycleBatchCount: 1,
        policy: { schemaVersion: 1, turnHotWindowSize: 8, sessionIdleMs: 60000, trashRetentionMs: 86400001,
          protectedPriorityFloor: 80, protectedMemoryLevelFloor: 3, skipProtectedSharedMemories: true,
          profileExpiryLimit: 100, memoryRecycleLimit: 100, coldTurnScanLimit: 100, coldTurnJobLimit: 100,
          idleSessionLimit: 100, vectorGcLimit: 100, trashPurgeLimit: 100 } } }
      break
    case "native_lexical_status": data = { kind: action, value: { storageVersion: 19, activeMemoryCount: 1, documentCount: 1, indexedRowCount: 3 } }; break
    case "native_vector_status": data = { kind: action, value: { configured: true, embeddingDimension: 1024,
      activeMemoryCount: 1, recycleBinMemoryCount: 1, currentMemoryCount: 0, currentRecycleBinMemoryCount: 0,
      rebuildRequiredMemoryCount: 2, activeRebuildRequiredMemoryCount: 1, recycleBinRebuildRequiredMemoryCount: 1,
      legacyVectorSpaceCount: 1, pendingAnalysisJobCount: 0 } }; break
    case "native_project_catalog": data = { kind: action, value: { projects: [7, 8].map((projectId) => ({
      projectId, name: longName ? "SyntheticMemoryProject".repeat(35) : "同名合成项目", tag: `audit-${projectId}`,
      protected: false, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [],
    })), bindings: [] } }; break
    case "native_recycle_bin": data = { kind: action, value: { storageVersion: 19, batches: payload?.vmmProjectId === 8 ? [] : [maintenanceBatch] } }; break
    case "native_recycle_restore":
    case "native_recycle_purge": data = { kind: "native_recycle_batch", value: maintenanceBatch }; break
    case "native_vector_rebuild_preview": data = { kind: action, value: { storageVersion: 19, targetVectorSpaceId: "audit-vector",
      embeddingDimension: 1024, activeMemoryCount: 1, recycleBinMemoryCount: 1, rebuildMemoryCount: 2,
      activeRebuildMemoryCount: 1, recycleBinRebuildMemoryCount: 1, affectedProjectCount: 1, batchCount: 1, previewFingerprint: "audit-fingerprint", adoptableMemoryCount: 1 } }; break
    case "native_vector_rebuild": data = { kind: action, value: { storageVersion: 20, targetVectorSpaceId: "audit-vector", rebuiltMemoryCount: 2, batchCount: 1 } }; break
    case "native_vector_adoption": data = { kind: action, value: { storageVersion: 20, targetVectorSpaceId: "audit-vector", adopted: payload?.expectedPreviewFingerprint === "audit-fingerprint",
      adoptedMemoryCount: 1, remainingRebuildMemoryCount: 1, sampleSimilarity: 0.9999, requiredSimilarity: 0.95 } }; break
    case "native_lexical_rebuild": data = { kind: action, value: { storageVersion: 19, reindexedMemoryCount: 1 } }; break
    case "native_retention_preview": data = { kind: action, value: { previewId: "audit-cleanup", storageVersion: 19,
      evaluatedAtUnixMs: 1789200000000, expiresAtUnixMs: 1789200300000, projects: [] } }; break
    case "native_retention_run": data = { kind: action, value: { expiredProfileNodes: 0, recycledMemories: 0, recycledTurns: 0,
      enqueuedColdJobs: 0, completedColdJobs: 0, purgedTrashBatches: 0, projectFailures: [] } }; break
    default: throw new Error(`Unexpected synthetic maintenance action: ${action}`)
  }
  return { action, kind: action, summary: "", request: payload, response: data.value, data }
}
