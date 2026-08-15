import { describe, expect, it } from "vitest";

import type { KnowledgeDisplayIndexState, KnowledgeDocumentSummary } from "@/types";
import { buildKnowledgeDocumentTree, defaultKnowledgeBrowseCorpus } from "./KnowledgeLibraryViewerDialog";

// knowledgeDocument creates one complete document summary for tree tests.
// knowledgeDocument 为目录树测试创建一份完整文档摘要。
function knowledgeDocument(id: string, relativePath: string): KnowledgeDocumentSummary {
  // segments contains the slash-normalized path components used to derive the title and folder.
  // segments 包含用于派生标题与文件夹的斜杠规范化路径片段。
  const segments = relativePath.split("/");
  return {
    id,
    sourceId: "source-1",
    relativePath,
    folderPath: segments.slice(0, -1).join("/"),
    title: segments[segments.length - 1],
    byteSize: 10,
    lineCount: 2,
    chunkCount: 1,
    updatedAtMs: 1,
  };
}

describe("buildKnowledgeDocumentTree", () => {
  it("builds nested folders and keeps folders before root documents", () => {
    // tree is the deterministic hierarchy produced from unsorted document input.
    // tree 是从未排序文档输入生成的确定性层级。
    const tree = buildKnowledgeDocumentTree([
      knowledgeDocument("root", "README.md"),
      knowledgeDocument("nested-b", "guide/setup/windows.md"),
      knowledgeDocument("nested-a", "guide/intro.md"),
    ]);

    expect(tree.children.map((node) => node.name)).toEqual(["guide", "README.md"]);
    expect(tree.children[0].children.map((node) => node.name)).toEqual(["setup", "intro.md"]);
    expect(tree.children[0].children[0].children[0].document?.id).toBe("nested-b");
  });

  it("returns an empty root for an empty library", () => {
    expect(buildKnowledgeDocumentTree([])).toEqual({ path: "", name: "", children: [] });
  });

  it("builds lightweight folder nodes before any folder documents are loaded", () => {
    // tree represents server-provided directory paths without requiring document summaries.
    // tree 表示服务端提供的目录路径，无需预先取得文档摘要。
    const tree = buildKnowledgeDocumentTree([], ["guide/setup/windows", "reference/api"]);

    expect(tree.children.map((node) => node.name)).toEqual(["guide", "reference"]);
    expect(tree.children[0].children[0].children[0]).toMatchObject({
      path: "guide/setup/windows",
      name: "windows",
      children: [],
    });
  });
});

// displayIndex creates the exact display lifecycle state used by corpus-default tests.
// displayIndex 创建内容层默认值测试使用的精确显示库生命周期状态。
function displayIndex(activeGeneration: string | null): KnowledgeDisplayIndexState {
  return {
    status: activeGeneration ? "ready" : "not_created",
    activeGeneration,
    generatedDocumentCount: activeGeneration ? 1 : 0,
  };
}

describe("defaultKnowledgeBrowseCorpus", () => {
  it("opens a published display generation by default", () => {
    expect(defaultKnowledgeBrowseCorpus(displayIndex("generation-1"))).toBe("display");
  });

  it("keeps raw browsing when no display generation is published", () => {
    expect(defaultKnowledgeBrowseCorpus(displayIndex(null))).toBe("raw");
  });
});
