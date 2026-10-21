// @vitest-environment jsdom

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
  it.each([false, true])("preserves webpage and encoded local page hyperlinks (streaming=%s)", (streaming) => {
    // host parses actual rendered anchors, including the sanitizer and URL transform stages.
    // host 解析实际渲染的链接，包含清理器与 URL 转换阶段。
    const host = document.createElement("div");
    host.innerHTML = renderToStaticMarkup(<MarkdownMessage eager streaming={streaming} content={[
      "[Website](https://example.com/page?q=1#demo)",
      "",
      "[Local](file:///C:/my%20files/page.html#demo)",
      "",
      "[POSIX](file:///tmp/page.html)",
      "",
      "[Spaced](<file:///C:/my files/page.html>)",
      "",
      "Done.\n\n",
    ].join("\n")} />);
    expect(Array.from(host.querySelectorAll("a"), (link) => link.getAttribute("href"))).toEqual([
      "https://example.com/page?q=1#demo", "file:///C:/my%20files/page.html#demo",
      "file:///tmp/page.html", "file:///C:/my%20files/page.html",
    ]);
    expect(Array.from(host.querySelectorAll("a"), (link) => link.target)).toEqual(Array(4).fill("_blank"));
    expect(host.querySelectorAll('a > svg.vk-md-link-icon[aria-hidden="true"]')).toHaveLength(4);
    expect(Array.from(host.querySelectorAll("a"), (link) => link.textContent)).toEqual(["Website", "Local", "POSIX", "Spaced"]);
  });

  it.each(["javascript:alert%281%29", "data:text/html,hello", "vbscript:evil", "vulcan://private"])("does not turn %s into a navigable link", (url) => {
    // host verifies the rendered URL rather than only the parser input.
    // host 验证最终渲染的 URL，而不只是解析器输入。
    const host = document.createElement("div");
    host.innerHTML = renderToStaticMarkup(<MarkdownMessage eager content={`[Unsafe](${url})`} />);
    expect(host.querySelector("a")?.getAttribute("href") ?? "").toBe("");
  });

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
