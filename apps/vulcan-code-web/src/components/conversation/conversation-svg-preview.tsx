import * as React from "react";

import { MarkdownDiagramFrame } from "@/components/conversation/conversation-diagram-frame";

// SVG_PREVIEW_SOURCE_CHARACTER_LIMIT bounds untrusted transcript parsing and image decoding work.
// SVG_PREVIEW_SOURCE_CHARACTER_LIMIT 限制不可信会话内容的解析与图片解码工作量。
const SVG_PREVIEW_SOURCE_CHARACTER_LIMIT = 500_000;

// SvgPreviewProps describes one direct SVG source preview and its code fallback.
// SvgPreviewProps 描述一个直接 SVG 源码预览及其代码回退内容。
interface SvgPreviewProps {
  // Copies the complete SVG source through the shared Markdown clipboard path.
  // 通过共享 Markdown 剪贴板路径复制完整 SVG 源码。
  copySource: () => Promise<void>;
  // Markdown code-block fallback rendered when the SVG source is invalid.
  // SVG 源码无效时渲染的 Markdown 代码块回退内容。
  fallback: React.ReactNode;
  // Complete SVG source extracted from the fenced Markdown block.
  // 从 Markdown 围栏代码块提取的完整 SVG 源码。
  source: string;
}

// SvgPreviewState describes direct SVG Blob preparation and image loading state.
// SvgPreviewState 描述直接 SVG Blob 准备与图片加载状态。
type SvgPreviewState =
  | { status: "loading" }
  | { status: "ready"; imageUrl: string }
  | { status: "failed" };

// SvgPreview renders finalized SVG source through an isolated browser image boundary.
// SvgPreview 通过隔离的浏览器图片边界渲染最终确定的 SVG 源码。
//
// Parameters:
// 参数：
// - props: SVG source, copy callback, and ordinary code-block fallback.
// - props：SVG 源码、复制回调与普通代码块回退内容。
//
// Returns:
// 返回值：
// A loading frame, isolated SVG image, or the supplied source fallback.
// 加载框、隔离 SVG 图片或传入的源码回退内容。
export function SvgPreview(props: SvgPreviewProps) {
  // state tracks whether the direct SVG source is loading, ready, or rejected.
  // state 跟踪直接 SVG 源码处于加载、完成还是拒绝状态。
  const [state, setState] = React.useState<SvgPreviewState>({ status: "loading" });

  React.useEffect(() => {
    setState({ status: "loading" });
    if (!isRenderableSvgSource(props.source)) {
      setState({ status: "failed" });
      return undefined;
    }
    // svgBlob isolates untrusted SVG markup behind the browser image boundary.
    // svgBlob 将不可信 SVG 标记隔离在浏览器图片边界之后。
    const svgBlob = new Blob([props.source], { type: "image/svg+xml;charset=utf-8" });
    // imageUrl exposes the isolated SVG Blob to the shared diagram frame.
    // imageUrl 将隔离后的 SVG Blob 暴露给共享图表展示框。
    const imageUrl = URL.createObjectURL(svgBlob);
    setState({ status: "ready", imageUrl });
    return () => URL.revokeObjectURL(imageUrl);
  }, [props.source]);

  if (state.status === "failed") {
    return props.fallback;
  }
  return (
    <MarkdownDiagramFrame
      alt={props.source}
      copySource={props.copySource}
      iconPath="diagram.svg"
      imageUrl={state.status === "ready" ? state.imageUrl : null}
      loading={state.status === "loading"}
      onImageError={() => setState({ status: "failed" })}
      title="SVG"
    />
  );
}

// isRenderableSvgSource validates the fenced source without injecting it into the document DOM.
// isRenderableSvgSource 在不向文档 DOM 注入源码的前提下校验围栏内容。
//
// Parameters:
// 参数：
// - source: Complete fenced SVG source.
// - source：完整的围栏 SVG 源码。
//
// Returns:
// 返回值：
// True only for bounded XML documents whose root element is SVG and contain no parser errors.
// 仅当 XML 文档大小受限、根元素为 SVG 且不存在解析错误时返回 true。
function isRenderableSvgSource(source: string): boolean {
  if (!source.trim() || source.length > SVG_PREVIEW_SOURCE_CHARACTER_LIMIT) {
    return false;
  }
  // parsedDocument parses XML in an inert document used only for structural validation.
  // parsedDocument 在仅用于结构校验的惰性文档中解析 XML。
  const parsedDocument = new DOMParser().parseFromString(source, "image/svg+xml");
  // rootElement is the single XML document element that must be an SVG root.
  // rootElement 是必须为 SVG 根节点的唯一 XML 文档元素。
  const rootElement = parsedDocument.documentElement;
  return rootElement.localName.toLowerCase() === "svg"
    && parsedDocument.getElementsByTagName("parsererror").length === 0;
}
