// @vitest-environment jsdom
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ToolPresentationCard } from "../conversation-tool-presentation-card";
import { resolveToolPresentation } from "../conversation-tool-presentations";
import type { ToolPresentationDisplayState } from "./types";

/** Projects canonical `event` at `state` through the real card router; returns markup with expansion explicitly requested.
 * 将规范`event`按`state`经过真实卡片路由投影；返回明确请求展开时的标记。
 */
function renderRead(event: ToolCallEvent, state: ToolPresentationDisplayState): string {
  // These fields match the V1 transcript's toolMetadata projection, including its exact arguments owner.
  // 这些字段匹配V1转录的toolMetadata投影，包括精确参数归属。
  const presentation = resolveToolPresentation([{
    id: "media-read-event", kind: "tool-call", toolName: event.tool_name, toolCallId: "read-1",
    title: event.tool_name, summary: "", body: "",
    rawMetadata: { v1_event: true, v1_tool_event: event, arguments: event.input },
  }], state);
  return renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
    <ToolPresentationCard presentation={presentation} v1ToolEvent={event} expanded
      expandedToolPresentationIds={new Set()} requestProgress={null} detailLoadEnabled={false}
      onExpandedChange={() => undefined} onLoadToolDetail={async () => null} />
  </I18nProvider>);
}

describe("read_media compact receipt", () => {
  it.each(["photo.png", "recording.wav", "clip.mp4", "report.pdf"])("only identifies the read resource %s", fileName => {
    // Owned locators must use the returned resource name rather than a private provider reference or prose guess.
    // 有归属定位符必须使用返回的资源名称，不能使用供应商私有引用或猜测说明文字。
    const event: ToolCallEvent = {
      kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: "read_media", status: "completed", request_stream_id: "read-1",
      input: { path: "project-resource:7" },
      output: { model_output: JSON.stringify({ $vulcan_native_media_input: 1,
        text: "Read media: not-the-resource-name. Automatically converted from image/tiff.",
        resource: { file_name: fileName, url: "data:image/png;base64,cHJpdmF0ZQ==", id: "private-provider-id" },
      }) },
    };
    // Even global expand-all must retain the one-row receipt and keep media data out of rendered markup.
    // 即使全局全部展开也必须保留单行回执，媒体数据不得进入渲染标记。
    const html = renderRead(event, "completed");
    expect(html).toContain("已读取");
    expect(html).toContain(fileName);
    expect(html).toContain('title="project-resource:7"');
    expect(html).not.toMatch(/<button|<img|<audio|<video|<pre|aria-expanded|role="tab"/);
    expect(html).not.toMatch(/base64|private-provider-id|not-the-resource-name|Automatically converted|read_media/);
  });

  it.each([
    ["running", "读取中"], ["failed", "读取失败"], ["stopped", "已停止"], ["unknown", "读取"],
  ] as const)("does not claim success for %s", (state, label) => {
    // The original path remains visible even when conversion fails before a native resource envelope exists.
    // 转换在原生资源信封生成前失败时，原始路径依然可见。
    const event: ToolCallEvent = {
      kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: "read_media",
      status: state === "unknown" ? "prepared" : state, request_stream_id: "read-1",
      input: { path: "D:\\photos\\原图.tiff" },
      output: state === "failed" ? { model_output: "Image conversion failed: invalid TIFF." } : undefined,
    };
    // Failure details stay available on hover while every state keeps a compact resource row.
    // 失败详情保留在悬停提示中，各状态始终保持紧凑资源行。
    const html = renderRead(event, state);
    expect(html).toContain(label);
    expect(html).toContain("原图.tiff");
    expect(html).not.toContain("已读取");
    if (state === "failed") expect(html).toContain('title="Image conversion failed: invalid TIFF."');
  });
});
