// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { KnowledgeOperationFeedback } from "./knowledge-feedback";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

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
    await act(async () => root.render(<KnowledgeOperationFeedback message={message} />));
    expect(document.querySelector('[role="dialog"] pre')?.textContent).toBe(message);
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "knowledge.feedback.copy")!.click());
    expect(copy).toHaveBeenCalledWith(message);
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "knowledge.importDialog.close")!.click());
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(host.querySelector('[role="alert"]')).not.toBeNull();
    await act(async () => host.querySelector<HTMLButtonElement>('[role="alert"] button')!.click());
    expect(document.querySelector('[role="dialog"] pre')?.textContent).toBe(message);
  });

  it("retains diagnostics and offers manual selection if clipboard access fails", async () => {
    copy.mockRejectedValue(new Error("clipboard denied"));
    await act(async () => root.render(<KnowledgeOperationFeedback message="完整原始错误" />));
    await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((button) => button.textContent === "knowledge.feedback.copy")!.click());
    expect(document.querySelector('[role="status"]')?.textContent).toContain("knowledge.feedback.copyFailed");
    expect(document.querySelector('[role="dialog"] pre')?.textContent).toBe("完整原始错误");
  });

});
