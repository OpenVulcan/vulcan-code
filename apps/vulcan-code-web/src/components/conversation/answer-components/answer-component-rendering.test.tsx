// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { prerender } from "react-dom/static";
import { WorkspaceFileOpenContext } from "@/components/workspace/workspace-file-open-request";
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
    expect(host.querySelectorAll(".vk-a-checklist li")).toHaveLength(2);
    expect(host.querySelector(".vk-codeblock")).toBeNull();
  });

  it("keeps an unclosed fence as code while it streams", async () => {
    const host = await render("```ui-checklist\nok | Builds\n", { streaming: true });
    expect(host.querySelector(".vk-a-checklist")).toBeNull();
    expect(host.querySelector(".vk-codeblock")).not.toBeNull();
  });

  it("keeps invalid component syntax as code", async () => {
    const host = await render("```ui-checklist\nmaybe | Builds\n```");
    expect(host.querySelector(".vk-a-checklist")).toBeNull();
    expect(host.querySelector(".vk-codeblock")).not.toBeNull();
  });

  it("renders components nested in a container with a longer outer fence", async () => {
    const host = await render("````ui-callout warning Heads up\nBody text\n\n```ui-kv\nRepo | vulcan\n```\n````");
    const callout = host.querySelector(".vk-a-callout");
    expect(callout?.getAttribute("data-tone")).toBe("warning");
    expect(callout?.querySelector(".vk-a-kv dd")?.textContent).toBe("vulcan");
  });

  it("links finding locations inside the workspace and leaves them plain without one", async () => {
    const fence = "```ui-findings\nhigh | Range read bypass | src/read.ts:373\n```";
    expect((await render(fence, { workspace: "D:\\repo" })).querySelector("a.vk-a-path")?.getAttribute("href")).toBe("file:///D:/repo/src/read.ts");
    expect((await render(fence)).querySelector("a.vk-a-path")).toBeNull();
    expect((await render(fence)).querySelector("code.vk-a-path")?.textContent).toBe("src/read.ts:373");
  });

  it("does not link prose written in a finding location", async () => {
    const fence = "```ui-findings\nmedium | Claim is unproven | 保湿剂，咖啡因是抗氧化剂\nlow | Note | see the readme\n```";
    expect((await render(fence, { workspace: "D:\\repo" })).querySelector("a.vk-a-path")).toBeNull();
  });

  it("links a checklist location cell and shows one-sided file counts and change labels", async () => {
    const checklist = await render("```ui-checklist\nno | Double count | reasoning in output | src/turn_stats.rs:150\n```", { workspace: "D:\\repo" });
    expect(checklist.querySelector(".vk-a-checklist a.vk-a-path")?.getAttribute("href")).toBe("file:///D:/repo/src/turn_stats.rs");
    const files = await render("```ui-files\nsrc/a.ts | +145 | entry\nsrc/b.ts | new | project id\n```");
    const rows = files.querySelectorAll(".vk-a-files li");
    expect(rows[0].querySelector(".vk-add")?.textContent).toBe("+145");
    expect(rows[0].querySelector(".vk-del")).toBeNull();
    expect(rows[1].querySelector(".vk-badge")?.textContent).toBe("new");
  });

  it("frames Markdown tables and right-aligns numeric columns only", async () => {
    const host = await render("| Stage | ms | Note |\n| --- | --- | --- |\n| Acquire | 12.4 | lock |\n| Execute | 1,204 | — |");
    expect(host.querySelector(".vk-figure table.vk-table")).not.toBeNull();
    const numeric = Array.from(host.querySelectorAll("tbody tr:first-child td"), (cell) => cell.hasAttribute("data-numeric"));
    expect(numeric).toEqual([false, true, false]);
  });

  it("draws a flow as one graph: a repeated label is one node and every hop is one edge", async () => {
    const host = await render("```ui-flow\n(Start) -> {Valid?}\n{Valid?} -> Render: yes\n{Valid?} --> Code: no\n```");
    expect(host.querySelectorAll(".vk-a-graph-node")).toHaveLength(4);
    expect(host.querySelectorAll(".vk-a-graph-edge")).toHaveLength(3);
    expect(host.querySelectorAll(".vk-a-graph-edge[data-dashed]")).toHaveLength(1);
    expect(host.querySelector(".vk-a-graph-node[data-shape='decision'] polygon")).not.toBeNull();
  });

  it("draws entity tables with one link per reference", async () => {
    const host = await render("```ui-er\nUser\n  id PK\nOrder\n  id PK\n  user_id -> User\n```");
    expect(host.querySelectorAll(".vk-a-er-table")).toHaveLength(2);
    expect(host.querySelectorAll(".vk-a-er .vk-a-graph-edge")).toHaveLength(1);
  });

  it("typesets inline $`…`$ math and leaves dollar amounts as text", async () => {
    const host = await render("Rate $`\\lambda < \\mu`$ holds; costs $5 and $10, and `$HOME` stays code.");
    expect(host.querySelectorAll(".vk-a-math-inline .katex")).toHaveLength(1);
    expect(host.textContent).toContain("costs $5 and $10");
    expect(host.querySelector("code.vk-md-code")?.textContent).toBe("$HOME");
  });

  it("keeps a formula KaTeX cannot parse visible as source", async () => {
    const host = await render("```ui-math\n\\frac{1}{\n```");
    expect(host.querySelector(".vk-a-math .katex")).toBeNull();
    expect(host.querySelector("code.vk-a-math-source")?.textContent).toBe("\\frac{1}{");
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
    const [captured, missing] = Array.from(host.querySelectorAll(".vk-a-code > figure"));
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
    expect(host.querySelector(".vk-a-code-range")?.textContent).toBe("10");
  });

  it("explains streaming, uncaptured and context-free excerpts", async () => {
    const message = async (state: AnswerCodeSnapshotState | null) =>
      (await renderCode(fence, state)).querySelector(".vk-a-code-missing p")?.textContent;
    expect(await message({ pending: true })).toBe("answerComponents.code.pending");
    expect(await message({ pending: false, snapshots: [] })).toBe("answerComponents.code.notCaptured");
    expect(await message(null)).toBe("answerComponents.code.notCaptured");
  });
});

