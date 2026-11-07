// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { prepareFeedback, submitFeedback, type FeedbackPreview } from "@/service/feedback-service";
import { OpenVulcanAccountContext, type OpenVulcanAccountController } from "@/components/features/account/openvulcan-account-context";
import type { SystemProviderError } from "@/components/conversation/conversation-system-error-card";
import FeedbackDialog from "./feedback-dialog";
import { FeedbackCenterContext, type FeedbackCenterController, type FeedbackSeed } from "./feedback-center-context";

vi.mock("@/i18n", () => ({
  useI18n: () => ({ locale: "en-US", t: (key: string) => key }),
  formatMessage: (template: string) => template,
}));
vi.mock("@/service/feedback-service", () => ({ prepareFeedback: vi.fn(), submitFeedback: vi.fn() }));

let host: HTMLDivElement;
let root: Root;
let center: FeedbackCenterController;
let account: OpenVulcanAccountController;

// PREVIEW is a synthetic filtered request; only its id and digest matter to the flow.
// PREVIEW 是合成的过滤后请求；流程只关心其编号与摘要。
const PREVIEW: FeedbackPreview = {
  draftId: "draft-1", digest: "digest-1", payload: "{\n  \"title\": \"t\"\n}", byteLength: 20,
  redactionHits: { path: 2 }, logLineCount: 3, signedIn: true,
};

// SYSTEM_ERROR is a synthetic error-card failure; the report must carry its facts with the HTTP status as a number.
// SYSTEM_ERROR 是合成的错误卡失败；报告必须携带其事实，且 HTTP 状态为数字。
const SYSTEM_ERROR: SystemProviderError = {
  raw: "", provider: "openai", category: "rate_limited", stage: "response", dispatchOutcome: "sent",
  semanticOutputObserved: false, retryable: true, sideEffectObserved: false, terminal: true, httpStatus: "429",
  subcategory: null, upstreamCode: null, upstreamType: null, upstreamParameter: null, upstreamMessage: null,
  requestId: "req-1", entryId: null, modelId: "gpt-test", retryAfterMs: null, message: "Too many requests", structured: {},
};

// render mounts the dialog for one seed inside the account and message-center contexts.
// render 在账户与消息中心上下文中为一个初始内容挂载弹窗。
async function render(seed: FeedbackSeed) {
  center.feedbackSeed = seed;
  await act(async () => root.render(
    <OpenVulcanAccountContext.Provider value={account}>
      <FeedbackCenterContext.Provider value={center}><FeedbackDialog /></FeedbackCenterContext.Provider>
    </OpenVulcanAccountContext.Provider>,
  ));
}

// button finds an action by its catalog key; a missing key means the expected action may have been renamed.
// button 按目录键查找操作；找不到说明预期操作可能已改名。
function button(key: string): HTMLButtonElement {
  const match = Array.from(document.querySelectorAll<HTMLButtonElement>("button")).find((candidate) => candidate.textContent?.includes(key));
  if (!match) throw new Error(`Expected catalog action may have changed: ${key}`);
  return match;
}

// type sets a controlled field the way user input does.
// type 以用户输入的方式设置受控字段。
async function type(element: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  await act(async () => {
    Object.getOwnPropertyDescriptor(prototype, "value")?.set?.call(element, value);
    element.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.mocked(prepareFeedback).mockReset().mockResolvedValue(PREVIEW);
  vi.mocked(submitFeedback).mockReset().mockResolvedValue({ feedbackId: "fb_x", status: "received", createdAt: "2026-11-07T00:00:00Z" });
  center = {
    feedbackSeed: null, openFeedback: vi.fn(), closeFeedback: vi.fn(), messageCenterTab: null, openMessageCenter: vi.fn(),
    closeMessageCenter: vi.fn(), summary: null, announcements: null, setSummary: vi.fn(),
    runtime: { current: { stop: vi.fn(), refresh: vi.fn().mockResolvedValue(undefined), markSeen: vi.fn(), dismissBanner: vi.fn() } },
  };
  account = {
    snapshot: { connected: true, connectionId: "conn-1", displayName: "tester", region: "global", message: null },
    initialized: true, open: false, busy: false, waiting: false, loginRegion: null, error: null,
    setOpen: vi.fn(), cancel: vi.fn(), login: vi.fn(), logout: vi.fn(), refresh: vi.fn(),
  };
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// Sending is possible only from a preview, and sends exactly the previewed draft id and digest.
// 只能从预览发送，且发送的正是预览的草稿编号与摘要。
it("previews before sending and sends the previewed digest", async () => {
  await render({ source: "manual" });
  expect(button("feedback.preview").disabled).toBe(true);
  await type(document.querySelector<HTMLInputElement>("#feedback-title")!, "Title");
  await type(document.querySelector<HTMLTextAreaElement>("#feedback-body")!, "Body");
  await act(async () => button("feedback.preview").click());
  expect(prepareFeedback).toHaveBeenCalledWith(expect.objectContaining({
    draftId: null, kind: "suggestion", source: "manual", title: "Title", body: "Body", includeLog: true, allowContact: false, error: null,
  }));
  expect(document.querySelector("pre")?.textContent).toBe(PREVIEW.payload);
  await act(async () => button("feedback.send").click());
  expect(submitFeedback).toHaveBeenCalledWith("draft-1", "digest-1");
  expect(center.runtime.current?.refresh).toHaveBeenCalled();
});

// Going back to edit discards the preview; the next preview reuses the draft id as the idempotency key.
// 返回编辑会丢弃预览；下一次预览复用草稿编号作为幂等键。
it("discards the preview on edit and reuses the draft id", async () => {
  await render({ source: "error_card", systemError: SYSTEM_ERROR });
  await type(document.querySelector<HTMLTextAreaElement>("#feedback-body")!, "Steps");
  await act(async () => button("feedback.preview").click());
  expect(prepareFeedback).toHaveBeenLastCalledWith(expect.objectContaining({
    source: "error_card", kind: "bug", title: "feedback.errorTitle",
    error: expect.objectContaining({ category: "rate_limited", httpStatus: 429, providerId: "openai", requestId: "req-1" }),
  }));
  await act(async () => button("feedback.backToEdit").click());
  expect(document.querySelector("pre")).toBeNull();
  expect(submitFeedback).not.toHaveBeenCalled();
  await act(async () => button("feedback.preview").click());
  expect(prepareFeedback).toHaveBeenLastCalledWith(expect.objectContaining({ draftId: "draft-1" }));
});

// A signed-out user can preview but cannot send, and is offered the sign-in entry.
// 未登录用户可以预览但不能发送，并会看到登录入口。
it("requires a signed-in account to send", async () => {
  account.snapshot = { connected: false, connectionId: null, displayName: null, region: null, message: null };
  await render({ source: "manual", title: "Title", body: "Body" });
  await act(async () => button("feedback.preview").click());
  expect(button("feedback.send").disabled).toBe(true);
  await act(async () => button("openvulcan.account.login").click());
  expect(account.setOpen).toHaveBeenCalledWith(true);
});
