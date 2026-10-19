// @vitest-environment jsdom
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ToolCallCard } from "../tool-call-card";
import { MediaResultView } from "./media-result-view";
import { ModelParametersView } from "./model-parameters-view";

/** Builds a resource envelope matching render_provider_resource_result, including its private host path.
 * 构造符合 render_provider_resource_result 的资源信封，包含私有宿主路径。 */
function mediaEvent(name = "speech_synthesize"): ToolCallEvent {
  return { kind: "tool_call", api_call_id: 1, tool_call_id: 7, request_stream_id: "media", tool_name: name, input: { model: "test/native", args: {} }, status: "completed", output: {
    model_output: { resources: [{ asset_id: "asset-1", resource_path: "attachment://asset-1", file_name: "generated.mp3", kind: "audio", mime_type: "audio/mpeg", byte_len: 1635252 }] },
    host_result: { kind: "generated_attachments", payload: { outputs: [{ asset_id: "asset-1", local_path: "D:\\workspace\\.vulcan\\code\\attachments\\audio\\generated.mp3" }] } },
  } };
}

// parametersEvent mirrors the exact backend model-extension-parameters response contract.
// parametersEvent 对应后端模型扩展参数响应的精确契约。
const parametersEvent: ToolCallEvent = { kind: "tool_call", api_call_id: 1, tool_call_id: 7, tool_name: "get_model_extension_parameters", request_stream_id: "parameters", status: "completed", input: { capability: "speech_synthesize" }, output: { model_output: { kind: "model_extension_parameters", capability: "speech_synthesize", models: [{ model: "test/speech", display_name: "Speech HD", description: "High quality speech.", input_modes: ["text"], characteristics: ["Stable voice"], constraints: ["Match the declared schema."], limitations: [], evidence_refs: [], resolution_policy: null, duration_policy: null, format_policy: null, args_schema: { type: "object", properties: { text: { type: "string", description: "Text to speak." }, speed: { type: "number", default: 1 }, format: { type: "string", enum: ["mp3", "wav"] } }, required: ["text"], additionalProperties: false, allOf: [{ if: { properties: { format: { const: "wav" } } }, then: { required: ["speed"] } }] } }], instruction: "Use the exact model id." } } };

/** Renders one node into a detached host so assertions can query its structure.
 * 将节点渲染到独立宿主中，便于断言查询其结构。 */
function mount(node: React.ReactElement): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = renderToStaticMarkup(<I18nProvider initialLocale="en-US">{node}</I18nProvider>);
  return host;
}

