// @vitest-environment jsdom
import React from "react";
import { prerender } from "react-dom/static";
import { describe, expect, it, vi } from "vitest";
import {
  AnswerCodeSnapshotsProvider,
  MarkdownImageWorkspacePathProvider,
  MarkdownMessage,
  type AnswerCodeSnapshotState,
} from "../conversation-markdown";

// Only the text catalog is synthetic; keys stand in for localized labels.
// 仅文案目录为合成边界；以键代替本地化标签。
vi.mock("@/i18n", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/i18n")>(),
  useI18n: () => ({ t: (key: string) => key }),
}));

// render returns a detached element holding the prerendered Markdown once deferred components resolve.
// render 在延迟组件就绪后返回承载预渲染 Markdown 的游离元素。
async function render(content: string, options: { streaming?: boolean; workspace?: string } = {}): Promise<HTMLElement> {
  const host = document.createElement("div");
  const { prelude } = await prerender(
    <MarkdownImageWorkspacePathProvider value={options.workspace}>
      <MarkdownMessage content={content} eager streaming={options.streaming} />
    </MarkdownImageWorkspacePathProvider>,
  );
  host.innerHTML = await new Response(prelude).text();
  return host;
}

describe("answer components in conversation Markdown", () => {
  it.each([false, true])("renders a closed component fence natively (streaming=%s)", async (streaming) => {
    const host = await render("Intro\n\n```ui-checklist\nok | Builds\nno | Tests | flaky\n```\n\nAfter", { streaming });
    expect(host.querySelectorAll(".answer-checklist li")).toHaveLength(2);
    expect(host.querySelector(".vk-codeblock")).toBeNull();
  });

  it("keeps an unclosed fence as code while it streams", async () => {
    const host = await render("```ui-checklist\nok | Builds\n", { streaming: true });
    expect(host.querySelector(".answer-checklist")).toBeNull();
    expect(host.querySelector(".vk-codeblock")).not.toBeNull();
  });

  it("keeps invalid component syntax as code", async () => {
    const host = await render("```ui-checklist\nmaybe | Builds\n```");
    expect(host.querySelector(".answer-checklist")).toBeNull();
    expect(host.querySelector(".vk-codeblock")).not.toBeNull();
  });

  it("renders components nested in a container with a longer outer fence", async () => {
    const host = await render("````ui-callout warning Heads up\nBody text\n\n```ui-kv\nRepo | vulcan\n```\n````");
    const callout = host.querySelector(".answer-callout");
    expect(callout?.getAttribute("data-tone")).toBe("warning");
    expect(callout?.querySelector(".answer-kv dd")?.textContent).toBe("vulcan");
  });

  it("links finding locations inside the workspace and leaves them plain without one", async () => {
    const fence = "```ui-findings\nhigh | Range read bypass | src/read.ts:373\n```";
    expect((await render(fence, { workspace: "D:\\repo" })).querySelector("a.answer-path")?.getAttribute("href")).toBe("file:///D:/repo/src/read.ts");
    expect((await render(fence)).querySelector("a.answer-path")).toBeNull();
    expect((await render(fence)).querySelector("code.answer-path")?.textContent).toBe("src/read.ts:373");
  });

  it("does not link prose written in a finding location", async () => {
    const fence = "```ui-findings\nmedium | Claim is unproven | 保湿剂，咖啡因是抗氧化剂\nlow | Note | see the readme\n```";
    expect((await render(fence, { workspace: "D:\\repo" })).querySelector("a.answer-path")).toBeNull();
  });

  it("frames Markdown tables and right-aligns numeric columns only", async () => {
    const host = await render("| Stage | ms | Note |\n| --- | --- | --- |\n| Acquire | 12.4 | lock |\n| Execute | 1,204 | — |");
    expect(host.querySelector(".vk-figure table.vk-table")).not.toBeNull();
    const numeric = Array.from(host.querySelectorAll("tbody tr:first-child td"), (cell) => cell.hasAttribute("data-numeric"));
    expect(numeric).toEqual([false, true, false]);
  });

  it("draws a flow as one graph: a repeated label is one node and every hop is one edge", async () => {
    const host = await render("```ui-flow\n(Start) -> {Valid?}\n{Valid?} -> Render: yes\n{Valid?} --> Code: no\n```");
    expect(host.querySelectorAll(".answer-graph-node")).toHaveLength(4);
    expect(host.querySelectorAll(".answer-graph-edge")).toHaveLength(3);
    expect(host.querySelectorAll(".answer-graph-edge[data-dashed]")).toHaveLength(1);
    expect(host.querySelector(".answer-graph-node[data-shape='decision'] polygon")).not.toBeNull();
  });

  it("draws entity tables with one link per reference", async () => {
    const host = await render("```ui-er\nUser\n  id PK\nOrder\n  id PK\n  user_id -> User\n```");
    expect(host.querySelectorAll(".answer-er-table")).toHaveLength(2);
    expect(host.querySelectorAll(".answer-er .answer-graph-edge")).toHaveLength(1);
  });

  it("typesets inline $`…`$ math and leaves dollar amounts as text", async () => {
    const host = await render("Rate $`\\lambda < \\mu`$ holds; costs $5 and $10, and `$HOME` stays code.");
    expect(host.querySelectorAll(".answer-math-inline .katex")).toHaveLength(1);
    expect(host.textContent).toContain("costs $5 and $10");
    expect(host.querySelector("code.vk-md-code")?.textContent).toBe("$HOME");
  });

  it("keeps a formula KaTeX cannot parse visible as source", async () => {
    const host = await render("```ui-math\n\\frac{1}{\n```");
    expect(host.querySelector(".answer-math .katex")).toBeNull();
    expect(host.querySelector("code.answer-math-source")?.textContent).toBe("\\frac{1}{");
  });
});

