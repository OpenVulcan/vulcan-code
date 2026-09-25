import { describe, expect, it } from "vitest";

import type { KnowledgeImportJobSummary } from "@/types";

import { knowledgeImportHasCommittedContent, knowledgeImportProgressPercent } from "./knowledge-base-area";

// importJob builds one exact progress snapshot for percentage tests.
// importJob 为百分比测试构建一份准确的进度快照。
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

describe("knowledgeImportProgressPercent", () => {
  it("keeps progress indeterminate until scanning establishes the total", () => {
    expect(knowledgeImportProgressPercent(null)).toBeNull();
    expect(knowledgeImportProgressPercent(importJob({ phase: "scanning", totalFiles: 0, processedFiles: 0 }))).toBeNull();
  });

  it("reports and bounds deterministic processing progress", () => {
    expect(knowledgeImportProgressPercent(importJob())).toBe(40);
    expect(knowledgeImportProgressPercent(importJob({ processedFiles: 11 }))).toBe(100);
    expect(knowledgeImportProgressPercent(importJob({ processedFiles: -1 }))).toBe(0);
  });

  it("switches to index work progress during incremental and recovery indexing", () => {
    expect(knowledgeImportProgressPercent(importJob({
      phase: "rebuilding",
      totalIndexItems: 0,
      processedIndexItems: 0,
    }))).toBeNull();
    expect(knowledgeImportProgressPercent(importJob({
      phase: "rebuilding",
      processedFiles: 10,
      totalIndexItems: 3826,
      processedIndexItems: 1913,
    }))).toBe(50);
    expect(knowledgeImportProgressPercent(importJob({
      operationKind: "repair",
      phase: "repairing",
      totalFiles: 0,
      totalIndexItems: 40,
      processedIndexItems: 10,
    }))).toBe(25);
  });
});

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
