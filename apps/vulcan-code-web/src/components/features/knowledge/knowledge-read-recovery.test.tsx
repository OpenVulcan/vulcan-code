// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { KnowledgeIndexIntegritySummary, KnowledgeLibrarySummary } from "@/types";
import { KnowledgeBaseArea } from "./knowledge-base-area";

// reads isolates live storage while preserving the production component and event handlers.
// reads 隔离真实存储，同时保留生产组件与事件处理函数。
const reads = vi.hoisted(() => ({ libraries: vi.fn(), sources: vi.fn(), integrity: vi.fn(), job: vi.fn(), create: vi.fn() }));
vi.mock("@/service/knowledge-base-service", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/service/knowledge-base-service")>(),
  listKnowledgeLibraries: reads.libraries,
  listKnowledgeSources: reads.sources,
  getKnowledgeIndexIntegrity: reads.integrity,
  getKnowledgeImportJob: reads.job,
  createKnowledgeLibrary: reads.create,
}));
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: translate, locale: "zh-CN" }) }));
// translate exposes stable keys for assertions without changing callback identity between renders.
// translate 使用稳定键进行断言，不在渲染间改变回调身份。
// Parameters: key is the message identifier. Returns: the same identifier.
// 参数：key 是文案标识。返回值：同一标识。
function translate(key: string): string { return key; }

// library represents a valid empty store, not a fabricated successful error result.
// library 表示有效的空知识库，不是为错误伪造的成功结果。
const library: KnowledgeLibrarySummary = {
  id: "library-a", name: "Library A", description: "", lifecycleState: "active", documentCount: 0, chunkCount: 0, indexStatus: "empty", createdAtMs: 1, updatedAtMs: 1,
  settings: { retrievalMode: "raw_bm25", fallbackPolicy: "explicit_only", curation: { enabled: false, promptVersion: "knowledge-display-v1" }, displayIndex: { status: "not_created", generatedDocumentCount: 0 }, embedding: { enabled: false, embeddingBatchSize: 10, batchSizeSource: "fallback_default" }, vectorIndex: { status: "disabled" } },
};
// integrity is a complete physical-index inspection for this empty library.
// integrity 是该空知识库的完整物理索引检查。
const integrity: KnowledgeIndexIntegritySummary = {
  libraryId: library.id, indexExists: true, searchable: false, indexStatus: "empty", tokenizer: "jieba", contentRevision: 0, indexedContentRevision: 0,
  dictionaryRevision: 0, indexedDictionaryRevision: 0, authoritativeChunkCount: 0, indexedChunkCount: 0, missingOrMismatchedCount: 0, orphanOrMismatchedCount: 0,
};

describe("knowledge page read recovery", () => {
  // host contains the actual page; root owns its effects and cleanup.
  // host 容纳实际页面；root 拥有副作用与清理。
  let host: HTMLDivElement;
  let root: Root;
  beforeEach(() => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    vi.resetAllMocks();
    reads.libraries.mockResolvedValue([library]); reads.sources.mockResolvedValue([]); reads.integrity.mockResolvedValue(integrity); reads.job.mockResolvedValue(null);
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

  it("keeps failed registry reads distinct from an empty library and recovers without a write", async () => {
    reads.libraries.mockRejectedValueOnce(new Error("registry unavailable"));
    await act(async () => root.render(<KnowledgeBaseArea />));
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("registry unavailable");
    expect(host.textContent).not.toContain("knowledge.emptyLibraries");
    await act(async () => host.querySelector<HTMLButtonElement>('[role="alert"] button')!.click());
    expect(host.querySelector('[role="alert"]')).toBeNull();
    expect(host.textContent).toContain(library.name);
    expect(reads.libraries).toHaveBeenCalledTimes(2);
    expect(reads.create).not.toHaveBeenCalled();
  });

  it("isolates source and integrity failures and retries only the selected read", async () => {
    reads.sources.mockRejectedValueOnce(new Error("source unavailable"));
    reads.integrity.mockRejectedValueOnce(new Error("inspection unavailable"));
    await act(async () => root.render(<KnowledgeBaseArea />));
    expect(host.querySelectorAll('[role="alert"]')).toHaveLength(2);
    expect(host.textContent).not.toContain("knowledge.sources.empty");
    await act(async () => host.querySelector<HTMLButtonElement>('.knowledge-sources [role="alert"] button')!.click());
    expect(host.querySelector('.knowledge-sources [role="alert"]')).toBeNull();
    expect(host.textContent).toContain("knowledge.sources.empty");
    expect(reads.sources).toHaveBeenCalledTimes(2);
    expect(reads.integrity).toHaveBeenCalledTimes(1);
    await act(async () => host.querySelector<HTMLButtonElement>('.knowledge-index-health [role="alert"] button')!.click());
    expect(host.querySelectorAll('[role="alert"]')).toHaveLength(0);
    expect(host.querySelector('.knowledge-index-health-state')?.textContent).toBe("knowledge.indexStatus.empty");
    expect(reads.integrity).toHaveBeenCalledTimes(2);
    expect(reads.create).not.toHaveBeenCalled();
  });

  it("discards a delayed source failure when the user switches libraries", async () => {
    // rejectOld settles the obsolete A read only after B has published its current result.
    // rejectOld 仅在 B 发布当前结果后结束已过期的 A 读取。
    let rejectOld!: (reason: Error) => void;
    reads.libraries.mockResolvedValue([library, { ...library, id: "library-b", name: "Library B" }]);
    reads.sources.mockImplementation((id: string) => id === "library-a" ? new Promise((_, reject) => { rejectOld = reject; }) : Promise.resolve([]));
    reads.integrity.mockImplementation((id: string) => Promise.resolve({ ...integrity, libraryId: id }));
    await act(async () => root.render(<KnowledgeBaseArea />));
    await act(async () => host.querySelectorAll<HTMLButtonElement>('.knowledge-library-select')[1].click());
    await act(async () => rejectOld(new Error("obsolete source failure")));
    expect(host.querySelector('[role="alert"]')).toBeNull();
    expect(host.textContent).toContain("knowledge.sources.empty");
    expect(host.querySelector('.knowledge-library-row.active')?.textContent).toContain("Library B");
  });

  it("does not let an older registry request overwrite a successful explicit refresh", async () => {
    // resolveOld retains the first pending list so it can return after the user's refresh.
    // resolveOld 保留首个未完成目录请求，使其在用户刷新之后返回。
    let resolveOld!: (value: KnowledgeLibrarySummary[]) => void;
    reads.libraries.mockImplementationOnce(() => new Promise((resolve) => { resolveOld = resolve; }));
    await act(async () => root.render(<KnowledgeBaseArea />));
    expect(host.textContent).toContain("knowledge.librariesLoading");
    expect(host.textContent).not.toContain("knowledge.emptyLibraries");
    await act(async () => Array.from(host.querySelectorAll<HTMLButtonElement>('.knowledge-toolbar button')).find((button) => button.textContent === "knowledge.refresh")!.click());
    await act(async () => resolveOld([]));
    expect(host.querySelector('.knowledge-library-select')?.textContent).toContain(library.name);
    expect(host.textContent).not.toContain("knowledge.emptyLibraries");
  });
});
