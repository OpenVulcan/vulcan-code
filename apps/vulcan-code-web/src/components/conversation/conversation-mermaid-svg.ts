// serializeMermaidSvg converts Mermaid's browser markup into standalone SVG XML.
// serializeMermaidSvg 将 Mermaid 的浏览器标记转换为独立 SVG XML。
// Parameters: markup is the SVG returned by Mermaid in strict security mode.
// 参数：markup 是 Mermaid 在严格安全模式下返回的 SVG。
// Returns XML suitable for an image Blob; throws when the SVG root is absent.
// 返回适用于图片 Blob 的 XML；缺失 SVG 根元素时抛出错误。
export function serializeMermaidSvg(markup: string): string {
  // template parses HTML labels inertly, without mounting them in the application.
  // template 惰性解析 HTML 标签，不将它们挂载到应用页面。
  const template = document.createElement("template");
  template.innerHTML = markup;
  // svg is the single root produced by Mermaid, including its XHTML foreignObject labels.
  // svg 是 Mermaid 生成的唯一根元素，包含 XHTML foreignObject 标签。
  const svg = template.content.firstElementChild;
  if (template.content.children.length !== 1
    || svg?.localName !== "svg"
    || svg.namespaceURI !== "http://www.w3.org/2000/svg") {
    throw new Error("Mermaid did not return an SVG root.");
  }
  // HTML parsing leaves XHTML xmlns attributes unnamespaced; let XMLSerializer declare the DOM namespace.
  // HTML 解析会让 XHTML 的 xmlns 属性缺少命名空间；由 XMLSerializer 声明 DOM 命名空间。
  // Keeping both can produce duplicate xmlns attributes in standards-based XML serializers.
  // 同时保留两者可能让标准 XML 序列化器生成重复 xmlns 属性。
  for (const element of svg.querySelectorAll("[xmlns]")) {
    // declaration is redundant only when it exactly matches the parsed element namespace.
    // declaration 只有与解析元素的命名空间完全一致时才属于冗余声明。
    const declaration = element.getAttributeNode("xmlns");
    if (declaration?.namespaceURI === null && declaration.value === element.namespaceURI) {
      element.removeAttributeNode(declaration);
    }
  }
  // viewBox defines Mermaid's drawing units; explicit dimensions prevent the browser's 300x150 default.
  // viewBox 定义 Mermaid 绘图单位；显式尺寸避免浏览器使用 300x150 默认大小。
  // This gives the shared viewer meaningful original-size and zoom limits for large diagrams.
  // 这让共享查看器对大型图表拥有合理的原始尺寸与缩放上限。
  const viewBox = svg.getAttribute("viewBox");
  if (viewBox !== null) {
    // dimensions follow SVG's four-number viewBox grammar, including comma separators.
    // dimensions 遵循 SVG 四个数字的 viewBox 语法，包含逗号分隔符。
    const dimensions = viewBox.trim().split(/[\s,]+/u).map(Number);
    if (dimensions.length !== 4 || !dimensions.every(Number.isFinite)
      || dimensions[2] <= 0 || dimensions[3] <= 0) {
      throw new Error("Mermaid returned an invalid SVG viewBox.");
    }
    svg.setAttribute("width", String(dimensions[2]));
    svg.setAttribute("height", String(dimensions[3]));
  }
  // XML serialization closes HTML void elements such as <br>; raw Mermaid markup does not.
  // XML 序列化会闭合 <br> 等 HTML 空元素；Mermaid 原始标记不会保证这一点。
  return new XMLSerializer().serializeToString(svg);
}
