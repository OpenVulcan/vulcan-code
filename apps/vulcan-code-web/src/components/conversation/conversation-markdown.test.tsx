import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { MarkdownMessage } from "./conversation-markdown";

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
});
