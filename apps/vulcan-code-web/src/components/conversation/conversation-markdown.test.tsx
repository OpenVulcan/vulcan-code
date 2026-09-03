// @vitest-environment jsdom

import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { encodeBase64UrlUtf8 } from "@/resources/localMedia";
import {
  MarkdownImageWorkspacePathProvider,
  MarkdownMessage,
} from "./conversation-markdown";

// This regression test verifies that the static Markdown renderer emits semantic block elements.
// 该回归测试验证静态 Markdown 渲染器会输出语义化块级元素。
describe("MarkdownMessage", () => {
  it("renders headings and unordered lists from standard Markdown", () => {
    // markup captures the server-rendered HTML used to verify Markdown structure.
    // markup 捕获服务端渲染出的 HTML，用于验证 Markdown 结构。
    const markup = renderToStaticMarkup(
      <MarkdownMessage
        content={[
          "# 一级标题",
          "",
          "## 二级标题",
          "",
          "### 三级标题",
          "",
          "- 横线项目",
          "* 星号项目",
        ].join("\n")}
      />,
    );

    expect(markup).toContain("<h1>一级标题</h1>");
    expect(markup).toContain("<h2>二级标题</h2>");
    expect(markup).toContain("<h3>三级标题</h3>");
    expect(markup).toContain("<ul>");
    expect(markup).toContain("<li>横线项目</li>");
    expect(markup).toContain("<li>星号项目</li>");
  });

  it("resolves generated relative image paths through the absolute workspace asset route", () => {
    // browserStorage keeps service discovery deterministic in the isolated jsdom runtime.
    // browserStorage 让服务发现逻辑在隔离的 jsdom 运行时中保持确定性。
    const browserStorage = { getItem: () => null };
    Object.defineProperty(window, "localStorage", { configurable: true, value: browserStorage });
    Object.defineProperty(window, "sessionStorage", { configurable: true, value: browserStorage });
    // workspacePath is the exact project root owned by the active conversation.
    // workspacePath 是当前会话拥有的精确项目根目录。
    const workspacePath = "D:\\openvulcan\\vulcan-code";
    // relativeImagePath is the model-visible generated attachment path.
    // relativeImagePath 是模型可见的生成附件相对路径。
    const relativeImagePath = ".vulcan/code/attachments/images/generated-image.jpg";
    // absoluteImagePath is the path that must enter the browser asset endpoint.
    // absoluteImagePath 是必须进入浏览器资源接口的绝对路径。
    const absoluteImagePath = `${workspacePath}\\.vulcan\\code\\attachments\\images\\generated-image.jpg`;
    // markup captures the final image URL after workspace-relative resolution.
    // markup 捕获相对工作区解析后的最终图片 URL。
    const markup = renderToStaticMarkup(
      <MarkdownImageWorkspacePathProvider value={workspacePath}>
        <MarkdownMessage content={`![generated](${relativeImagePath})`} eager />
      </MarkdownImageWorkspacePathProvider>,
    );

    expect(markup).toContain(encodeBase64UrlUtf8(absoluteImagePath));
    expect(markup).not.toContain(encodeBase64UrlUtf8(relativeImagePath));
  });
});
