import { describe, expect, it } from "vitest";

import { findKnowledgePackageNameConflict } from "./KnowledgePackageDialogs";
import type { KnowledgeLibrarySummary } from "@/types";

// libraryFixture builds one minimal authoritative library summary for conflict tests.
// libraryFixture 为冲突测试构建一个最小权威知识库摘要。
function libraryFixture(id: string, name: string): KnowledgeLibrarySummary {
  return {
    id,
    name,
    description: "",
    lifecycleState: "active",
    documentCount: 0,
    chunkCount: 0,
    indexStatus: "ready",
    createdAtMs: 1,
    updatedAtMs: 1,
  };
}

describe("findKnowledgePackageNameConflict", () => {
  it("matches the backend trim and non-locale lowercase contract", () => {
    const existing = libraryFixture("library-1", "项目资料");
    expect(findKnowledgePackageNameConflict([existing], "  项目资料  ")).toBe(existing);
  });

  it("returns the exact conflicting target and leaves unique names available", () => {
    const first = libraryFixture("library-1", "Alpha");
    const second = libraryFixture("library-2", "Beta");
    expect(findKnowledgePackageNameConflict([first, second], "bEtA")).toBe(second);
    expect(findKnowledgePackageNameConflict([first, second], "Gamma")).toBeNull();
    expect(findKnowledgePackageNameConflict([first, second], "   ")).toBeNull();
  });
});
