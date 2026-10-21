// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { KnowledgeOperationSummary } from "@/types/knowledge-base";
import { KnowledgeIndex } from "./knowledge-index";
import { knowledgeLibraryFixture } from "./knowledge-test-fixtures";

// Live status is simulated without a provider or real service connection.
// 实时状态通过模拟实现，不连接供应商或真实服务。
const service = vi.hoisted(() => ({ operation: vi.fn(), vectors: vi.fn(), integrity: vi.fn() }));
vi.mock("@/service/knowledge-base-service", () => ({ getKnowledgeOperation: service.operation, getKnowledgeEmbeddingStatus: service.vectors, getKnowledgeIndexIntegrity: service.integrity }));
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
// A stable operation identity remains cancellable across every poll.
// 稳定操作身份在每次轮询中保持可取消。
const operation: KnowledgeOperationSummary = { id: "operation-a", libraryId: knowledgeLibraryFixture.id, kind: "rebuild", state: "running", phase: "indexing", totalItems: 8, processedItems: 2, currentItem: null, warnings: [], errorMessage: null, createdAtMs: 1, startedAtMs: 2, finishedAtMs: null };
// One test root owns all timers and effects.
// 一个测试根拥有全部计时器及副作用。
let root: Root;
// This host never shares the real application DOM.
// 此宿主绝不共享真实应用 DOM。
let host: HTMLDivElement;
beforeEach(() => {
  vi.resetAllMocks(); vi.useFakeTimers(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  service.operation.mockResolvedValue(operation);
  service.vectors.mockResolvedValue({ desiredSpaceId: null, activeSpaceId: null, pending: 0, ready: 0, failed: 0, paused: false, failures: [], nextOffset: null });
  service.integrity.mockResolvedValue({ indexMode: "bm25", documentCount: 0, chunkCount: 0, ftsPresent: true, indexedChunkCount: 0, mismatchedRows: 0, headChangeSeq: 0, indexedChangeSeq: 0, dictionaryRevision: 0, indexedDictionaryRevision: 0, healthy: true });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers(); vi.unstubAllGlobals(); });

it("does not overlap status polls and stops at the durable terminal state", async () => {
  // Hold a real read promise across several presentation intervals.
  // 将真实读取 Promise 保持等待，跨越多个展示间隔。
  let finish!: (value: KnowledgeOperationSummary) => void;
  service.operation.mockResolvedValueOnce(operation).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => root.render(<KnowledgeIndex library={knowledgeLibraryFixture} busy={false} onRebuild={vi.fn()} onRepair={vi.fn()} onCancel={vi.fn()} onRetry={vi.fn()} />));
  await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
  expect(service.operation).toHaveBeenCalledTimes(2);
  await act(async () => finish({ ...operation, state: "completed", processedItems: 8, finishedAtMs: 3 }));
  await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
  expect(service.operation).toHaveBeenCalledTimes(2);
  expect(host.textContent).toContain("knowledge.operation.completed");
});

it("cancels the exact observed operation and never replays cancellation while polling", async () => {
  // Cancellation is a mutation distinct from every status query.
  // 取消为不同于每个状态查询的变更。
  const cancel = vi.fn().mockResolvedValue(undefined);
  await act(async () => root.render(<KnowledgeIndex library={knowledgeLibraryFixture} busy={false} onRebuild={vi.fn()} onRepair={vi.fn()} onCancel={cancel} onRetry={vi.fn()} />));
  // Locate by stable semantic label.
  // 按稳定语义标签定位。
  const button = [...host.querySelectorAll("button")].find((entry) => entry.textContent === "knowledge.index.cancel")!;
  service.operation.mockResolvedValue({ ...operation, state: "cancelled" });
  await act(async () => button.click());
  await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
  expect(cancel).toHaveBeenCalledExactlyOnceWith(operation.id);
  expect(host.textContent).toContain("knowledge.operation.cancelled");
});

it("requires a current job read before enabling a new durable index operation", async () => {
  service.operation.mockRejectedValue(new Error("job read unavailable"));
  // A failed status read cannot authorize rebuilding behind an unknown live job.
  // 状态读取失败不能授权在未知实时任务后重建。
  const rebuild = vi.fn();
  await act(async () => root.render(<KnowledgeIndex library={knowledgeLibraryFixture} busy={false} onRebuild={rebuild} onRepair={vi.fn()} onCancel={vi.fn()} onRetry={vi.fn()} />));
  // Catalog identity locates this exact operation control.
  // 目录身份定位此精确操作控件。
  const button = [...host.querySelectorAll("button")].find((entry) => entry.textContent === "knowledge.rebuildIndex")!;
  expect(button.disabled).toBe(true);
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("job read unavailable");
  await act(async () => button.click());
  expect(rebuild).not.toHaveBeenCalled();
});
