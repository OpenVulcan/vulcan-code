// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { KnowledgeFileDisposition, KnowledgeFilePreview, KnowledgeFilePreviewEntry, KnowledgeIgnoredFile } from "@/types";
import { KnowledgeFileConfirmationDialog } from "./KnowledgeFileConfirmationDialog";
import { previewKnowledgeImport, previewKnowledgeDisplay, updateKnowledgeIgnoredFiles } from "@/service/knowledge-base-service";

vi.mock("@/service/knowledge-base-service", () => ({ previewKnowledgeImport: vi.fn(), previewKnowledgeDisplay: vi.fn(), updateKnowledgeIgnoredFiles: vi.fn() }));

// row constructs a protocol-faithful fixture with an exact root/path identity and explicit eligibility.
// row 构造符合协议的夹具，包含精确根目录及路径身份和明确可用性。
function row(name: string, disposition: KnowledgeFileDisposition = "ready"): KnowledgeFilePreviewEntry {
  return { key: name, sourceRoot: "D:/docs", relativePath: name, byteSize: disposition === "oversized" ? 70000 : 100,
    disposition, detail: null, documentId: null, contentSha256: "hash", alreadyCompleted: disposition === "completed" };
}

// button finds one visible action by its exact localized text; returns its HTML button.
// button 根据精确本地化文案查找一个可见操作，返回 HTML 按钮。
function button(label: string): HTMLButtonElement {
  return [...document.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === label)!;
}

