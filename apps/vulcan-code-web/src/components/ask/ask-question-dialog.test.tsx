// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { PendingQuestionSnapshot } from "@/types";
import { AskQuestionDialog } from "./ask-question-dialog";

// host and root isolate the actual question surface and its focus lifecycle.
// host 和 root 隔离真实问题面板及其焦点生命周期。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// request builds a typed two-choice prompt; id separates retired requests in lifecycle checks.
// request 构建有类型的双选项问题；id 用于生命周期检查中区分已离开的请求。
// Returns one synthetic request without invoking a runtime or provider.
// 返回不调用运行时或供应商的合成请求。
function request(id = "ask-1", multiple = false): PendingQuestionSnapshot {
  return { id, sessionId: "session-1", sequence: 1, questions: [{ question: "选择方向", header: "方向", multiple,
    options: [{ label: "甲", description: "甲方向说明" }, { label: "乙", description: "乙方向说明" }] }] };
}

// mount renders the production component and callbacks; returns after React commits.
// mount 渲染生产组件及回调；在 React 提交后返回。
async function mount(snapshot: PendingQuestionSnapshot, submit: (id: string, answers: string[][]) => Promise<void>, reject = async (_id: string) => {}) {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><AskQuestionDialog request={snapshot} onSubmit={submit} onReject={reject} /></I18nProvider>));
}

// button finds a visible action by its text; missing controls fail the test immediately.
// button 按文本查找可见操作；控件缺失时立即令测试失败。
function button(text: string): HTMLButtonElement {
  const match = Array.from(host.querySelectorAll("button")).find((element) => element.textContent?.includes(text));
  if (!match) throw new Error(`Missing action: ${text}`);
  return match;
}

// key dispatches a real bubbling keyboard event and returns its prevented state.
// key 发出真实冒泡键盘事件并返回是否被阻止。
async function key(element: Element, value: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key: value, bubbles: true, cancelable: true, ...options });
  await act(async () => { element.dispatchEvent(event); });
  return event.defaultPrevented;
}

it("validates required answers, follows focused choices and ignores input-method confirmation", async () => {
  const submit = vi.fn(async () => {});
  await mount(request(), submit);
  const panel = host.querySelector('[role="dialog"]')!;
  expect(panel.getAttribute("aria-modal")).toBe("false");
  await key(panel, "Enter");
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("请先选择");
  await act(async () => button("乙方向说明").focus());
  await key(button("乙方向说明"), " ");
  expect(button("乙方向说明").getAttribute("aria-pressed")).toBe("true");
  expect(await key(panel, "Enter", { isComposing: true })).toBe(false);
  expect(host.textContent).not.toContain("确认提交");
  await key(panel, "Enter");
  expect(host.textContent).toContain("确认提交");
  await key(panel, "Enter");
  expect(submit).toHaveBeenCalledExactlyOnceWith("ask-1", [["乙"]]);
  expect(button("处理中").disabled).toBe(true);
});

it("blocks synchronous duplicate replies, retains errors for retry and isolates replacement requests", async () => {
  let fail: (error: Error) => void = () => {};
  const submit = vi.fn(() => new Promise<void>((_resolve, reject) => { fail = reject; }));
  await mount(request(), submit);
  await act(async () => button("甲方向说明").click());
  await act(async () => button("提交").click());
  await act(async () => { button("确认提交").click(); button("确认提交").click(); });
  expect(submit).toHaveBeenCalledTimes(1);
  await act(async () => fail(new Error("合成提交失败")));
  expect(host.querySelector('[role="alert"]')?.textContent).toBe("合成提交失败");
  expect(button("确认提交").disabled).toBe(false);
  await act(async () => button("确认提交").click());
  expect(submit).toHaveBeenCalledTimes(2);
  await mount(request("ask-2"), submit);
  await act(async () => fail(new Error("旧请求迟到失败")));
  expect(host.textContent).not.toContain("旧请求迟到失败");
  expect(button("提交").disabled).toBe(false);
  expect(button("甲方向说明").getAttribute("aria-pressed")).toBe("false");
});

it("keeps multiple choices and custom text, and requires explicit rejection confirmation", async () => {
  const submit = vi.fn(async () => {});
  const reject = vi.fn(async () => { throw new Error("合成终止失败"); });
  await mount(request("multiple", true), submit, reject);
  await act(async () => { button("甲方向说明").click(); });
  await act(async () => { button("乙方向说明").click(); });
  await act(async () => button("其他").click());
  const textarea = host.querySelector("textarea")!;
  const setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  await act(async () => { setValue.call(textarea, "补充文本"); textarea.dispatchEvent(new Event("input", { bubbles: true })); });
  expect(await key(textarea, "Enter", { shiftKey: true })).toBe(false);
  await key(textarea, "Escape");
  expect(reject).not.toHaveBeenCalled();
  expect(host.textContent).toContain("确认终止");
  await key(host.querySelector('[role="dialog"]')!, "Escape");
  await key(textarea, "Enter");
  await key(host.querySelector('[role="dialog"]')!, "Enter");
  expect(submit).toHaveBeenCalledExactlyOnceWith("multiple", [["甲", "乙", "补充文本"]]);
  await mount(request("reject"), submit, reject);
  await key(host.querySelector('[role="dialog"]')!, "Escape");
  await key(host.querySelector('[role="dialog"]')!, "Enter");
  expect(reject).toHaveBeenCalledExactlyOnceWith("reject");
  expect(host.querySelector('[role="alert"]')?.textContent).toBe("合成终止失败");
});

it("preserves text-only answers across question navigation and same-request projections", async () => {
  // snapshot declares two text-only questions under one stable request identity.
  // snapshot 在一个稳定请求标识下声明两道纯文本问题。
  const snapshot: PendingQuestionSnapshot = { ...request("text-only"), questions: [
    { question: "项目名称", header: "名称", multiple: false, options: [] },
    { question: "验收要求", header: "验收", multiple: false, options: [] },
  ] };
  const submit = vi.fn(async () => {});
  await mount(snapshot, submit);
  const firstInput = host.querySelector("textarea")!;
  const setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  await act(async () => { setValue.call(firstInput, "资料整理"); firstInput.dispatchEvent(new Event("input", { bubbles: true })); firstInput.focus(); });
  await mount({ ...snapshot, questions: snapshot.questions.map((question) => ({ ...question })) }, submit);
  expect(document.activeElement).toBe(firstInput);
  expect(firstInput.value).toBe("资料整理");
  await key(firstInput, "Enter");
  expect(host.textContent).toContain("2 / 2");
  const secondInput = host.querySelector("textarea")!;
  await act(async () => { setValue.call(secondInput, "可检索并能追溯来源"); secondInput.dispatchEvent(new Event("input", { bubbles: true })); });
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="上一题"]')!.click());
  expect(host.querySelector("textarea")?.value).toBe("资料整理");
  await key(host.querySelector("textarea")!, "Enter");
  expect(host.querySelector("textarea")?.value).toBe("可检索并能追溯来源");
  await key(host.querySelector("textarea")!, "Enter");
  await key(host.querySelector('[role="dialog"]')!, "Enter");
  expect(submit).toHaveBeenCalledExactlyOnceWith("text-only", [["资料整理"], ["可检索并能追溯来源"]]);
});
