// @vitest-environment jsdom
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import enUS from "@catalogs/webui/en-US.json";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { readToolCall } from "../tool-call";
import { resolvePurposeCard, resolveToolCard } from "../tool-call-card";
import { requestToolCall } from "../tool-request";
import { BrowserCard } from "./browser-card";

/** Builds a browser event from its public name, submitted arguments, and optional finished result.
 * 通过公开名称、提交参数和可选终态结果构建浏览器事件。 */
function browserEvent(
  toolName: string,
  input: ToolCallEvent["input"],
  output?: ToolCallEvent["output"],
): ToolCallEvent {
  return {
    api_call_id: 1,
    kind: "tool_call",
    input,
    output,
    request_stream_id: "browser-request",
    status: output ? "completed" : "running",
    tool_call_id: 1,
    tool_name: toolName,
  };
}

/** Renders a browser event through the normal card resolver into a searchable test element.
 * 通过常规卡片路由将浏览器事件渲染为可查询的测试元素。 */
function renderCard(event: ToolCallEvent): HTMLDivElement {
  // Resolve the same component and family used by the conversation transcript.
  // 解析会话记录使用的同一组件和家族。
  const call = readToolCall(event);
  const { Card, family } = resolveToolCard(call);
  // Parse the static HTML so assertions address visible output by element and state.
  // 解析静态 HTML，使断言能够按元素及状态验证显示结果。
  const rendered = document.createElement("div");
  rendered.innerHTML = renderToStaticMarkup(
    <I18nProvider initialLocale="en-US">
      <Card call={call} family={family} onOpenChange={() => undefined} open />
    </I18nProvider>,
  );
  return rendered;
}

describe("Playwright browser tool card", () => {
  it("routes both a streaming request and its finished call and leaves unrelated MCP names alone", () => {
    // Use the public tool identity recorded by the browser integration.
    // 使用浏览器集成记录的公开工具标识。
    const name = "mcp_vulcan_browser_playwright_browser_click";
    const request = requestToolCall(name, { bytes: 0, phase: "receiving", preview: null }, "browser-request", "running");
    expect(resolveToolCard(request)).toEqual({ Card: BrowserCard, family: "web" });
    expect(resolveToolCard(readToolCall(browserEvent(name, { target: "e1" }, { model_output: "Clicked." })))).toEqual({ Card: BrowserCard, family: "web" });
    expect(resolvePurposeCard(readToolCall(browserEvent("mcp_other_playwright_browser_click", { target: "e1" })))).toBeNull();
  });

  it("shows the requested element and preserves the real error lines before the collapsed arguments", () => {
    // A terminal-styled failure matches the shape of a Playwright click diagnostic.
    // 带终端样式的失败与 Playwright 点击诊断的格式一致。
    const escape = String.fromCharCode(27);
    const diagnostic = "TimeoutError\n  - overlay intercepts pointer events";
    const view = renderCard(browserEvent("mcp_vulcan_browser_playwright_browser_click", {
      element: "Canvas node b",
      target: '[data-cls="two/b"]',
    }, { is_error: true, model_output: `${escape}[2m${diagnostic}${escape}[22m` }));
    expect(view.querySelector(".vk-tool")?.getAttribute("data-state")).toBe("failed");
    expect(view.querySelector(".vk-tool-head")?.textContent).toContain(enUS["toolCard.browser.click"]);
    expect(view.querySelector(".vk-tool-head")?.getAttribute("title")).toBe("Canvas node b");
    expect(view.querySelector(".vk-tool-body")?.firstElementChild?.textContent).toBe(diagnostic);
    expect(view.querySelector(".vk-browser-tool-arguments[open]")).toBeNull();
    expect(view.querySelector(".vk-browser-tool-arguments .vk-json")?.textContent).toContain("two/b");
  });

  it("renders a page snapshot in the conversation Markdown pipeline and uses the actual tab action and index", () => {
    // Playwright page details are Markdown followed by the accessibility tree in a fenced block.
    // Playwright 页面详情用 Markdown 返回，随后在围栏代码块中给出无障碍树。
    const snapshot = renderCard(browserEvent("mcp_vulcan_browser_playwright_browser_snapshot", {}, {
      model_output: "### Page\n- Page URL: https://example.test/\n\n### Snapshot\n```yaml\n- button Submit [ref=e1]\n```",
    }));
    expect(snapshot.querySelector(".vk-tool-result h3")?.textContent).toBe("Page");
    expect(snapshot.querySelector(".vk-tool-result pre")?.textContent).toContain("button Submit [ref=e1]");
    expect(snapshot.querySelector(".vk-browser-tool-arguments")).toBeNull();
    // The first tab is a valid zero index.
    // 首个标签页的索引零是有效值。
    const tab = renderCard(browserEvent("mcp_vulcan_browser_playwright_browser_tabs", { action: "select", index: 0 }));
    expect(tab.querySelector(".vk-tool-head")?.textContent).toContain(enUS["toolCard.browser.selectTab"]);
    expect(tab.querySelector(".vk-tool-head")?.getAttribute("title")).toBe("#0");
  });
});
