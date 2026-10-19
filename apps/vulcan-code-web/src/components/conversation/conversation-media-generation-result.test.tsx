// @vitest-environment jsdom
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ConversationMediaGenerationResult } from "./conversation-media-generation-result";
import { ToolPresentationCard } from "./conversation-tool-presentation-card";
import { modelExtensionToolTitleKey } from "./tool-presentations/model-extension-tool-presentation";

/** Builds a resource envelope matching render_provider_resource_result, including its private host path.
 * 构造符合 render_provider_resource_result 的资源信封，包含私有宿主路径。
 * Parameter name selects the tool; returns a complete event without exposing the host path in model_output.
 * 参数 name 选择工具；返回完整事件，不向 model_output 暴露宿主路径。
 */
function mediaEvent(name = "speech_synthesize"): ToolCallEvent {
  return { kind: "tool_call", api_call_id: 1, tool_call_id: 7, request_stream_id: "media", tool_name: name, input: { model: "test/native", args: {} }, status: "completed", output: {
    model_output: { resources: [{ asset_id: "asset-1", resource_path: "attachment://asset-1", file_name: "generated.mp3", kind: "audio", mime_type: "audio/mpeg", byte_len: 1635252 }] },
    host_result: { kind: "generated_attachments", payload: { outputs: [{ asset_id: "asset-1", local_path: "D:\\workspace\\.vulcan\\code\\attachments\\audio\\generated.mp3" }] } },
  } };
}

/** Renders a result card under the Chinese locale whose labels these assertions read.
 * 在断言读取的中文语言环境下渲染结果卡片。
 * Parameter node is the element to render; returns static markup.
 * 参数 node 为待渲染元素；返回静态标记。
 */
function renderZh(node: React.ReactElement): string {
  return renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">{node}</I18nProvider>);
}

describe("eager media generation results", () => {
  it.each(["speech_synthesize", "video_generate", "image_generate", "image_edit", "video_edit", "video_extend", "music_generate", "music_cover"])("renders %s immediately in the tool card with a host-owned local path", name => {
    // event must reach the production tool card unchanged.
    // event 必须原样进入生产工具卡。
    const event = mediaEvent(name);
    const before = JSON.stringify(event);
    const cards = [<ToolPresentationCard expanded expandedToolPresentationIds={new Set()} detailLoadEnabled={false}
      presentation={{ id: "media", kind: "generic_tool_markdown", state: "completed", markdown: "", toolName: name, titleKey: modelExtensionToolTitleKey(name)! }} v1ToolEvent={event}
      requestProgress={null} onExpandedChange={() => undefined} onLoadToolDetail={async () => null} />];
    for (const card of cards) {
      // host inspects the visible result separately from hidden raw input/output panels.
      // host 将可见结果与隐藏的原始输入输出面板分开检查。
      const host = document.createElement("div");
      host.innerHTML = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><TooltipProvider>{card}</TooltipProvider></I18nProvider>);
      expect(Array.from(host.querySelectorAll('[role="tab"]'), tab => tab.textContent)).toEqual(["生成结果", "输入", "输出"]);
      expect(host.querySelector('[data-media-generation-result]')?.textContent).toContain("1.6 MiB");
      expect(host.querySelector('[data-media-generation-result]')?.textContent).toContain("本地路径D:\\workspace\\.vulcan\\code\\attachments\\audio\\generated.mp3");
      expect(host.querySelectorAll(".tool-media-resource")).toHaveLength(1);
      expect(host.textContent).not.toContain("正在加载输出");
    }
    expect(JSON.stringify(event)).toBe(before);
    expect(JSON.stringify(event.output?.model_output)).not.toContain("D:");
  });

  it("matches host paths by asset identity instead of position, name or public locator", () => {
    // event intentionally puts an unrelated host resource first.
    // event 刻意把无关宿主资源放在首位。
    const event = mediaEvent();
    event.output!.host_result = { kind: "generated_attachments", payload: { outputs: [{ asset_id: "foreign", local_path: "D:/wrong.mp3" }, { asset_id: "asset-1", local_path: "D:/correct.mp3" }] } };
    const markup = renderZh(<ConversationMediaGenerationResult event={event} />);
    expect(markup).toContain("D:/correct.mp3");
    expect(markup).not.toContain("D:/wrong.mp3");
  });

  it("never manufactures a local path for missing or unrelated host results", () => {
    for (const hostResult of [undefined, { kind: "provider_tool_output", payload: { outputs: [{ asset_id: "asset-1", local_path: "D:/untrusted.mp3" }] } }]) {
      const event = mediaEvent();
      if (hostResult === undefined) delete event.output!.host_result;
      else event.output!.host_result = hostResult;
      const markup = renderZh(<ConversationMediaGenerationResult event={event} />);
      expect(markup).toContain("未提供本地路径");
      expect(markup).not.toContain("D:/untrusted.mp3");
    }
  });

  it("preserves composite text and execution failures alongside generated resources", () => {
    const event = mediaEvent();
    event.output!.model_output = { kind: "composite", composite: { text: "Exact caption", resources: [{ asset_id: "asset-1", file_name: "generated.mp3" }], embeddings: [], reranks: [] } };
    expect(renderZh(<ConversationMediaGenerationResult event={event} />)).toContain("Exact caption");
    event.status = "failed";
    event.output!.host_result = { kind: "tool_error", payload: { error: "Exact provider error" } };
    const markup = renderZh(<ConversationMediaGenerationResult event={event} />);
    expect(markup).toContain('role="alert"');
    expect(markup).toContain("Exact provider error");
  });

  it("distinguishes pending, empty and non-resource results without a module-loading placeholder", () => {
    const event = mediaEvent();
    expect(renderZh(<ConversationMediaGenerationResult event={{ ...event, status: "running", output: undefined }} />)).toContain("尚无生成结果");
    expect(renderZh(<ConversationMediaGenerationResult event={{ ...event, output: { model_output: { resources: [] } } }} />)).toContain("未返回生成资源");
    expect(renderZh(<ConversationMediaGenerationResult event={{ ...event, output: { model_output: "Exact response" } }} />)).toContain("Exact response");
  });
});
