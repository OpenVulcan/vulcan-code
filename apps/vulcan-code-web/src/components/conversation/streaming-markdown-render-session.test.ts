import { describe, expect, it } from "vitest";
import {
  STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES,
  applyStreamingMarkdownDeltas,
  createStreamingMarkdownRenderState,
  streamingMarkdownRevisionRequiresRebase,
  streamingMarkdownRenderStateBody,
} from "./streaming-markdown-render-session";
import type {
  ConversationLiveStreamBodySnapshot,
  ConversationLiveStreamRevision,
} from "@/state/conversation-live-stream-store";

// bodySnapshot builds one atomic Store read for a mounted render session.
// bodySnapshot 构建挂载渲染会话使用的一次原子 Store 读取。
function bodySnapshot(body: string, generation = 1): ConversationLiveStreamBodySnapshot {
  return {
    key: "conversation\u0000message",
    conversationId: "conversation",
    messageId: "message",
    recordKind: "thinking-output",
    lifecycle: "active",
    baseGeneration: generation,
    lastGeneration: generation,
    version: generation,
    utf16Length: body.length,
    utf8ByteLength: new TextEncoder().encode(body).byteLength,
    body,
  };
}

// revision builds one immutable Store notification snapshot.
// revision 构建一份不可变 Store 通知快照。
function revision(
  generation: number,
  lifecycle: "active" | "sealed" = "active",
): ConversationLiveStreamRevision {
  return {
    ...bodySnapshot("", generation),
    lifecycle,
    baseGeneration: 1,
    lastGeneration: generation,
    version: generation,
  };
}

