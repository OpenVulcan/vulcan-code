import type { VceProjectActivityResponse } from "@/types";

// Create complete host activity for projectId/revision; returns fresh data without IO or index creation.
// 为 projectId/revision 创建完整宿主活动；返回全新数据，不执行输入输出或创建索引。
export function createVceActivityFixture(projectId = "project-a", revision = 4): VceProjectActivityResponse {
  return {
    configuration: {
      ready: true, consumerProfileId: "vce", queryRewritePurposeId: "code_retrieval.query_rewrite", reasoningPolicy: "fixed_off",
      embedding: { configured: true, available: true }, queryRewrite: { configured: true, available: true },
      rerank: { configured: true, available: true }, issues: [],
    },
    settings: {
      enabled: true, updateMode: "manual", autoUpdateMinIntervalSeconds: 300, revision,
      migratedFromCodebaseMemory: false, maxFileBytes: 1048576,
      defaultIgnorePatterns: ["*.lock", "*.map", "generated/"], extraIgnorePatterns: [], allowQueryRewrite: false,
      allowRerank: true, embeddingMaxConcurrency: 4, queryMaxConcurrency: 3, maxResults: 10, maxContextChars: 32000,
    },
    status: {
      projectId, enabled: true, settingsRevision: revision, lifecycle: "ready", checkpointVersion: 2,
      presentFiles: 7, checkpointBlobs: 6,
    },
    queryAvailable: true, lastSuccessfulUpdateUnixMs: 1000,
    sourceFreshness: { watching: false, pendingChanges: false, reconciliationRequired: true },
  };
}
