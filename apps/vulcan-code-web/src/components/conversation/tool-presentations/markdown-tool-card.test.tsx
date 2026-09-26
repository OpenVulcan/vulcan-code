// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolDetailPayload } from "../conversation-detail";
import type { MarkdownToolPresentation } from "../conversation-tool-presentations";
import { MarkdownToolPresentationCard } from "./markdown-tool-card";

// Isolate card-owned commits from the syntax worker, whose independent completion legitimately renders again.
// 隔离卡片自身提交与语法 Worker；后者独立完成时允许再次渲染。
vi.mock("../conversation-markdown", () => ({
  // Displays the exact supplied Markdown; returns a read-only body without asynchronous highlighting.
  // 显示传入的精确 Markdown；返回不执行异步高亮的只读正文。
  MarkdownMessage: (props: { content: string }) => <pre>{props.content}</pre>,
}));

// host and root retain the same real card across streaming snapshot replacements.
// host 和 root 在流式快照替换期间保留同一真实卡片。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

/** Builds an AST result presentation; id owns its identity and markdown is its current body.
 * 构建 AST 结果展示；id 持有身份，markdown 为当前正文。
 * Returns a canonical completed card without a legacy detail RPC.
 * 返回不带旧版详情 RPC 的规范已完成卡片。 */
function presentation(id: string, markdown: string): MarkdownToolPresentation {
  return { id, markdown, kind: "markdown_tool_output", contentKind: "markdown", icon: "ast_tree",
    state: "completed", sourceSections: [], summaryTargets: [],
    titleKey: "toolPresentation.codekit.astTree.title", toolId: "vulcan_codekit_ast_tree" };
}

/** Mounts or updates one real card; commits captures committed DOM and loader is the optional detail source.
 * 挂载或更新真实卡片；commits 捕获已提交 DOM，loader 为可选详情来源。
 * Returns after React effects settle, exposing stale intermediate commits and update feedback.
 * 在 React effect 稳定后返回，暴露过期中间提交与更新反馈。 */
async function render(card: MarkdownToolPresentation, commits: string[], loader?: () => Promise<ToolDetailPayload | null>) {
  await act(async () => root.render(<I18nProvider initialLocale="en-US">
    <React.Profiler id="ast-card" onRender={() => commits.push(host.textContent ?? "")}>
      <MarkdownToolPresentationCard presentation={card} expanded requestProgress={null}
        onExpandedChange={() => {}} onLoadToolDetail={loader} />
    </React.Profiler>
  </I18nProvider>));
}

it("commits each authoritative AST update once without stale prop-mirroring effects", async () => {
  // commits observes actual renders, including any updates caused by child effects.
  // commits 观察真实渲染，包括子 effect 引起的更新。
  const commits: string[] = [];
  await render(presentation("live-ast", "previous output"), commits);
  commits.length = 0;
  await render(presentation("live-ast", "current output"), commits);
  expect(commits).toHaveLength(1);
  expect(commits[0]).toContain("current output");
  expect(commits[0]).not.toContain("previous output");
  for (let revision = 0; revision < 60; revision += 1) {
    commits.length = 0;
    await render(presentation("live-ast", "current output"), commits);
    expect(commits).toHaveLength(1);
  }
});

it("retains one pending detail load across equivalent streamed presentation objects", async () => {
  // resolveDetail controls the real async boundary without timers or network traffic.
  // resolveDetail 控制真实异步边界，不依赖计时器或网络。
  let resolveDetail!: (value: ToolDetailPayload) => void;
  const loader = vi.fn(() => new Promise<ToolDetailPayload>((resolve) => { resolveDetail = resolve; }));
  const card = { ...presentation("pending-detail", "summary"), detailRef: { sessionId: "card-test", recordId: "pending-detail" } };
  await render(card, [], loader);
  await render({ ...card, detailRef: { ...card.detailRef } }, [], loader);
  expect(loader).toHaveBeenCalledTimes(1);
  await act(async () => resolveDetail({ body: "complete detail" }));
  expect(host.textContent).toContain("complete detail");
  await render({ ...card, state: "failed" }, [], loader);
  expect(loader).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain("complete detail");
});

it("does not transfer a late detail response into the next tool card", async () => {
  // resolvePrevious releases the old lookup after the component has changed ownership.
  // resolvePrevious 在组件改变归属后释放旧查询。
  let resolvePrevious!: (value: ToolDetailPayload) => void;
  const loader = () => new Promise<ToolDetailPayload>((resolve) => { resolvePrevious = resolve; });
  await render({ ...presentation("previous-tool", "old"), detailRef: { sessionId: "card-test", recordId: "previous-tool" } }, [], loader);
  await render(presentation("next-tool", "new owner"), []);
  await act(async () => resolvePrevious({ body: "late old detail" }));
  expect(host.textContent).toContain("new owner");
  expect(host.textContent).not.toContain("late old detail");
});
