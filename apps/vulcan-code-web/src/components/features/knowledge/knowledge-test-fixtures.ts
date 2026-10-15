import type { KnowledgeDocumentSummary, KnowledgeLibrarySummary } from "@/types/knowledge-base";

// A synthetic current-format registry entry never accesses a real store or model.
// 合成当前格式注册表条目绝不访问真实存储或模型。
export const knowledgeLibraryFixture: KnowledgeLibrarySummary = {
  id: "library-a", name: "Library A", description: "Synthetic purpose", lifecycleState: "active", errorMessage: null,
  settings: { indexMode: "bm25", embedding: null }, documentCount: 0, chunkCount: 0, pendingSummaryCount: 0,
  indexStatus: "ready", headChangeSeq: 0, lastChangeAtMs: null, createdAtMs: 1, updatedAtMs: 1,
};

// An immutable document summary provides exact identity and historical image fields.
// 不可变文档摘要提供精确身份及历史镜像字段。
export const knowledgeDocumentFixture: KnowledgeDocumentSummary = {
  id: "document-a", path: "notes/a.md", pathKey: "notes/a.md", folderPath: "notes", title: "A document", summary: "A summary", summaryState: "provided",
  keywords: [], bodySha256: "a".repeat(64), lineCount: 1, embedPolicy: "none", provenance: [], revision: 1, createdChangeSeq: 1, updatedChangeSeq: 1,
};