describe("media generation result", () => {
  it.each(["speech_synthesize", "video_generate", "image_generate", "image_edit", "video_edit", "video_extend", "music_generate", "music_cover"])("shows %s resources inside the open provider card with the host path", (name) => {
    const event = mediaEvent(name);
    const before = JSON.stringify(event);
    const host = mount(<ToolCallCard event={event} onOpenChange={() => undefined} open />);
    const result = host.querySelector("[data-media-generation-result]");
    expect(result).not.toBeNull();
    expect(result!.querySelectorAll("[data-media-resource]")).toHaveLength(1);
    expect(result!.querySelector(".vk-media-path > code")?.textContent).toBe("D:\\workspace\\.vulcan\\code\\attachments\\audio\\generated.mp3");
    expect(result!.querySelector(".vk-media-head > span")?.textContent).toContain("1.6 MiB");
    expect(JSON.stringify(event)).toBe(before);
    expect(JSON.stringify(event.output?.model_output)).not.toContain("D:");
  });

  it("matches host paths by asset identity instead of position, name or public locator", () => {
    const event = mediaEvent();
    event.output!.host_result = { kind: "generated_attachments", payload: { outputs: [{ asset_id: "foreign", local_path: "D:/wrong.mp3" }, { asset_id: "asset-1", local_path: "D:/correct.mp3" }] } };
    const host = mount(<MediaResultView event={event} />);
    expect(host.querySelector(".vk-media-path > code")?.textContent).toBe("D:/correct.mp3");
    expect(host.textContent).not.toContain("D:/wrong.mp3");
  });

  it("never manufactures a local path for missing or unrelated host results", () => {
    for (const hostResult of [undefined, { kind: "provider_tool_output", payload: { outputs: [{ asset_id: "asset-1", local_path: "D:/untrusted.mp3" }] } }]) {
      const event = mediaEvent();
      if (hostResult === undefined) delete event.output!.host_result;
      else event.output!.host_result = hostResult;
      const host = mount(<MediaResultView event={event} />);
      expect(host.querySelector(".vk-media-path > code")).toBeNull();
      expect(host.querySelector(".vk-media-path > em")).not.toBeNull();
      expect(host.textContent).not.toContain("D:/untrusted.mp3");
    }
  });

  it("keeps composite text and execution failures alongside generated resources", () => {
    const event = mediaEvent();
    event.output!.model_output = { kind: "composite", composite: { text: "Exact caption", resources: [{ asset_id: "asset-1", file_name: "generated.mp3" }], embeddings: [], reranks: [] } };
    expect(mount(<MediaResultView event={event} />).textContent).toContain("Exact caption");
    event.status = "failed";
    event.output!.host_result = { kind: "tool_error", payload: { error: "Exact provider error" } };
    const host = mount(<MediaResultView event={event} />);
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("Exact provider error");
  });

  it("distinguishes pending, empty and non-resource results", () => {
    const event = mediaEvent();
    const pending = mount(<MediaResultView event={{ ...event, status: "running", output: undefined }} />);
    const empty = mount(<MediaResultView event={{ ...event, output: { model_output: { resources: [] } } }} />);
    expect(pending.querySelector("[data-media-resource]")).toBeNull();
    expect(empty.querySelector("[data-media-resource]")).toBeNull();
    expect(pending.querySelector(".vk-tool-note")?.textContent).not.toBe(empty.querySelector(".vk-tool-note")?.textContent);
    expect(mount(<MediaResultView event={{ ...event, output: { model_output: "Exact response" } }} />).textContent).toContain("Exact response");
  });
});

describe("model parameters result", () => {
  it("lists model identity and parameter rows inside the open provider card, keeping the exact schema", () => {
    const host = mount(<ToolCallCard event={parametersEvent} onOpenChange={() => undefined} open />);
    const result = host.querySelector("[data-model-parameters]");
    expect(result).not.toBeNull();
    expect(result!.querySelector(".vk-model-head")?.textContent).toContain("Speech HD");
    expect(result!.querySelector(".vk-model-head > code")?.textContent).toBe("test/speech");
    expect(result!.querySelectorAll(".vk-param")).toHaveLength(3);
    expect(result!.querySelectorAll(".vk-param-req[data-required]")).toHaveLength(1);
    expect(result!.querySelector(".vk-param-default > code")?.textContent).toBe("1");
    const schema = [...result!.querySelectorAll(".vk-model-more .vk-json")].map((node) => node.textContent).join("\n");
    expect(schema).toContain('"allOf"');
    expect(schema).toContain('"additionalProperties": false');
  });

  it("keeps empty and invalid responses explicit and distinct", () => {
    const empty = mount(<ModelParametersView event={{ ...parametersEvent, output: { model_output: { models: [] } } }} />);
    const invalid = mount(<ModelParametersView event={{ ...parametersEvent, output: { model_output: { models: [{}] } } }} />);
    expect(empty.querySelector(".vk-model")).toBeNull();
    expect(empty.querySelector(".vk-tool-note")).not.toBeNull();
    expect(invalid.querySelector('[role="alert"]')).not.toBeNull();
  });

  it("shows an exact tool failure instead of an empty result", () => {
    const host = mount(<ModelParametersView event={{ ...parametersEvent, status: "failed", output: { model_output: "Model disabled", host_result: { kind: "tool_error", payload: { error: "Model disabled" } } } }} />);
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("Model disabled");
  });
});
