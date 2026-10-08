// @vitest-environment jsdom
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { MarkdownImageWorkspacePathProvider, MarkdownMessage } from "../conversation-markdown";

// Only the text catalog is synthetic; keys stand in for localized labels.
// 仅文案目录为合成边界；以键代替本地化标签。
vi.mock("@/i18n", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/i18n")>(),
  useI18n: () => ({ t: (key: string) => key }),
}));

// render returns a detached element holding the server-rendered Markdown.
// render 返回承载服务端渲染 Markdown 的游离元素。
function render(content: string, options: { streaming?: boolean; workspace?: string } = {}): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = renderToStaticMarkup(
    <MarkdownImageWorkspacePathProvider value={options.workspace}>
      <MarkdownMessage content={content} eager streaming={options.streaming} />
    </MarkdownImageWorkspacePathProvider>,
  );
  return host;
}

describe("answer components in conversation Markdown", () => {
  it.each([false, true])("renders a closed component fence natively (streaming=%s)", (streaming) => {
    const host = render("Intro\n\n```ui-checklist\nok | Builds\nno | Tests | flaky\n```\n\nAfter", { streaming });
    expect(host.querySelectorAll(".answer-checklist li")).toHaveLength(2);
    expect(host.querySelector(".markdown-code-block")).toBeNull();
  });

  it("keeps an unclosed fence as code while it streams", () => {
    const host = render("```ui-checklist\nok | Builds\n", { streaming: true });
    expect(host.querySelector(".answer-checklist")).toBeNull();
    expect(host.querySelector(".markdown-code-block")).not.toBeNull();
  });

  it("keeps invalid component syntax as code", () => {
    const host = render("```ui-checklist\nmaybe | Builds\n```");
    expect(host.querySelector(".answer-checklist")).toBeNull();
    expect(host.querySelector(".markdown-code-block")).not.toBeNull();
  });

  it("renders components nested in a container with a longer outer fence", () => {
    const host = render("````ui-callout warning Heads up\nBody text\n\n```ui-kv\nRepo | vulcan\n```\n````");
    const callout = host.querySelector(".answer-callout");
    expect(callout?.getAttribute("data-tone")).toBe("warning");
    expect(callout?.querySelector(".answer-kv dd")?.textContent).toBe("vulcan");
  });

  it("links finding locations inside the workspace and leaves them plain without one", () => {
    const fence = "```ui-findings\nhigh | Range read bypass | src/read.ts:373\n```";
    expect(render(fence, { workspace: "D:\\repo" }).querySelector("a.answer-path")?.getAttribute("href")).toBe("file:///D:/repo/src/read.ts");
    expect(render(fence).querySelector("a.answer-path")).toBeNull();
    expect(render(fence).querySelector("code.answer-path")?.textContent).toBe("src/read.ts:373");
  });

  it("frames Markdown tables and right-aligns numeric columns only", () => {
    const host = render("| Stage | ms | Note |\n| --- | --- | --- |\n| Acquire | 12.4 | lock |\n| Execute | 1,204 | — |");
    expect(host.querySelector(".answer-frame table.answer-table")).not.toBeNull();
    const numeric = Array.from(host.querySelectorAll("tbody tr:first-child td"), (cell) => cell.hasAttribute("data-numeric"));
    expect(numeric).toEqual([false, true, false]);
  });
});
