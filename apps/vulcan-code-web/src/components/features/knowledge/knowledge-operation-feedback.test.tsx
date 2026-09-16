// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { KnowledgeImportJobSummary } from "@/types";
import { KnowledgeOperationFeedback } from "./KnowledgeFeedback";
import { KnowledgeOperationProgress } from "./KnowledgeOperationProgress";
import { KnowledgeSkippedDocuments } from "./KnowledgeSkippedDocuments";

// failedJob reproduces partial curation and an incomplete stream snapshot with a durable failure.
// failedJob 复现部分整理成功、预览快照未完成但持久状态已失败的情形。
const failedJob: KnowledgeImportJobSummary = {
  id: "failed-job", libraryId: "library-a", operationKind: "display_build", state: "failed", phase: "curating",
  scannedFiles: 0, totalFiles: 0, processedFiles: 0, totalIndexItems: 1762, processedIndexItems: 1758,
  processedBytes: 0, warnings: [], errorMessage: "info.md: input exceeds limit",
  observability: {
    llmRequestCount: 411, llmRetryCount: 3, embeddingRequestCount: 0, generatedVectorCount: 0,
    inputTokens: 711936, outputTokens: 663157, cacheReadTokens: 196736, cacheWriteTokens: null,
    currentRequest: { requestId: "last", label: "窗口/设置/键盘监听事件.md", phase: "curating", provider: "deepseek", model: "deepseek-flash", protocol: "openai-responses.v1", status: "failed", outputMarkdown: "```json\n{\"symbols\":[\"KEY_A\"]}\n```", startedAtMs: 1 },
    recentRequests: [],
  },
};

describe("knowledge operation feedback", () => {
  // host/root own the mounted component and portal cleanup.
  // host/root 负责已挂载组件与门户清理。
  let host: HTMLDivElement;
  let root: Root;
  // copy records exact clipboard content without touching the user's clipboard.
  // copy 记录精确剪贴板内容，不接触用户剪贴板。
  const copy = vi.fn();
  beforeEach(() => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    copy.mockReset().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: copy } });
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

  it("keeps a long failure complete, copies it exactly, and lets details close and reopen", async () => {
    // message is deliberately much larger than a compact progress panel.
    // message 刻意远大于紧凑进度面板。
    const message = `显示库整理有 4 份文档失败\n${"KEY_A, ".repeat(2000)}\n最后一项`;
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeOperationFeedback message={message} /></I18nProvider>));
    expect(document.querySelector('[role="dialog"] pre')?.textContent).toBe(message);
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "复制详情")!.click());
    expect(copy).toHaveBeenCalledWith(message);
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "关闭")!.click());
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(host.querySelector('[role="alert"]')).not.toBeNull();
    await act(async () => host.querySelector<HTMLButtonElement>('[role="alert"] button')!.click());
    expect(document.querySelector('[role="dialog"] pre')?.textContent).toBe(message);
  });

  it("retains diagnostics and offers manual selection if clipboard access fails", async () => {
    copy.mockRejectedValue(new Error("clipboard denied"));
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeOperationFeedback message="完整原始错误" /></I18nProvider>));
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "复制详情")!.click());
    expect(document.querySelector('[role="status"]')?.textContent).toContain("手动复制");
    expect(document.querySelector('[role="dialog"] pre')?.textContent).toBe("完整原始错误");
  });

  it("projects durable failure, preserves completed counts, and folds JSON output by default", async () => {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeOperationProgress job={failedJob} running={false} cancelRequested={false} outcome={null} /></I18nProvider>));
    expect(host.querySelector('.knowledge-operation-progress-head strong')?.textContent).toBe("操作失败");
    expect(host.querySelector('.knowledge-operation-progress-meta')?.textContent).toContain("1758 / 1762");
    expect(host.querySelector('.knowledge-operation-progress-meta')?.textContent).not.toContain("bytes");
    expect(host.querySelector<HTMLDetailsElement>('details')?.open).toBe(false);
    expect(host.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("99");
  });

  it("shows completion at 100 percent while distinguishing generated documents from skipped sources", async () => {
    // job models the reported 1761 successes plus one explicitly skipped index file.
    // job 模拟用户报告的 1761 份成功结果和一份被明确忽略的索引文件。
    const job: KnowledgeImportJobSummary = {
      ...failedJob, state: "completed", phase: "completed", processedIndexItems: 1762, errorMessage: null,
      observability: { ...failedJob.observability!, skippedDocuments: [{ sourceDocumentId: "info", relativePath: "info.md", byteSize: 147383, byteLimit: 65536 }] },
    };
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeOperationProgress job={job} running={false} cancelRequested={false} outcome={null} /></I18nProvider>));
    expect(host.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("100");
    expect(host.querySelector('.knowledge-operation-progress-head strong')?.textContent).toBe("操作完成");
    expect(host.textContent).toContain("已生成 1761 · 已忽略 1");
    expect(host.textContent).toContain("info.md");
    expect(host.textContent).toContain("147383 字节 > 65536 字节");
    expect(host.querySelector('[role="alert"]')).toBeNull();
  });

  it("retains the entire skipped list in an in-window copyable dialog and removes it after supplementation", async () => {
    // documents intentionally exceed the old twenty-item diagnostic preview limit.
    // documents 刻意超过旧诊断预览的二十条上限。
    const documents = Array.from({ length: 25 }, (_, index) => ({ sourceDocumentId: String(index), relativePath: `目录/${index}.md`, byteSize: 65537 + index, byteLimit: 65536 }));
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeSkippedDocuments documents={documents} /></I18nProvider>));
    expect(host.querySelectorAll('li')).toHaveLength(25);
    await act(async () => host.querySelector<HTMLButtonElement>('button')!.click());
    expect(document.querySelector('[role="dialog"] pre')?.textContent).toContain("目录/24.md");
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "复制详情")!.click());
    expect(copy).toHaveBeenCalledWith(expect.stringContaining("目录/24.md"));
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "关闭")!.click());
    expect(host.querySelectorAll('li')).toHaveLength(25);
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeSkippedDocuments documents={[]} /></I18nProvider>));
    expect(host.textContent).toBe("");
  });

  it("completes an all-skipped build without claiming a generated document", async () => {
    // job has no generated display content but one processed exclusion.
    // job 没有已生成显示内容，但包含一条已处理忽略项。
    const job: KnowledgeImportJobSummary = {
      ...failedJob, state: "completed", phase: "completed", totalIndexItems: 1, processedIndexItems: 1,
      observability: { ...failedJob.observability!, skippedDocuments: [{ sourceDocumentId: "info", relativePath: "info.md", byteSize: 147383, byteLimit: 65536 }] },
    };
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeOperationProgress job={job} running={false} cancelRequested={false} outcome={null} /></I18nProvider>));
    expect(host.textContent).toContain("已生成 0 · 已忽略 1");
    expect(host.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("100");
  });
});