describe("ui-code excerpts", () => {
  // renderCode prerenders a reply whose `ui-code` fences resolve against `state`.
  // renderCode 预渲染一条回答，其 `ui-code` 围栏按 `state` 解析。
  async function renderCode(content: string, state: AnswerCodeSnapshotState | null): Promise<HTMLElement> {
    const host = document.createElement("div");
    const { prelude } = await prerender(
      <AnswerCodeSnapshotsProvider value={state}>
        <MarkdownMessage content={content} eager />
      </AnswerCodeSnapshotsProvider>,
    );
    host.innerHTML = await new Response(prelude).text();
    return host;
  }

  const fence = "```ui-code\nsrc/a.rs:10-12 | 11 | Entry\nsrc/b.rs:3\n```";

  it("shows the captured lines numbered from the start line, never the fence text", async () => {
    const host = await renderCode(fence, {
      pending: false,
      snapshots: [
        { location: "src/a.rs:10-12", result: { status: "captured", text: "fn a() {\n    lock();\n}" } },
        { location: "src/b.rs:3", result: { status: "unavailable", reason: "not_found" } },
      ],
    });
    const [captured, missing] = Array.from(host.querySelectorAll(".answer-code > figure"));
    expect(Array.from(captured.querySelectorAll(".vk-codeblock-num"), (cell) => cell.textContent)).toEqual(["10", "11", "12"]);
    expect(captured.querySelector(".vk-codeblock-line[data-highlighted='true'] .vk-codeblock-num")?.textContent).toBe("11");
    expect(captured.textContent).toContain("lock();");
    expect(missing.querySelector("p")?.textContent).toBe("answerComponents.code.reason.not_found");
  });

  it("shows the range the file actually had when it ended early", async () => {
    const host = await renderCode(fence, {
      pending: false,
      snapshots: [{ location: "src/a.rs:10-12", result: { status: "captured", text: "only" } }],
    });
    expect(host.querySelector(".answer-code-range")?.textContent).toBe("10");
  });

  it("explains streaming, uncaptured and context-free excerpts", async () => {
    const message = async (state: AnswerCodeSnapshotState | null) =>
      (await renderCode(fence, state)).querySelector(".answer-code-missing p")?.textContent;
    expect(await message({ pending: true })).toBe("answerComponents.code.pending");
    expect(await message({ pending: false, snapshots: [] })).toBe("answerComponents.code.notCaptured");
    expect(await message(null)).toBe("answerComponents.code.notCaptured");
  });
});
