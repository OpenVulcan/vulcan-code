// @vitest-environment jsdom
// Behaviour of the feedback card: only a successful draft offers review, and review opens the feedback window with the
// draft as an AI-draft seed; without a message center the card offers nothing.
// 反馈卡的行为：只有成功的草稿提供检查，检查以 AI 草稿种子打开反馈窗口；没有消息中心时卡片不提供任何操作。
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { FeedbackCenterContext, type FeedbackCenterController, type FeedbackSeed } from "@/components/features/feedback/feedback-center-context";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { readToolCall } from "../tool-call";
import FeedbackCard, { feedbackDraft } from "./feedback-card";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

const DRAFT = { body: "Steps to reproduce", kind: "bug", title: "Crash on start" };

// feedbackEvent builds a finished feedback call with the given host payload.
// feedbackEvent 构建一条带给定宿主载荷的已完成反馈调用。
function feedbackEvent(input: Record<string, unknown>, payload: Record<string, unknown>, isError = false): ToolCallEvent {
  return {
    api_call_id: 1,
    input,
    kind: "tool_call",
    output: { host_result: { kind: "feedback", payload }, is_error: isError, model_output: "result", tool_call_id: "call-1", tool_name: "feedback" },
    request_stream_id: "stream-1",
    started_at_ms: 0,
    status: "completed",
    tool_call_id: 1,
    tool_name: "feedback",
  } as unknown as ToolCallEvent;
}

const DRAFT_EVENT = feedbackEvent({ action: "draft", ...DRAFT }, { draft: DRAFT, message: "m", status: "awaiting_user" });

// render mounts the card collapsed, with or without a message center.
// render 以收起状态挂载卡片，可带或不带消息中心。
function render(event: ToolCallEvent, center: FeedbackCenterController | null): void {
  act(() => root.render(
    <I18nProvider initialLocale="en-US">
      <FeedbackCenterContext.Provider value={center}>
        <FeedbackCard call={readToolCall(event)} family="agent" onOpenChange={() => undefined} open={false} />
      </FeedbackCenterContext.Provider>
    </I18nProvider>,
  ));
}

describe("feedback card", () => {
  it("reads a draft only from a successful awaiting-user result", () => {
    expect(feedbackDraft(readToolCall(DRAFT_EVENT))).toEqual(DRAFT);
    expect(feedbackDraft(readToolCall(feedbackEvent({ action: "draft" }, { draft: DRAFT, status: "awaiting_user" }, true)))).toBeNull();
    expect(feedbackDraft(readToolCall(feedbackEvent({ action: "link_pr" }, { links: [], status: "linked" })))).toBeNull();
    expect(feedbackDraft(readToolCall(feedbackEvent({ action: "draft" }, { draft: { ...DRAFT, kind: "question" }, status: "awaiting_user" })))).toBeNull();
  });

  it("opens the feedback window with the draft as an AI-draft seed", () => {
    const openFeedback = vi.fn<(seed?: FeedbackSeed) => void>();
    render(DRAFT_EVENT, { openFeedback } as unknown as FeedbackCenterController);
    const review = host.querySelector<HTMLButtonElement>(".vk-tool-foot button");
    expect(review).not.toBeNull();
    act(() => review?.click());
    expect(openFeedback).toHaveBeenCalledWith({ ...DRAFT, source: "ai_draft" });
  });

  it("offers no review without a message center", () => {
    render(DRAFT_EVENT, null);
    expect(host.querySelector(".vk-tool-foot")).not.toBeNull();
    expect(host.querySelector(".vk-tool-foot button")).toBeNull();
  });
});