describe("streaming Markdown render session", () => {
  it("creates one cold baseline instead of replaying historical chunks", () => {
    // body contains two complete paragraphs and one incomplete live paragraph.
    // body 包含两个完整段落与一个未完成实时段落。
    const body = "first\n\nsecond\n\ncurrent";
    // state captures all safe history in one baseline string.
    // state 将全部安全历史捕获为一个基线字符串。
    const state = createStreamingMarkdownRenderState(bodySnapshot(body));

    expect(state.baseline).toBe("first\n\nsecond\n\n");
    expect(state.committedSegments).toHaveLength(0);
    expect(state.markdownTail).toBe("current");
  });

  it("commits one immutable segment only after 2KB and a safe boundary", () => {
    // firstBlock exceeds the normal threshold and ends at a top-level blank line.
    // firstBlock 超过正常阈值并在顶层空行结束。
    const firstBlock = `${"a".repeat(2200)}\n\n`;
    // initial state begins with one incomplete short paragraph.
    // initial state 从一个未完成短段落开始。
    const initial = createStreamingMarkdownRenderState(bodySnapshot("start"));
    // updated consumes one safe large block and one residual live suffix.
    // updated 消费一个安全大块与一个剩余实时后缀。
    const updated = applyStreamingMarkdownDeltas(
      initial,
      [{ generation: 2, text: `\n\n${firstBlock}tail` }],
      revision(2),
    );

    expect(updated.committedSegments).toHaveLength(1);
    expect(updated.committedSegments[0]?.content.endsWith("\n\n")).toBe(true);
    expect(updated.markdownTail).toBe("tail");
    expect(streamingMarkdownRenderStateBody(updated)).toBe(`start\n\n${firstBlock}tail`);
  });

  it("uses raw preview for a huge unclosed fence and renders it once when sealed", () => {
    // openCode exceeds the hard Markdown tail limit without a closing fence.
    // openCode 在没有关闭围栏的情况下超过 Markdown 尾部硬上限。
    const openCode = `\n\n\`\`\`text\n${"x".repeat(STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES + 100)}`;
    // initial begins with one safe baseline paragraph.
    // initial 从一个安全基线段落开始。
    const initial = createStreamingMarkdownRenderState(bodySnapshot("intro"));
    // active keeps the huge open structure out of ReactMarkdown.
    // active 防止大型未闭合结构进入 ReactMarkdown。
    const active = applyStreamingMarkdownDeltas(
      initial,
      [{ generation: 2, text: openCode }],
      revision(2),
    );
    // sealed applies EOF semantics without changing earlier baseline content.
    // sealed 应用 EOF 语义，同时不改变更早基线内容。
    const sealed = applyStreamingMarkdownDeltas(active, [], revision(3, "sealed"));

    expect(active.rawPreview).toBe(true);
    expect(active.markdownTail).toBe("");
    expect(sealed.rawPreview).toBe(false);
    expect(sealed.markdownTail.length).toBeGreaterThan(STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES);
    expect(sealed.baseline).toBe(active.baseline);
  });

  it("does not commit an unresolved reference across independent Markdown documents", () => {
    // unresolved exceeds 2KB but requires a definition that has not arrived.
    // unresolved 超过 2KB，但依赖尚未到达的定义。
    const unresolved = `${"body ".repeat(450)}[link][later]\n\n`;
    // state must retain the candidate as an active tail.
    // state 必须把候选段保留为活动尾部。
    const state = applyStreamingMarkdownDeltas(
      createStreamingMarkdownRenderState(bodySnapshot("")),
      [{ generation: 2, text: unresolved }],
      revision(2),
    );

    expect(state.committedSegments).toHaveLength(0);
    expect(state.segmentationDisabled).toBe(true);
    expect(streamingMarkdownRenderStateBody(state)).toBe(unresolved);
  });

  it("renders cold safe history as one baseline even when it contains reusable references", () => {
    const body = `[target]: /url\n\n${"body ".repeat(450)}[link][target]\n\n`;
    const state = createStreamingMarkdownRenderState(bodySnapshot(body));

    expect(state.baseline).toBe(body);
    expect(state.segmentationDisabled).toBe(false);
    expect(streamingMarkdownRenderStateBody(state)).toBe(body);
  });

  it("keeps Markdown enabled for a long cold body containing ordinary bracket text", () => {
    const body = `${"paragraph [state] text ".repeat(900)}\n\n`;
    const state = createStreamingMarkdownRenderState(bodySnapshot(body));

    expect(state.baseline).toBe(body);
    expect(state.markdownTail).toBe("");
    expect(state.rawPreview).toBe(false);
  });

  it("does not rescan an ever-growing tail after global reference semantics disable segmentation", () => {
    const unresolved = `${"body ".repeat(450)}[link][later]\n\n`;
    let state = applyStreamingMarkdownDeltas(
      createStreamingMarkdownRenderState(bodySnapshot("")),
      [{ generation: 2, text: unresolved }],
      revision(2),
    );
    for (let generation = 3; generation <= 40; generation += 1) {
      state = applyStreamingMarkdownDeltas(
        state,
        [{ generation, text: `${"x".repeat(1022)}\n\n` }],
        revision(generation),
      );
    }

    expect(state.segmentationDisabled).toBe(true);
    expect(state.committedSegments).toHaveLength(0);
    expect(state.rawPreview).toBe(true);
  });

  it("measures the 2KB threshold in UTF-8 bytes for Chinese and emoji", () => {
    const unicodeBlock = `${"界".repeat(700)}${"🙂".repeat(10)}\n\n`;
    const state = applyStreamingMarkdownDeltas(
      createStreamingMarkdownRenderState(bodySnapshot("")),
      [{ generation: 2, text: unicodeBlock }],
      revision(2),
    );

    expect(unicodeBlock.length).toBeLessThan(2048);
    expect(state.committedSegments).toHaveLength(1);
    expect(state.committedSegments[0]?.content).toBe(unicodeBlock);
  });

  it("keeps 128KB of stable streaming history immutable across 1KB updates", () => {
    const oneKilobyteBlock = `${"x".repeat(1022)}\n\n`;
    let state = createStreamingMarkdownRenderState(bodySnapshot(""));
    let firstCommittedSegment = null as (typeof state.committedSegments)[number] | null;
    for (let index = 0; index < 128; index += 1) {
      state = applyStreamingMarkdownDeltas(
        state,
        [{ generation: index + 2, text: oneKilobyteBlock }],
        revision(index + 2),
      );
      firstCommittedSegment ??= state.committedSegments[0] ?? null;
      if (firstCommittedSegment) {
        expect(state.committedSegments[0]).toBe(firstCommittedSegment);
      }
      expect(state.markdownTail.length).toBeLessThanOrEqual(1024);
    }

    expect(state.committedSegments).toHaveLength(64);
    expect(streamingMarkdownRenderStateBody(state)).toBe(oneKilobyteBlock.repeat(128));
  });

  it("coalesces transport fragments into one raw preview part per render batch", () => {
    const state = applyStreamingMarkdownDeltas(
      createStreamingMarkdownRenderState(bodySnapshot("```text\n")),
      Array.from({ length: 100 }, (_, index) => ({
        generation: index + 2,
        text: "x",
      })),
      revision(101),
    );

    expect(state.tailParts).toHaveLength(1);
    expect(streamingMarkdownRenderStateBody(state)).toBe(`\`\`\`text\n${"x".repeat(100)}`);
  });

  it("requires a full read when an authoritative snapshot rebases the current generation", () => {
    const state = createStreamingMarkdownRenderState(bodySnapshot("local body", 7));
    const correctedRevision = {
      ...revision(7),
      baseGeneration: 7,
      version: 8,
    };

    expect(streamingMarkdownRevisionRequiresRebase(state, correctedRevision)).toBe(true);
  });
});