describe("knowledge file confirmation", () => {
  // host/root own the mounted dialog and portals.
  // host/root 持有已挂载弹窗和门户。
  let host: HTMLDivElement;
  let root: Root;
  // confirm/close/refresh observe intent without calling live services.
  // confirm/close/refresh 观察意图，不调用真实服务。
  const confirm = vi.fn();
  const close = vi.fn();
  const refresh = vi.fn().mockResolvedValue(undefined);
  beforeEach(() => {
    vi.clearAllMocks();
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers(); });

  // Read-only waiting must never trap the user, and late results must not re-enable confirmation.
  // 只读等待绝不能困住用户，延迟结果也不能重新启用确认。
  it("allows closing a pending scan and aborts its transport without starting work", async () => {
    // Completion is controlled independently from cancellation to model a late server response.
    // 完成与取消独立控制，以模拟延迟服务端响应。
    let complete!: (value: KnowledgeFilePreview) => void;
    vi.mocked(previewKnowledgeDisplay).mockReturnValueOnce(new Promise((resolve) => { complete = resolve; }));
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeFileConfirmationDialog target={{ kind: "display", libraryId: "lib" }} ignoredFiles={[]} onConfirm={confirm} onClose={close} onRulesChanged={refresh} /></I18nProvider>));
    expect(document.body.textContent).toContain("正在检查文件清单");
    expect(button("关闭，不执行").disabled).toBe(false);
    expect(button("确认清单并生成").disabled).toBe(true);
    // The actual request receives the dialog-owned abort signal.
    // 实际请求接收弹窗持有的中止信号。
    const signal = vi.mocked(previewKnowledgeDisplay).mock.calls[0][1]!;
    await act(async () => button("关闭，不执行").click());
    expect(signal.aborted).toBe(true);
    expect(close).toHaveBeenCalledOnce();
    await act(async () => complete({ fingerprint: "too-late", entries: [row("late.md")] }));
    expect(document.querySelector("tbody tr")).toBeNull();
    expect(confirm).not.toHaveBeenCalled();
    expect(updateKnowledgeIgnoredFiles).not.toHaveBeenCalled();
  });

  // A missing response gets a visible timeout and a fresh retry; stale results cannot win the race.
  // 无响应会得到可见超时及全新重试，过期结果不能覆盖重试。
  it("times out a stalled scan and accepts only the new snapshot after retry", async () => {
    vi.useFakeTimers();
    // The first server response arrives only after the replacement scan completes.
    // 第一次服务端响应在替代扫描完成后才到达。
    let complete!: (value: KnowledgeFilePreview) => void;
    vi.mocked(previewKnowledgeDisplay)
      .mockReturnValueOnce(new Promise((resolve) => { complete = resolve; }))
      .mockResolvedValueOnce({ fingerprint: "fresh", entries: [row("fresh.md")] });
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeFileConfirmationDialog target={{ kind: "display", libraryId: "lib" }} ignoredFiles={[]} onConfirm={confirm} onClose={close} onRulesChanged={refresh} /></I18nProvider>));
    await act(async () => vi.advanceTimersByTimeAsync(30_000));
    expect(document.body.textContent).toContain("文件清单检查超过 30 秒");
    expect(vi.mocked(previewKnowledgeDisplay).mock.calls[0][1]!.aborted).toBe(true);
    expect(button("关闭，不执行").disabled).toBe(false);
    expect(button("重新扫描").disabled).toBe(false);
    expect(button("确认清单并生成").disabled).toBe(true);
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>(".knowledge-message-dialog button")].find((button) => button.textContent === "关闭")!.click());
    await act(async () => button("重新扫描").click());
    await act(async () => complete({ fingerprint: "old", entries: [row("old.md")] }));
    await act(async () => button("确认清单并生成").click());
    expect(confirm).toHaveBeenCalledWith({ fingerprint: "fresh", selectedKeys: ["fresh.md"] });
  });

  // Persistent writes keep their stricter dismissal guard; the following read-only rescan does not.
  // 持久写入保持严格关闭保护，随后的只读重新扫描不锁住关闭。
  it("blocks closing only during an ignore write, not its follow-up scan", async () => {
    // A controlled write distinguishes persistence from inspection.
    // 受控写入区分持久化与检查。
    let saved!: (value: KnowledgeIgnoredFile[]) => void;
    vi.mocked(previewKnowledgeDisplay).mockResolvedValueOnce({ fingerprint: "first", entries: [row("a.md")] })
      .mockReturnValueOnce(new Promise(() => {}));
    vi.mocked(updateKnowledgeIgnoredFiles).mockReturnValueOnce(new Promise((resolve) => { saved = resolve; }));
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeFileConfirmationDialog target={{ kind: "display", libraryId: "lib" }} ignoredFiles={[]} onConfirm={confirm} onClose={close} onRulesChanged={refresh} /></I18nProvider>));
    await act(async () => button("加入忽略").click());
    expect(document.body.textContent).toContain("正在保存忽略规则");
    expect(button("关闭，不执行").disabled).toBe(true);
    await act(async () => saved([{ sourceRoot: "D:/docs", relativePath: "a.md" }]));
    expect(document.body.textContent).toContain("正在检查文件清单");
    expect(button("关闭，不执行").disabled).toBe(false);
    await act(async () => button("关闭，不执行").click());
    expect(close).toHaveBeenCalledOnce();
    expect(confirm).not.toHaveBeenCalled();
  });

  it("does no work before confirmation and sends only checked eligible files", async () => {
    vi.mocked(previewKnowledgeImport).mockResolvedValue({ fingerprint: "exact-snapshot", entries: [row("a.md"), row("b.md"), row("large.md", "oversized"), row("done.md", "completed"), row("note.txt", "unsupported"), row("ignored.md", "ignored"), row("bad.md", "unreadable")] });
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeFileConfirmationDialog target={{ kind: "import", libraryId: "lib", sourceRoot: "D:/docs", sourceId: null }} ignoredFiles={[]} onConfirm={confirm} onClose={close} onRulesChanged={refresh} /></I18nProvider>));
    expect(confirm).not.toHaveBeenCalled();
    expect(updateKnowledgeIgnoredFiles).not.toHaveBeenCalled();
    expect(document.querySelectorAll('input[type="checkbox"]:disabled')).toHaveLength(5);
    expect(document.querySelectorAll('input[type="checkbox"]:checked')).toHaveLength(2);
    await act(async () => document.querySelector<HTMLInputElement>('input[aria-label="b.md"]')!.click());
    await act(async () => button("确认清单并导入").click());
    expect(confirm).toHaveBeenCalledWith({ fingerprint: "exact-snapshot", selectedKeys: ["a.md"] });
  });

  it("persists ignores, rescans eligibility and retains deliberate unchecks", async () => {
    vi.mocked(previewKnowledgeDisplay).mockResolvedValueOnce({ fingerprint: "before", entries: [row("a.md"), row("b.md")] })
      .mockResolvedValueOnce({ fingerprint: "ignored", entries: [row("a.md"), row("b.md", "ignored")] })
      .mockResolvedValueOnce({ fingerprint: "unignored", entries: [row("a.md"), row("b.md", "oversized")] });
    vi.mocked(updateKnowledgeIgnoredFiles).mockResolvedValueOnce([{ sourceRoot: "D:/docs", relativePath: "b.md" }]).mockResolvedValueOnce([]);
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeFileConfirmationDialog target={{ kind: "display", libraryId: "lib" }} ignoredFiles={[]} onConfirm={confirm} onClose={close} onRulesChanged={refresh} /></I18nProvider>));
    await act(async () => document.querySelector<HTMLInputElement>('input[aria-label="a.md"]')!.click());
    await act(async () => document.querySelector<HTMLInputElement>('input[aria-label="b.md"]')!.closest("tr")!.querySelector<HTMLButtonElement>("button")!.click());
    expect(updateKnowledgeIgnoredFiles).toHaveBeenCalledWith("lib", [expect.objectContaining({ sourceRoot: "D:/docs", relativePath: "b.md" })], true);
    expect(document.querySelector<HTMLInputElement>('input[aria-label="a.md"]')!.checked).toBe(false);
    expect(document.querySelector<HTMLInputElement>('input[aria-label="b.md"]')!.disabled).toBe(true);
    await act(async () => button("取消忽略").click());
    expect(document.querySelector<HTMLInputElement>('input[aria-label="b.md"]')!.disabled).toBe(true);
    expect(document.body.textContent).toContain("不可选：超过 64 KiB");
    expect(confirm).not.toHaveBeenCalled();
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("manages missing ignored files without scanning or starting a generation", async () => {
    vi.mocked(updateKnowledgeIgnoredFiles).mockResolvedValue([]);
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeFileConfirmationDialog target={{ kind: "ignored", libraryId: "lib" }} ignoredFiles={[{ sourceRoot: "D:/removed", relativePath: "lost.md" }, { sourceRoot: "D:/elsewhere", relativePath: "lost.md" }]} onConfirm={confirm} onClose={close} onRulesChanged={refresh} /></I18nProvider>));
    expect(previewKnowledgeDisplay).not.toHaveBeenCalled();
    expect(previewKnowledgeImport).not.toHaveBeenCalled();
    await act(async () => button("选中当前筛选的忽略项").click());
    await act(async () => button("批量取消忽略").click());
    expect(updateKnowledgeIgnoredFiles).toHaveBeenCalledWith("lib", [{ sourceRoot: "D:/removed", relativePath: "lost.md" }, { sourceRoot: "D:/elsewhere", relativePath: "lost.md" }], false);
    expect(document.body.textContent).toContain("没有匹配文件");
    expect(confirm).not.toHaveBeenCalled();
  });

  it("never confirms a stale snapshot after a failed rescan and closing does no work", async () => {
    vi.mocked(previewKnowledgeDisplay).mockResolvedValueOnce({ fingerprint: "old", entries: [row("a.md")] }).mockRejectedValueOnce(new Error("scan failed"));
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeFileConfirmationDialog target={{ kind: "display", libraryId: "lib" }} ignoredFiles={[]} onConfirm={confirm} onClose={close} onRulesChanged={refresh} /></I18nProvider>));
    await act(async () => button("重新扫描").click());
    expect(button("确认清单并生成").disabled).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>(".knowledge-message-dialog button")].find((button) => button.textContent === "关闭")!.click());
    expect(close).not.toHaveBeenCalled();
    await act(async () => button("关闭，不执行").click());
    expect(close).toHaveBeenCalledOnce();
    expect(confirm).not.toHaveBeenCalled();
  });

  it("paginates the rendered list without truncating the confirmed selection", async () => {
    vi.mocked(previewKnowledgeDisplay).mockResolvedValue({ fingerprint: "large-list", entries: Array.from({ length: 205 }, (_, index) => row(`${index}.md`)) });
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><KnowledgeFileConfirmationDialog target={{ kind: "display", libraryId: "lib" }} ignoredFiles={[]} onConfirm={confirm} onClose={close} onRulesChanged={refresh} /></I18nProvider>));
    expect(document.querySelectorAll("tbody tr")).toHaveLength(100);
    await act(async () => button("下一页").click());
    await act(async () => button("下一页").click());
    expect(document.querySelectorAll("tbody tr")).toHaveLength(5);
    await act(async () => button("确认清单并生成").click());
    expect(confirm.mock.calls[0][0].selectedKeys).toHaveLength(205);
  });
});
