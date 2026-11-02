// @vitest-environment jsdom
// Behaviour of the web search and extraction cards: routing of model-native and host provider tools, query
// operators, and the running, finished, empty, folded and failed bodies.
// 网页搜索与提取卡的行为：模型原生与宿主供应商工具的路由、查询运算符，以及运行中、完成、空结果、折叠与失败的正文。
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import enUS from "@catalogs/webui/en-US.json";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { readToolCall } from "../tool-call";
import { resolveToolCard, toolCallFamily } from "../tool-call-card";
import { FOLD_LIMIT, queryTokens, WebExtractCard, WebSearchCard } from "./web-cards";

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

// toolEvent builds a tool-call event with the given lifecycle and output.
// toolEvent 以给定生命周期与输出构建一条工具调用事件。
function toolEvent(
  toolName: string,
  input: Record<string, unknown>,
  status: ToolCallEvent["status"],
  output: { text: string; isError?: boolean } | null = null,
): ToolCallEvent {
  return {
    api_call_id: 1,
    input,
    kind: "tool_call",
    ...(output ? { output: { host_result: null, is_error: output.isError ?? false, model_output: output.text, tool_call_id: "call-1", tool_name: toolName } } : {}),
    request_stream_id: "stream-1",
    started_at_ms: 0,
    status,
    tool_call_id: 1,
    tool_name: toolName,
  } as unknown as ToolCallEvent;
}

// searchOutput repeats the host search report for the given number of hits.
// searchOutput 按给定命中数重复宿主搜索报告。
function searchOutput(query: string, count: number): string {
  if (count === 0) return `# Search: \`${query}\`\n\nNo results.\n`;
  const hits = Array.from({ length: count }, (_, index) => `\n${index + 1}. [Hit ${index + 1}](<https://site${index + 1}.test/page>)\n\nSnippet ${index + 1}\n`);
  return `# Search: \`${query}\`\n${hits.join("")}`;
}

// renderOpen mounts the card for one event with its body open.
// renderOpen 挂载一条事件的卡片并展开其正文。
function renderOpen(event: ToolCallEvent): void {
  const call = readToolCall(event);
  const { Card, family } = resolveToolCard(call);
  act(() => root.render(
    <I18nProvider initialLocale="en-US">
      <Card call={call} family={family} onOpenChange={() => undefined} open />
    </I18nProvider>,
  ));
}

const QUERY = "site:developers.openai.com/codex authentication ChatGPT API key files upload";

describe("web card routing", () => {
  it("routes model-native and host provider web tools to the web cards", () => {
    for (const name of ["web_search", "vulcan_web_search"]) {
      expect(resolveToolCard(readToolCall(toolEvent(name, { query: "q" }, "running"))).Card).toBe(WebSearchCard);
    }
    for (const name of ["web_extract", "vulcan_web_extract"]) {
      expect(resolveToolCard(readToolCall(toolEvent(name, { url: "https://a.test" }, "running"))).Card).toBe(WebExtractCard);
    }
    expect(toolCallFamily(toolEvent("vulcan_web_search", { query: "q" }, "running"))).toBe("web");
    expect(toolCallFamily(toolEvent("vulcan_image_generate", { prompt: "p" }, "running"))).toBe("media");
  });
});

describe("query tokens", () => {
  it("separates operators, negations and phrases, keeping addresses as terms", () => {
    expect(queryTokens("-site:a.test lang:en \"exact words\" -spam https://b.test/x key")).toEqual([
      { key: "site", kind: "operator", negated: true, value: "a.test" },
      { key: "lang", kind: "operator", negated: false, value: "en" },
      { key: "", kind: "phrase", negated: false, value: "exact words" },
      { key: "", kind: "term", negated: true, value: "spam" },
      { key: "", kind: "term", negated: false, value: "https://b.test/x" },
      { key: "", kind: "term", negated: false, value: "key" },
    ]);
  });
});

