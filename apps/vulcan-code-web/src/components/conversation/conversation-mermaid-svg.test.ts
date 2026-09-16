// @vitest-environment jsdom
import { expect, it } from "vitest";
import { serializeMermaidSvg } from "./conversation-mermaid-svg";

it("repairs Mermaid HTML line breaks for standalone XML images without losing labels", () => {
  // markup reproduces Mermaid 11.16 output for a node containing the user's <br/> syntax.
  // markup 复现 Mermaid 11.16 对用户节点中 <br/> 语法生成的输出。
  const markup = '<svg xmlns="http://www.w3.org/2000/svg"><foreignObject width="200" height="48"><div xmlns="http://www.w3.org/1999/xhtml"><p>conversation_submission.rs:2274<br>service_start_conversation_execution &amp; 中文</p></div></foreignObject></svg>';
  // parser checks the exact XML contract used by SVG image decoding.
  // parser 检查 SVG 图片解码使用的精确 XML 契约。
  const parser = new DOMParser();
  expect(parser.parseFromString(markup, "image/svg+xml").querySelector("parsererror")).not.toBeNull();
  // document verifies both XML validity and namespace-preserving label content.
  // document 同时验证 XML 有效性与保留命名空间的标签内容。
  const document = parser.parseFromString(serializeMermaidSvg(markup), "image/svg+xml");
  expect(document.querySelector("parsererror")).toBeNull();
  expect(document.querySelector("br")?.namespaceURI).toBe("http://www.w3.org/1999/xhtml");
  expect(document.querySelector("p")?.textContent).toBe("conversation_submission.rs:2274service_start_conversation_execution & 中文");
});

it("preserves SVG geometry, styles, markers and text when serializing", () => {
  // markup exercises SVG-native elements as well as HTML entities in foreignObject labels.
  // markup 同时覆盖 SVG 原生元素和 foreignObject 标签中的 HTML 实体。
  const markup = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 80"><style>.node { fill: red; }</style><defs><marker id="arrow" /></defs><path d="M0 0 L20 20" marker-end="url(#arrow)"/><text>中文 &amp; A</text><foreignObject><div xmlns="http://www.w3.org/1999/xhtml">A&nbsp;B</div></foreignObject></svg>';
  // document contains the serialized image tree, rather than an implementation string snapshot.
  // document 包含序列化后的图片树，而不是实现字符串快照。
  const document = new DOMParser().parseFromString(serializeMermaidSvg(markup), "image/svg+xml");
  expect(document.querySelector("parsererror")).toBeNull();
  expect(document.documentElement.getAttribute("viewBox")).toBe("0 0 200 80");
  expect(document.documentElement.getAttribute("width")).toBe("200");
  expect(document.documentElement.getAttribute("height")).toBe("80");
  expect(document.querySelector("style")?.textContent).toBe(".node { fill: red; }");
  expect(document.querySelector("path")?.getAttribute("marker-end")).toBe("url(#arrow)");
  expect(document.querySelector("marker")?.namespaceURI).toBe("http://www.w3.org/2000/svg");
  expect(document.querySelector("text")?.textContent).toBe("中文 & A");
  expect(document.querySelector("div")?.textContent).toBe("A\u00a0B");
});

it("uses large viewBox dimensions instead of percentage dimensions for image zoom", () => {
  // document verifies real diagram units even when Mermaid emits responsive percentage sizing.
  // document 验证 Mermaid 输出响应式百分比尺寸时仍保留真实图表单位。
  const document = new DOMParser().parseFromString(serializeMermaidSvg(
    '<svg xmlns="http://www.w3.org/2000/svg" width="100%" viewBox="-10, -20, 4200, 3100"></svg>',
  ), "image/svg+xml");
  expect(document.documentElement.getAttribute("width")).toBe("4200");
  expect(document.documentElement.getAttribute("height")).toBe("3100");
  expect(document.documentElement.getAttribute("viewBox")).toBe("-10, -20, 4200, 3100");
});

it.each(["", "<div>not SVG</div>", "<svg></svg><div>extra root</div>"])("rejects invalid diagram roots: %s", (markup) => {
  expect(() => serializeMermaidSvg(markup)).toThrow("Mermaid did not return an SVG root.");
});
