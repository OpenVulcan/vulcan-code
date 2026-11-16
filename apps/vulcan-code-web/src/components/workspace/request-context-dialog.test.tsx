// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider, readLocaleMessage } from "@/i18n";
import { serviceClient } from "@/service/webui-service-runtime";
import { RequestContextDialog, type RequestContextSnapshot } from "./request-context-dialog";

vi.mock("@/service/webui-service-runtime", () => ({ serviceClient: { request: vi.fn() } }));

/** Builds two distinct physical request snapshots for one conversation. */
/** 为同一会话构建两份不同的物理请求快照。 */
function snapshot(executionId: string, content: string): { status: "available"; snapshot: RequestContextSnapshot } {
  return {
    status: "available",
    snapshot: {
      dispatch: { executionId, apiCallId: "42", model: "fixture-model", protocol: "responses", purpose: "main_generation", dispatchedAtMs: 100 },
      nodeCount: 2,
      nodes: [
        { field: "previous_response_id", index: null, role: null, kind: null, content: "cursor-42", truncated: false },
        { field: "input", index: 0, role: "user", kind: "message", content, text: content, truncated: false },
      ],
    },
  };
}

it("expands one dispatched node, replaces it on the next dispatch, and stops reading when closed", async () => {
  // Use the real modal and detail selection against a deterministic sequence of service responses.
  // 用真实弹窗和详情选择验证确定的服务响应序列。
  vi.useFakeTimers();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const request = vi.mocked(serviceClient.request);
  request.mockResolvedValueOnce(snapshot("execution-a", "first request"));
  request.mockResolvedValueOnce(snapshot("execution-b", "second request"));
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  try {
    await act(async () => root.render(<I18nProvider initialLocale="en-US"><RequestContextDialog sessionId="fixture-session" onClose={vi.fn()} /></I18nProvider>));
    expect(request).toHaveBeenCalledWith("service_latest_request_context_v1", { sessionId: "fixture-session", knownExecutionId: undefined, knownAssistantResponse: false });
    expect(request).toHaveBeenCalledTimes(1);
    expect(document.querySelector('[data-slot="dialog-title"] + [data-slot="dialog-description"]')?.textContent).toContain("fixture-model");
    expect(document.querySelector('[data-slot="dialog-header"] button')).toBeNull();
    const rows = [...document.querySelectorAll<HTMLButtonElement>(".vk-request-context-row")];
    expect(rows.map((row) => row.dataset.tone)).toEqual(["context", "user"]);
    expect(document.querySelector(".vk-request-context-detail")).toBeNull();
    await act(async () => rows[1].click());
    expect(document.querySelector(".vk-request-context-detail .vk-md")?.textContent).toBe("first request");
    await act(async () => { await vi.runOnlyPendingTimersAsync(); });
    expect(request).toHaveBeenLastCalledWith("service_latest_request_context_v1", { sessionId: "fixture-session", knownExecutionId: "execution-a", knownAssistantResponse: false });
    expect(document.querySelector(".vk-request-context-body")?.textContent).toContain("second request");
    expect(document.querySelector(".vk-request-context-body pre")).toBeNull();
  } finally {
    await act(async () => root.unmount());
    await vi.runOnlyPendingTimersAsync();
    host.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
  expect(request).toHaveBeenCalledTimes(2);
});

it("assembles only once while the actual record is absent and switches when one arrives", async () => {
  // Keep polling the actual record while displaying local simulation.
  // 显示本机模拟时继续轮询实发记录。
  vi.useFakeTimers();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const request = vi.mocked(serviceClient.request);
  request.mockReset();
  request.mockResolvedValueOnce({ status: "empty" });
  // Preserve the actual raw schema so the Raw tab can be compared byte for byte.
  // 保留实际原始 Schema，以便逐字节比对原始内容标签。
  const schema = JSON.stringify({
    name: "preview_tool", description: "## Tool introduction",
    parameters: {
      type: "object",
      properties: { format: { type: "string", enum: ["json", "text"], description: "**Output format**" } },
      required: ["format"],
      oneOf: [{ required: ["from_file"] }, { required: ["from_text"] }],
    },
  });
  request.mockResolvedValueOnce({ nodeCount: 4, nodes: [
    { field: "baseline", index: 0, role: "system", kind: null, content: "preview instructions", truncated: false },
    { field: "tools", index: 0, role: null, kind: "function", content: schema, truncated: false },
    { field: "conversation", index: 0, role: null, kind: "user_input", content: "automatically assembled", truncated: false },
    { field: "conversation", index: 1, role: null, kind: "assistant_text", content: '{"tool_calls":[{"tool_name":"lookup"}]}', text: "",
      toolParts: [{ name: "lookup", result: false, content: '{"query":"x"}' }], reasoning: "### Thinking before the tool", truncated: false },
  ] });
  request.mockResolvedValueOnce({ status: "empty" });
  request.mockResolvedValueOnce(snapshot("execution-a", "actual request"));
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  try {
    await act(async () => root.render(<I18nProvider initialLocale="en-US"><RequestContextDialog sessionId="cold-session" onClose={vi.fn()} /></I18nProvider>));
    expect(request).toHaveBeenNthCalledWith(1, "service_latest_request_context_v1", { sessionId: "cold-session", knownExecutionId: undefined, knownAssistantResponse: false });
    expect(request).toHaveBeenCalledTimes(2);
    expect(request).toHaveBeenLastCalledWith("service_simulate_request_context_v1", { sessionId: "cold-session" });
    expect(document.querySelector(".vk-request-context-body")?.textContent).toContain("automatically assembled");
    expect(document.querySelector('[data-slot="dialog-title"] + [data-slot="dialog-description"]')?.textContent)
      .toContain(readLocaleMessage("en-US", "requestContext.simulate"));
    expect(document.querySelector('[data-slot="dialog-header"] button')).toBeNull();
    // Simulated tools share the same setup tab and expandable schema as dispatched tools.
    // 模拟工具与实发工具共用初始设置标签页和可展开的 Schema。
    const tools = [...document.querySelectorAll<HTMLButtonElement>(".vk-request-context-tabs button")]
      .find((button) => button.textContent === readLocaleMessage("en-US", "skills.action.tools"))!;
    await act(async () => tools.click());
    // The row remains unset until the user expands the selected tool.
    // 工具行保持收起，直到用户展开相应工具。
    const tool = document.querySelector<HTMLButtonElement>('.vk-request-context-detail [data-tone="tool"]')!;
    expect(tool.textContent).toContain("preview_tool");
    await act(async () => tool.click());
    expect(document.querySelector(".vk-request-context-schema .vk-md h2")?.textContent).toBe("Tool introduction");
    expect(document.querySelector(".vk-request-context-schema tbody")?.textContent).toContain("format");
    expect(document.querySelector(".vk-request-context-schema tbody")?.textContent)
      .toContain(readLocaleMessage("en-US", "toolDomain.model.required"));
    expect(document.querySelector(".vk-request-context-schema td .vk-md strong")?.textContent).toBe("Output format");
    expect(document.querySelector(".vk-request-context-schema-branch summary")?.textContent)
      .toContain(readLocaleMessage("en-US", "requestContext.schemaOneOf"));
    expect(document.querySelector(".vk-request-context-schema-branch")?.textContent).toContain("from_text");
    // Raw retains the original definition rather than the parsed table representation.
    // 原始标签保留原始声明，而非解析后的表格表示。
    const raw = [...document.querySelectorAll<HTMLButtonElement>(".vk-request-context-tabs button")]
      .find((button) => button.textContent === readLocaleMessage("en-US", "requestContext.raw"))!;
    await act(async () => raw.click());
    expect(document.querySelector(".vk-request-context-detail pre")?.textContent).toBe(schema);
    // A simulated invocation exposes its model reasoning ahead of the invocation record.
    // 模拟工具调用在调用记录之前展示其模型思考。
    const call = document.querySelector<HTMLButtonElement>('.vk-request-context-list [data-tone="tool"]')!;
    await act(async () => call.click());
    expect(document.querySelector(".vk-request-context-reasoning .vk-md h3")?.textContent).toBe("Thinking before the tool");
    expect(document.querySelector(".vk-request-context-detail-header strong")?.textContent).toContain("lookup");
    expect(document.querySelector(".vk-request-context-text pre")?.textContent).toBe('{"query":"x"}');
    await act(async () => { await vi.runOnlyPendingTimersAsync(); });
    expect(request).toHaveBeenCalledTimes(3);
    expect(document.querySelector(".vk-request-context-body")?.textContent).toContain("automatically assembled");
    await act(async () => { await vi.runOnlyPendingTimersAsync(); });
    expect(request).toHaveBeenLastCalledWith("service_latest_request_context_v1", { sessionId: "cold-session", knownExecutionId: undefined, knownAssistantResponse: false });
    expect(document.querySelector(".vk-request-context-body")?.textContent).toContain("actual request");
  } finally {
    await act(async () => root.unmount());
    await vi.runOnlyPendingTimersAsync();
    host.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});

it("groups system prompt and tool schemas, and adds the same execution's assistant result", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const request = vi.mocked(serviceClient.request);
  request.mockReset();
  // The response arrives in a later poll without a change to the sent request.
  // 回复在后一次轮询到达，实发请求保持不变。
  const actual = snapshot("execution-c", "user prompt");
  actual.snapshot.nodes = [
    { field: "messages", index: 0, role: "system", kind: null, content: '{"content":"## system fixture"}', text: "## system fixture", truncated: false },
    { field: "tools", index: 0, role: null, kind: "function", content: '{"function":{"name":"fixture_tool","description":"fixture description"}}', truncated: false },
    ...actual.snapshot.nodes,
    { field: "messages", index: 2, role: "tool", kind: null, content: '{"host_result":null,"model_output":"## returned"}',
      toolParts: [{ name: "file_read", result: true, content: "## returned" }], truncated: false },
  ];
  actual.snapshot.promptOrigins = [{ kind: "memory_profile", source: "Fixture VMM" }];
  actual.snapshot.nodeCount = actual.snapshot.nodes.length;
  request.mockResolvedValueOnce(actual);
  request.mockResolvedValueOnce({ ...actual, assistantResponse: {
    content: "## finished reply", reasoning: "### thinking before the reply", truncated: false,
  } });
  request.mockResolvedValueOnce({ status: "unchanged" });
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  try {
    await act(async () => root.render(<I18nProvider initialLocale="en-US"><RequestContextDialog sessionId="fixture-session" onClose={vi.fn()} /></I18nProvider>));
    expect(document.querySelector(".vk-request-context-detail .vk-md h2")?.textContent).toBe("system fixture");
    expect(document.querySelector(".vk-request-context-origins")?.textContent).toContain("Fixture VMM");
    expect(document.querySelector(".vk-request-context-list")?.textContent).not.toContain("fixture_tool");
    const toolsKey = readLocaleMessage("en-US", "skills.action.tools");
    const tools = [...document.querySelectorAll<HTMLButtonElement>(".vk-request-context-tabs button")]
      .find((button) => button.textContent === toolsKey)!;
    await act(async () => tools.click());
    expect(document.querySelector(".vk-request-context-detail")?.textContent).toContain("fixture_tool");
    expect(document.querySelector(".vk-request-context-detail pre")).toBeNull();
    // Results render model output while keeping the provider envelope available under Raw.
    // 工具返回呈现给模型的输出，并在原始内容里保留供应商信封。
    const result = document.querySelector<HTMLButtonElement>('.vk-request-context-list [data-tone="tool"]')!;
    expect(result.textContent).toContain("file_read");
    await act(async () => result.click());
    expect(document.querySelector(".vk-request-context-tool-part .vk-md h2")?.textContent).toBe("returned");
    await act(async () => { await vi.runOnlyPendingTimersAsync(); });
    const response = [...document.querySelectorAll<HTMLButtonElement>(".vk-request-context-list button")]
      .find((button) => button.dataset.tone === "assistant")!;
    await act(async () => response.click());
    expect(document.querySelector(".vk-request-context-detail .vk-md h2")?.textContent).toBe("finished reply");
    expect(document.querySelector(".vk-request-context-reasoning .vk-md h3")?.textContent).toBe("thinking before the reply");
    const raw = [...document.querySelectorAll<HTMLButtonElement>(".vk-request-context-tabs button")]
      .find((button) => button.textContent === readLocaleMessage("en-US", "requestContext.raw"))!;
    await act(async () => raw.click());
    expect(document.querySelector(".vk-request-context-reasoning pre")?.textContent).toBe("### thinking before the reply");
    expect(document.querySelector(".vk-request-context-text pre")?.textContent).toBe("## finished reply");
    await act(async () => { await vi.runOnlyPendingTimersAsync(); });
    expect(request).toHaveBeenLastCalledWith("service_latest_request_context_v1", {
      sessionId: "fixture-session", knownExecutionId: "execution-c", knownAssistantResponse: true,
    });
  } finally {
    await act(async () => root.unmount());
    await vi.runOnlyPendingTimersAsync();
    host.remove();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});

it.each(["actual", "simulated"])("classifies an injected envelope in the %s view and separates metadata from Markdown", async (view) => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const request = vi.mocked(serviceClient.request);
  request.mockReset();
  // Both fixtures contain the exact serialized V1 envelope and only differ in storage shape.
  // 两个夹具包含精确序列化的 V1 信封，仅存储结构不同。
  const envelope = '<vulcan_context_update version="1" event_id="v1-context-update-3-dynamic.html_answers" kind="context_update" source="host" target="model" scope="session">## Context &amp; mode</vulcan_context_update>';
  const actual = snapshot("execution-d", "user prompt");
  actual.snapshot.nodes.push({ field: "input", index: 1, role: "user", kind: "message", text: envelope, content: JSON.stringify({ content: envelope }), truncated: false });
  const simulated = { nodeCount: 1, nodes: [
    { field: "conversation", index: 0, role: null, kind: "user_input", text: envelope, content: JSON.stringify({ text: envelope }), truncated: false },
  ] };
  if (view === "actual") request.mockResolvedValueOnce(actual);
  else request.mockResolvedValueOnce({ status: "empty" }).mockResolvedValueOnce(simulated);
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  try {
    await act(async () => root.render(<I18nProvider initialLocale="en-US"><RequestContextDialog sessionId="fixture-session" onClose={vi.fn()} /></I18nProvider>));
    const row = document.querySelector<HTMLButtonElement>('.vk-request-context-row[data-tone="injection"]')!;
    expect(row.textContent).toContain(readLocaleMessage("en-US", "requestContext.answerMode"));
    expect(row.textContent).not.toContain("<vulcan_context_update");
    await act(async () => row.click());
    expect(document.querySelector(".vk-request-context-metadata")?.textContent).toContain("v1-context-update-3-dynamic.html_answers");
    expect(document.querySelector(".vk-request-context-detail .vk-md h2")?.textContent).toBe("Context & mode");
  } finally {
    await act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  }
});
