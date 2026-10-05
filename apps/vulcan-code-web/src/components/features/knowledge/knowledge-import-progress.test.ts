import { describe, expect, it } from "vitest";

import type { KnowledgeImportJobSummary } from "@/types";

import { knowledgeImportHasCommittedContent } from "./use-knowledge-operation-job";

// importJob builds one exact durable operation snapshot.
// importJob 构建一份准确的持久化操作快照。
function importJob(overrides: Partial<KnowledgeImportJobSummary> = {}): KnowledgeImportJobSummary {
  return {
    id: "job-1",
    libraryId: "library-1",
    operationKind: "import",
    state: "running",
    phase: "parsing",
    scannedFiles: 10,
    totalFiles: 10,
    processedFiles: 4,
    totalIndexItems: 0,
    processedIndexItems: 0,
    currentFile: "guide/intro.md",
    processedBytes: 1024,
    warnings: [],
    errorMessage: null,
    ...overrides,
  };
}

describe("knowledgeImportHasCommittedContent", () => {
  it("exposes authoritative counts as soon as an import reaches post-commit work", () => {
    expect(knowledgeImportHasCommittedContent(importJob({ phase: "committing" }))).toBe(false);
    expect(knowledgeImportHasCommittedContent(importJob({ phase: "indexing" }))).toBe(true);
    expect(knowledgeImportHasCommittedContent(importJob({ phase: "verifying" }))).toBe(true);
    expect(knowledgeImportHasCommittedContent(importJob({ phase: "finalizing" }))).toBe(true);
  });

  it("does not treat index-only maintenance as a new content commit", () => {
    expect(knowledgeImportHasCommittedContent(importJob({ operationKind: "repair", phase: "repairing" }))).toBe(false);
    expect(knowledgeImportHasCommittedContent(importJob({ operationKind: "rebuild", phase: "rebuilding" }))).toBe(false);
  });
});
