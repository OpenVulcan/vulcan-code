// @vitest-environment jsdom
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ConversationToolResultTabs, toolResultMode } from "./conversation-tool-result-tabs";
import { ConversationModelParameters } from "./conversation-model-parameters";
import { ToolPresentationCard } from "./conversation-tool-presentation-card";
import { modelExtensionToolTitleKey } from "./tool-presentations/model-extension-tool-presentation";

// event mirrors the exact backend model-extension-parameters response contract.
// event 对应后端模型扩展参数响应的精确契约。
const event: ToolCallEvent = { kind: "tool_call", api_call_id: 1, tool_call_id: 7, tool_name: "get_model_extension_parameters", request_stream_id: "parameters", status: "completed", input: { capability: "speech_synthesize" }, output: { model_output: { kind: "model_extension_parameters", capability: "speech_synthesize", models: [{ model: "test/speech", display_name: "Speech HD", description: "High quality speech.", input_modes: ["text"], characteristics: ["Stable voice"], constraints: ["Match the declared schema."], limitations: [], evidence_refs: [], resolution_policy: null, duration_policy: null, format_policy: null, args_schema: { type: "object", properties: { text: { type: "string", description: "Text to speak." }, speed: { type: "number", default: 1 }, format: { type: "string", enum: ["mp3", "wav"] } }, required: ["text"], additionalProperties: false, allOf: [{ if: { properties: { format: { const: "wav" } } }, then: { required: ["speed"] } }] } }], instruction: "Use the exact model id." } } };

describe("model parameters card", () => {
  it("renders model identity and useful parameter rows directly with exact schema retained", () => {
    // host allows independent checks of visible parameter rows and the full schema disclosure.
    // host 支持独立检查可见参数行与完整模式展开区。
    const host = document.createElement("div");
    host.innerHTML = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationToolResultTabs event={event}><div>unexpected deferred body</div></ConversationToolResultTabs></I18nProvider>);
    expect(toolResultMode(event)).toBe("direct");
    expect(host.textContent).toContain("Speech HD");
    expect(host.textContent).toContain("test/speech");
    expect(host.querySelectorAll(".tool-model-parameter")).toHaveLength(3);
    expect(host.querySelectorAll(".tool-model-required")).toHaveLength(1);
    expect(host.querySelector(".tool-model-default")?.textContent).toContain("1");
    expect(host.querySelector(".tool-model-schema")?.textContent).toContain('"allOf"');
    expect(host.querySelector(".tool-model-schema")?.textContent).toContain('"additionalProperties": false');
    expect(host.querySelector('[role="tab"]')).toBeNull();
    expect(host.textContent).not.toContain("unexpected deferred body");
  });
  it("renders the actual expanded tool card synchronously without a loading placeholder", () => {
    // markup exercises the production dispatcher rather than just the specialized body.
    // markup 验证生产派发器，而不仅是专属正文。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ToolPresentationCard expanded expandedToolPresentationIds={new Set()} detailLoadEnabled={false}
      presentation={{ id: "parameters", kind: "generic_tool_markdown", state: "completed", markdown: "", toolName: event.tool_name, titleKey: modelExtensionToolTitleKey(event.tool_name)! }} v1ToolEvent={event}
      requestProgress={null} onExpandedChange={() => undefined} onLoadToolDetail={async () => null}
    /></I18nProvider>);
    expect(markup).toContain("tool-model-card");
    expect(markup).not.toContain("正在加载输出");
    expect(markup).not.toContain('role="tab"');
  });
  it("keeps empty and invalid responses explicit", () => {
    expect(renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationModelParameters event={{ ...event, output: { model_output: { models: [] } } }} /></I18nProvider>)).toContain("当前没有可用模型");
    expect(renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationModelParameters event={{ ...event, output: { model_output: { models: [{}] } } }} /></I18nProvider>)).toContain("模型参数返回格式无效");
  });
  it("preserves an exact tool failure instead of displaying an empty result", () => {
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationModelParameters event={{ ...event, status: "failed", output: { model_output: "Model disabled", host_result: { kind: "tool_error", payload: { error: "Model disabled" } } } }} /></I18nProvider>);
    expect(markup).toContain('role="alert"');
    expect(markup).toContain("Model disabled");
  });
});
