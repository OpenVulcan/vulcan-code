import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  transientReasoningStatusOverflows,
  transientReasoningSummaryStage,
} from "./conversation-blocks";

describe("transient reasoning status", () => {
  // Verifies the layout tolerance avoids oscillation on sub-pixel width differences.
  // 验证布局容差可避免亚像素宽度差异导致状态反复切换。
  it("activates marquee only after content exceeds the measured viewport", () => {
    expect(transientReasoningStatusOverflows(400, 400)).toBe(false);
    expect(transientReasoningStatusOverflows(401, 400)).toBe(false);
    expect(transientReasoningStatusOverflows(402, 400)).toBe(true);
  });

  // Verifies overflow animation remains scoped and preserves reduced-motion accessibility.
  // 验证溢出动画保持作用域隔离，并保留减少动态效果的无障碍行为。
  it("keeps the timeline-tail status single-line and motion-aware", () => {
    // css is the authoritative stylesheet loaded by the conversation surface.
    // css 是会话界面加载的权威样式表。
    const css = readFileSync(
      new URL("../../styles/conversation.css", import.meta.url),
      "utf8",
    );

    expect(css).toContain(
      ".tool-execution-row.transient-reasoning-status-line",
    );
    expect(css).toContain(".transient-reasoning-status-viewport");
    expect(css).toContain("white-space: nowrap");
    expect(css).toContain("data-overflowing=\"true\"");
    expect(css).toContain("transient-reasoning-status-marquee");
    expect(css).toContain("prefers-reduced-motion: reduce");
    expect(css).toContain(".transient-reasoning-status-detail");
    expect(css).toContain("white-space: pre-wrap");
  });

  // Verifies each complete bold heading replaces the previous summary phase and body.
  // 验证每个完整粗体标题都会替换之前的摘要阶段与正文。
  it("shows only the latest complete summary stage", () => {
    expect(transientReasoningSummaryStage(
      "**Analyzing code** First phase body.**Inspecting tests** Current phase body.",
    )).toEqual({
      title: "Inspecting tests",
      detail: "Current phase body.",
    });
    expect(transientReasoningSummaryStage(
      "**Analyzing code** First phase body.**Inspecting",
    )).toEqual({
      title: "Analyzing code",
      detail: "First phase body.",
    });
    expect(transientReasoningSummaryStage("**Inspecting tests**")).toEqual({
      title: "Inspecting tests",
      detail: null,
    });
  });

  // Verifies legacy plain summaries and split opening headings remain deterministic.
  // 验证旧式纯文本摘要与被分片的起始标题保持确定行为。
  it("preserves plain summaries and hides incomplete opening headings", () => {
    expect(transientReasoningSummaryStage("Inspect the workspace")).toEqual({
      title: "Inspect the workspace",
      detail: null,
    });
    expect(transientReasoningSummaryStage("**Inspecting")).toEqual({
      title: null,
      detail: null,
    });
  });

  // Verifies the read-only child transcript subscribes to its own ephemeral reasoning channel.
  // 验证只读子任务转录订阅自身的瞬态思考通道。
  it("projects transient reasoning into the active subagent transcript", () => {
    // source is the authoritative right-side subagent transcript component.
    // source 是右侧子智能体转录的权威组件源码。
    const source = readFileSync(
      new URL("../workspace/WorkspaceContextPanel.tsx", import.meta.url),
      "utf8",
    );

    expect(source).toContain("useConversationTransientReasoningStatus(taskId)");
    expect(source).toContain("transientReasoningStatus={transientReasoningStatus ? (");
    expect(source).toContain("<TransientReasoningStatusLine status={transientReasoningStatus} />");
    expect(source).toContain("transientReasoningStatusVisible");
  });
});
