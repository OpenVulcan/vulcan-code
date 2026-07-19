import { describe, expect, it } from "vitest";
import {
  appendStreamingMarkdownBoundary,
  createStreamingMarkdownBoundaryState,
  finishStreamingMarkdownBoundary,
  streamingMarkdownSegmentIsSelfContained,
} from "./streaming-markdown-boundary";

describe("streaming Markdown safe boundaries", () => {
  it("records a paragraph boundary only after its blank line", () => {
    // content contains one complete paragraph followed by a blank line and an incomplete paragraph.
    // content 包含一个带空行的完整段落，以及一个未完成段落。
    const content = "first paragraph\n\nsecond";
    // state scans complete lines while retaining the incomplete final line.
    // state 扫描完整行，同时保留不完整末行。
    const state = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      content,
    );

    expect(state.lastSafeOffset).toBe("first paragraph\n\n".length);
    expect(state.pendingLineLength).toBe("second".length);
  });

  it("never cuts an open fenced code block and accepts its exact close", () => {
    // firstChunk opens a four-backtick fence after one safe paragraph.
    // firstChunk 在一个安全段落后打开四反引号围栏。
    const firstChunk = "intro\n\n````ts\nconst value = `x`;\n";
    // openState must retain the boundary before the fence.
    // openState 必须保留围栏之前的边界。
    const openState = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      firstChunk,
    );
    // shortClose uses too few markers and must remain code content.
    // shortClose 使用过少围栏字符，必须继续作为代码内容。
    const shortClose = appendStreamingMarkdownBoundary(openState, "```\n");
    // closedState receives the exact four-marker close.
    // closedState 接收精确的四字符关闭围栏。
    const closedState = appendStreamingMarkdownBoundary(shortClose, "````\n");

    expect(openState.openStructure).toBe("fenced-code");
    expect(shortClose.lastSafeOffset).toBe("intro\n\n".length);
    expect(closedState.openStructure).toBeNull();
    expect(closedState.lastSafeOffset).toBe(firstChunk.length + "```\n````\n".length);
  });

  it("keeps a GFM table together until the first following non-table line", () => {
    // table ends without a blank line before the next paragraph.
    // table 在下一段落前结束，且中间没有空行。
    const table = "| A | B |\n| --- | --- |\n| 1 | 2 |\n";
    // state sees the first non-table line and seals immediately before it.
    // state 看到首条非表格行，并在其前方封口。
    const state = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      `${table}after table\n`,
    );

    expect(state.lastSafeOffset).toBe(table.length);
    expect(state.openStructure).toBe("paragraph");
  });

  it("delays a list boundary until the next top-level block proves the list ended", () => {
    // listChunk ends with one ambiguous blank line.
    // listChunk 以一个含义仍不明确的空行结束。
    const listChunk = "- first\n- second\n\n";
    // pendingState cannot yet prove whether another list item follows.
    // pendingState 尚不能证明后续是否还有列表项。
    const pendingState = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      listChunk,
    );
    // continuedState receives another item and must reject the pending boundary.
    // continuedState 接收另一列表项，必须拒绝待定边界。
    const continuedState = appendStreamingMarkdownBoundary(pendingState, "- third\n");
    // completedState receives a top-level paragraph after another blank line.
    // completedState 在另一空行后接收顶层段落。
    const completedState = appendStreamingMarkdownBoundary(continuedState, "\nafter\n");

    expect(pendingState.lastSafeOffset).toBe(0);
    expect(continuedState.lastSafeOffset).toBe(0);
    expect(completedState.lastSafeOffset).toBe(listChunk.length + "- third\n\n".length);
  });

  it("keeps indented continuation and loose items in one list container", () => {
    const firstItem = "- first\n  continued paragraph\n\n";
    const pending = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      firstItem,
    );
    const continued = appendStreamingMarkdownBoundary(pending, "- second\n\n");
    const completed = appendStreamingMarkdownBoundary(continued, "after\n");

    expect(pending.lastSafeOffset).toBe(0);
    expect(continued.lastSafeOffset).toBe(0);
    expect(completed.lastSafeOffset).toBe(`${firstItem}- second\n\n`.length);
  });

  it("keeps lazy prose continuations inside list and blockquote containers", () => {
    const listState = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "- first\nlazy continuation\n\n- second\n",
    );
    const quoteState = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "> quote\nlazy continuation\n\n> next\n",
    );

    expect(listState.lastSafeOffset).toBe(0);
    expect(listState.openStructure).toBe("list");
    expect(quoteState.lastSafeOffset).toBe(0);
    expect(quoteState.openStructure).toBe("blockquote");
  });

  it("tracks CRLF and a fence marker split across deltas", () => {
    // firstState retains an incomplete opening marker.
    // firstState 保留不完整的开启标记。
    const firstState = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "text\r\n\r\n``",
    );
    // secondState completes the opening fence and its content.
    // secondState 完成开启围栏及其内容。
    const secondState = appendStreamingMarkdownBoundary(firstState, "`js\r\ncode\r\n");

    expect(firstState.lastSafeOffset).toBe("text\r\n\r\n".length);
    expect(secondState.openStructure).toBe("fenced-code");
  });

  it("keeps fenced code nested in lists and blockquotes inside its container", () => {
    const listOpen = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "- item\n    ```ts\n    const value = 1;\n",
    );
    const listClosed = appendStreamingMarkdownBoundary(listOpen, "    ```\n\nafter\n");
    const quoteOpen = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "> intro\n> ~~~\n> quoted code\n",
    );
    const quoteClosed = appendStreamingMarkdownBoundary(quoteOpen, "> ~~~\n\nafter\n");

    expect(listOpen.openStructure).toBe("fenced-code");
    expect(listOpen.fenceContainer).toBe("list");
    expect(listClosed.lastSafeOffset).toBe("- item\n    ```ts\n    const value = 1;\n    ```\n\n".length);
    expect(quoteOpen.openStructure).toBe("fenced-code");
    expect(quoteOpen.fenceContainer).toBe("blockquote");
    expect(quoteClosed.lastSafeOffset).toBe("> intro\n> ~~~\n> quoted code\n> ~~~\n\n".length);
  });

  it("tracks table delimiters, HTML blocks, and directives across fragments", () => {
    const tableHeader = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "| A | B |\n| --",
    );
    const tableBody = appendStreamingMarkdownBoundary(tableHeader, "- | --- |\n| 1 | 2 |\n");
    const htmlOpen = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "<!-- open\n",
    );
    const htmlClosed = appendStreamingMarkdownBoundary(htmlOpen, "closed -->\n");
    const directiveOpen = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "::: details\ninside\n",
    );
    const directiveClosed = appendStreamingMarkdownBoundary(directiveOpen, ":::\n");

    expect(tableBody.openStructure).toBe("table");
    expect(htmlOpen.openStructure).toBe("html");
    expect(htmlClosed.openStructure).toBeNull();
    expect(directiveOpen.openStructure).toBe("directive");
    expect(directiveClosed.openStructure).toBeNull();
  });

  it("keeps textarea raw HTML open across internal blank lines", () => {
    const open = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "<textarea>\nvalue\n\n",
    );
    const closed = appendStreamingMarkdownBoundary(open, "</textarea>\n");

    expect(open.openStructure).toBe("html");
    expect(open.lastSafeOffset).toBe(0);
    expect(closed.openStructure).toBeNull();
    expect(closed.lastSafeOffset).toBe("<textarea>\nvalue\n\n</textarea>\n".length);
  });

  it("uses EOF as the final safe boundary after a sealed stream", () => {
    // openState models a provider ending with an unclosed code fence.
    // openState 模拟 Provider 以未关闭代码围栏结束。
    const openState = appendStreamingMarkdownBoundary(
      createStreamingMarkdownBoundaryState(),
      "```text\nunfinished",
    );
    // finishedState applies final Markdown EOF semantics once.
    // finishedState 一次性应用最终 Markdown EOF 语义。
    const finishedState = finishStreamingMarkdownBoundary(openState);

    expect(finishedState.lastSafeOffset).toBe(openState.inputLength);
    expect(finishedState.openStructure).toBeNull();
  });

  it("retains a long incomplete line as incremental fragments until one newline arrives", () => {
    let state = createStreamingMarkdownBoundaryState();
    for (let index = 0; index < 1_000; index += 1) {
      state = appendStreamingMarkdownBoundary(state, "abcdefgh");
    }

    expect(state.pendingLineLength).toBe(8_000);
    const completed = appendStreamingMarkdownBoundary(state, "\n\n");
    expect(completed.pendingLineLength).toBe(0);
    expect(completed.lastSafeOffset).toBe(8_002);
  });

  it("keeps explicit references and reusable definitions out of immutable independent segments", () => {
    expect(streamingMarkdownSegmentIsSelfContained("[link][target]\n\n")).toBe(false);
    expect(streamingMarkdownSegmentIsSelfContained("[link][target]\n\n[target]: /url\n\n")).toBe(false);
    expect(streamingMarkdownSegmentIsSelfContained("[ordinary label]\n\n")).toBe(true);
    expect(streamingMarkdownSegmentIsSelfContained("array[index]\n\n")).toBe(true);
    expect(streamingMarkdownSegmentIsSelfContained("[shortcut]\n\n[shortcut]: /url\n\n")).toBe(false);
    expect(streamingMarkdownSegmentIsSelfContained("![image][asset]\n\n")).toBe(false);
    expect(streamingMarkdownSegmentIsSelfContained("![image][asset]\n\n[asset]: /image.png\n\n")).toBe(false);
    expect(streamingMarkdownSegmentIsSelfContained("```text\n[not][a-reference]\n```\n\n")).toBe(true);
    expect(streamingMarkdownSegmentIsSelfContained("`[not][a-reference]`\n\n")).toBe(true);
    expect(streamingMarkdownSegmentIsSelfContained("note[^one]\n\n")).toBe(false);
    expect(streamingMarkdownSegmentIsSelfContained("note[^one]\n\n[^one]: detail\n\n")).toBe(false);
  });
});