describe("web search card", () => {
  it("marks operators in the head and lists them while the search runs", () => {
    renderOpen(toolEvent("vulcan_web_search", { max_results: 10, query: QUERY }, "running"));
    expect(host.querySelector(".vk-tool")?.getAttribute("data-state")).toBe("running");
    expect(host.querySelector(".vk-tool-head .vk-web-op[data-kind='operator'] > i")?.textContent).toBe("site:");
    const facts = [...host.querySelectorAll(".vk-web-query dt")].map((node) => node.textContent);
    expect(facts).toEqual([enUS["toolCard.web.terms"], enUS["toolCard.web.op.site"]]);
    expect(host.querySelector(".vk-tool-head")?.getAttribute("title")).toBe(QUERY);
    expect(host.querySelector(".vk-sources")).toBeNull();
  });

  it("lists hits with their count and folds the rest of a long list", () => {
    const count = FOLD_LIMIT + 3;
    renderOpen(toolEvent("vulcan_web_search", { query: QUERY }, "completed", { text: searchOutput(QUERY, count) }));
    expect(host.querySelector(".vk-tool")?.getAttribute("data-state")).toBe("done");
    expect(host.querySelector(".vk-tool-meta")?.textContent).toContain(enUS["toolCard.provider.hits"].replace("{count}", String(count)));
    expect(host.querySelectorAll(".vk-source")).toHaveLength(FOLD_LIMIT);
    expect(host.querySelector(".vk-source-title")?.getAttribute("href")).toBe("https://site1.test/page");
    const fold = host.querySelector<HTMLButtonElement>(".vk-web-fold");
    expect(fold?.textContent).toBe(enUS["toolCard.web.showMore"].replace("{count}", "3"));
    act(() => fold?.click());
    expect(host.querySelectorAll(".vk-source")).toHaveLength(count);
    expect(fold?.getAttribute("aria-expanded")).toBe("true");
  });

  it("does not fold a short list", () => {
    renderOpen(toolEvent("web_search", { args: { query: "q" }, model: "m/x" }, "completed", { text: searchOutput("q", 2) }));
    expect(host.querySelectorAll(".vk-source")).toHaveLength(2);
    expect(host.querySelector(".vk-web-fold")).toBeNull();
    expect(host.querySelector(".vk-web-query")).toBeNull();
  });

  it("shows the empty state when nothing matched", () => {
    renderOpen(toolEvent("vulcan_web_search", { query: "q" }, "completed", { text: searchOutput("q", 0) }));
    expect(host.querySelector(".vk-tool-note[data-empty]")?.textContent).toBe(enUS["toolCard.provider.noHits"]);
    expect(host.querySelector(".vk-sources")).toBeNull();
  });

  it("shows the provider error of a failed search", () => {
    renderOpen(toolEvent("vulcan_web_search", { query: QUERY }, "completed", { isError: true, text: "provider rejected the request" }));
    expect(host.querySelector(".vk-tool")?.getAttribute("data-state")).toBe("failed");
    expect(host.querySelector(".vk-tool-body")?.textContent).toContain("provider rejected the request");
    expect(host.querySelector(".vk-sources")).toBeNull();
  });
});

describe("web extract card", () => {
  it("lists fetched pages and failed sources", () => {
    const text = "# Extracted pages\n\n## [https://a.test/x](<https://a.test/x>)\n\nBody\n\n## Failed sources\n\n- [https://c.test/](<https://c.test/>): timeout\n";
    renderOpen(toolEvent("vulcan_web_extract", { url: "https://a.test/x" }, "completed", { text }));
    expect(host.querySelectorAll(".vk-source")).toHaveLength(2);
    expect(host.querySelectorAll(".vk-source[data-failed]")).toHaveLength(1);
    expect(host.querySelector(".vk-tool-head")?.getAttribute("title")).toBe("https://a.test/x");
  });

  it("shows the empty state when no page came back", () => {
    renderOpen(toolEvent("vulcan_web_extract", { url: "https://a.test/x" }, "completed", { text: "# Extracted pages\n\nNo pages extracted.\n" }));
    expect(host.querySelector(".vk-tool-note[data-empty]")?.textContent).toBe(enUS["toolCard.provider.noPages"]);
  });
});