describe("file references inside the workbench", () => {
  it("show the file at its lines in the files panel instead of linking", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const open = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    const state: AnswerCodeSnapshotState = {
      pending: false,
      snapshots: [{ location: "src/a.rs:10-12", result: { status: "captured", text: "a\nb\nc" } }],
    };
    await act(async () => root.render(
      <WorkspaceFileOpenContext.Provider value={open}>
        <AnswerCodeSnapshotsProvider value={state}>
          <MarkdownMessage content={"```ui-findings\nhigh | Range read | src/read.ts:373\n```\n\n```ui-code\nsrc/a.rs:10-12\n```"} eager />
        </AnswerCodeSnapshotsProvider>
      </WorkspaceFileOpenContext.Provider>,
    ));
    // The deferred component chunk can take seconds to transform under a loaded parallel run.
    // 在高负载的并行运行中，延迟组件分块的转换可能耗时数秒。
    for (const deadline = Date.now() + 5000; Date.now() < deadline && !host.querySelector(".vk-a-code-title button");) {
      await act(async () => new Promise((resolve) => setTimeout(resolve, 10)));
    }
    const finding = host.querySelector<HTMLElement>("code.vk-a-path[role='button']");
    expect(finding, "the finding location is a control").not.toBeNull();
    expect(host.querySelector("a.vk-a-path")).toBeNull();
    await act(async () => finding?.click());
    expect(open).toHaveBeenLastCalledWith("src/read.ts", { start: 373, end: 373 });
    await act(async () => host.querySelector<HTMLButtonElement>(".vk-a-code-title button")?.click());
    expect(open).toHaveBeenLastCalledWith("src/a.rs", { start: 10, end: 12 });
    await act(async () => root.unmount());
    host.remove();
  });

  it("show root files in the files panel and keep web pages as file links", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const open = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(
      <WorkspaceFileOpenContext.Provider value={open}>
        <MarkdownImageWorkspacePathProvider value="D:/repo">
          <MarkdownMessage content={"```ui-files\nnotes.txt | +2 | Log\nout/report.html | new | Page\n```"} eager />
        </MarkdownImageWorkspacePathProvider>
      </WorkspaceFileOpenContext.Provider>,
    ));
    for (const deadline = Date.now() + 5000; Date.now() < deadline && !host.querySelector(".vk-a-path");) {
      await act(async () => new Promise((resolve) => setTimeout(resolve, 10)));
    }
    expect(host.querySelector("a.vk-a-path")?.getAttribute("href")).toBe("file:///D:/repo/out/report.html");
    await act(async () => host.querySelector<HTMLElement>("code.vk-a-path[role='button']")?.click());
    expect(open).toHaveBeenLastCalledWith("notes.txt", null);
    await act(async () => root.unmount());
    host.remove();
  });
});
