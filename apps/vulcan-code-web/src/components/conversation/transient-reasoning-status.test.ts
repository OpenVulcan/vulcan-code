import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("transient reasoning status", () => {
  // Verifies summary nodes wrap naturally without Markdown parsing or marquee animation.
  // 验证摘要节点自然换行，且不依赖 Markdown 解析或跑马灯动画。
  it("renders provider summary parts as independent wrapped nodes", () => {
    // css is the authoritative stylesheet loaded by the conversation surface.
    // css 是会话界面加载的权威样式表。
    const css = readFileSync(
      new URL("../../styles/conversation.css", import.meta.url),
      "utf8",
    );
    // source owns the exact provider-node projection at the transcript tail.
    // source 持有转录尾部精确的供应商节点投影。
    const source = readFileSync(
      new URL("./conversation-blocks.tsx", import.meta.url),
      "utf8",
    );

    expect(css).toContain(".transient-reasoning-status-stack");
    expect(css).toContain(".tool-execution-row.transient-reasoning-status-line");
    expect(css).toContain("white-space: pre-wrap");
    expect(css).not.toContain("transient-reasoning-status-marquee");
    expect(source).toContain("props.status.summaries ?? []");
    expect(source).toContain("key={row.id}");
    expect(source).toContain('allowedElements={["p", "strong", "em", "br", "code"]}');
    expect(source).not.toContain("transientReasoningSummaryStage");
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
