// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { PendingApprovalSnapshot, PendingQuestionSnapshot } from "@/types";
import { AskQuestionDialog } from "./ask-question-dialog";

// host and root isolate the real Ask surface rendering one approval.
// host 和 root 隔离渲染单条审批的真实 Ask 面板。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// approvalRequest builds a synthetic approval with the canonical labels carried by the snapshot.
// approvalRequest 构建携带快照规范标签的合成审批。
function approvalRequest(id: string, approval: Partial<PendingApprovalSnapshot> = {}): PendingQuestionSnapshot {
  return {
    id, sessionId: "session-1", sequence: 1,
    questions: [{ question: "fallback", header: "Permission", multiple: false,
      options: [{ label: "allow-label", description: "" }, { label: "deny-label", description: "" }] }],
    approval: { toolName: "shell_command", permissionMode: "workspace_write",
      subject: { kind: "command", text: "rm -rf build" }, highRisk: "recursive_delete",
      allowLabel: "allow-label", denyLabel: "deny-label", ...approval },
  };
}

// mount renders the production Ask entry point; returns after React commits.
// mount 渲染生产 Ask 入口；在 React 提交后返回。
async function mount(snapshot: PendingQuestionSnapshot, submit = vi.fn(async () => {}), reject = vi.fn(async (_id: string) => {})) {
  await act(async () => root.render(<I18nProvider initialLocale="zh-TW"><AskQuestionDialog request={snapshot} onSubmit={submit} onReject={reject} /></I18nProvider>));
  return { submit, reject };
}

// action finds an approval control by its stable action marker; a missing control fails immediately.
// action 按稳定操作标记查找审批控件；控件缺失时立即失败。
function action(name: string): HTMLButtonElement {
  const match = host.querySelector<HTMLButtonElement>(`[data-approval-action="${name}"]`);
  if (!match) throw new Error(`Missing approval action: ${name}`);
  return match;
}

// key dispatches a bubbling keyboard event on the approval surface.
// key 在审批面板上派发冒泡键盘事件。
async function key(value: string) {
  const panel = host.querySelector('[role="dialog"]')!;
  await act(async () => { panel.dispatchEvent(new KeyboardEvent("keydown", { key: value, bubbles: true, cancelable: true })); });
}

it("replies with the snapshot labels and never approves from the keyboard", async () => {
  const { submit, reject } = await mount(approvalRequest("approve"));
  expect(host.querySelector("[data-approval-request]")).not.toBeNull();
  expect(host.querySelector("textarea")).toBeNull();
  await key("Enter");
  expect(submit).not.toHaveBeenCalled();
  expect(reject).not.toHaveBeenCalled();
  await act(async () => { action("allow").click(); action("allow").click(); });
  expect(submit).toHaveBeenCalledExactlyOnceWith("approve", [["allow-label"]]);
  expect(action("deny").disabled).toBe(true);

  const denied = await mount(approvalRequest("deny"));
  await act(async () => action("deny").click());
  expect(denied.submit).toHaveBeenCalledExactlyOnceWith("deny", [["deny-label"]]);
});

it("stops only after an explicit confirmation and keeps failures retryable", async () => {
  const reject = vi.fn(async (_id: string) => { throw new Error("synthetic stop failure"); });
  const { submit } = await mount(approvalRequest("stop"), undefined, reject);
  await key("Escape");
  expect(host.querySelector('[data-approval-action="allow"]')).toBeNull();
  await key("Escape");
  expect(action("allow")).toBeDefined();
  await key("Escape");
  await key("Enter");
  expect(reject).toHaveBeenCalledExactlyOnceWith("stop");
  expect(submit).not.toHaveBeenCalled();
  expect(host.querySelector('[role="alert"]')?.textContent).toBe("synthetic stop failure");
  expect(action("confirm-stop").disabled).toBe(false);
});

it("shows the affected paths and falls back to the raw key for unknown risk categories", async () => {
  await mount(approvalRequest("paths", {
    subject: { kind: "file_write", paths: ["../outside/a.txt", "../outside/b.txt"] },
    highRisk: "future_category",
    permissionMode: "read_only",
  }));
  expect(host.querySelector("pre")?.textContent).toBe("../outside/a.txt\n../outside/b.txt");
  expect(host.querySelector("[data-high-risk]")?.textContent).toContain("future_category");
  expect(host.querySelector("[data-permission-mode]")?.getAttribute("data-permission-mode")).toBe("read_only");
  expect(host.textContent).not.toContain("{");

  await mount(approvalRequest("shell", { subject: { kind: "command", text: "" }, highRisk: null }));
  expect(host.querySelector("pre")).toBeNull();
  expect(host.querySelector("[data-high-risk]")).toBeNull();
});

it("shows the judge finding and keeps an unavailable judge's reason on hover", async () => {
  await mount(approvalRequest("judged", { judge: { kind: "scored", label: "block" } }));
  expect(host.querySelector("[data-judge]")?.getAttribute("data-judge")).toBe("block");

  await mount(approvalRequest("unjudged", { judge: { kind: "unavailable", reason: "not loaded" } }));
  expect(host.querySelector("[data-judge]")?.getAttribute("data-judge")).toBe("unavailable");
  expect(host.querySelector("[data-judge]")?.getAttribute("title")).toBe("not loaded");

  await mount(approvalRequest("plain"));
  expect(host.querySelector("[data-judge]")).toBeNull();
});
