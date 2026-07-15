import * as React from "react";
import type { Mermaid } from "mermaid";

import { MarkdownDiagramFrame } from "@/components/conversation/conversation-diagram-frame";
import { useDiffShikiTheme } from "@/diff/diff-highlight";
import {
  DIFF_SHIKI_DARK_THEME,
  type DiffShikiTheme,
} from "@/diff/diff-shiki-shared";

// MermaidDiagramProps describes one rendered Mermaid source block and its failure fallback.
// MermaidDiagramProps 描述单个 Mermaid 源码块及其失败回退内容。
interface MermaidDiagramProps {
  // Copies the complete Mermaid source through the shared Markdown clipboard path.
  // 通过共享 Markdown 剪贴板路径复制完整 Mermaid 源码。
  copySource: () => Promise<void>;
  // Markdown code-block fallback rendered when Mermaid rejects the source.
  // Mermaid 拒绝源码时渲染的 Markdown 代码块回退内容。
  fallback: React.ReactNode;
  // Complete Mermaid source extracted from the fenced Markdown block.
  // 从 Markdown 围栏代码块提取的完整 Mermaid 源码。
  source: string;
}

// MermaidDiagramState describes the asynchronous diagram rendering lifecycle.
// MermaidDiagramState 描述异步图表渲染生命周期。
type MermaidDiagramState =
  | { status: "loading" }
  | { status: "ready"; imageUrl: string }
  | { status: "failed" };

// mermaidRuntimePromise caches the lazily imported Mermaid runtime outside the initial WebUI bundle.
// mermaidRuntimePromise 缓存按需导入的 Mermaid 运行时，避免进入 WebUI 初始包。
let mermaidRuntimePromise: Promise<Mermaid> | null = null;

// mermaidRenderQueue serializes Mermaid's global configuration and render operations.
// mermaidRenderQueue 串行化 Mermaid 的全局配置与渲染操作。
let mermaidRenderQueue: Promise<void> = Promise.resolve();

// MermaidDiagram renders finalized Mermaid source as an isolated SVG image.
// MermaidDiagram 将最终确定的 Mermaid 源码渲染为隔离的 SVG 图片。
//
// Parameters:
// 参数：
// - props: Mermaid source and the ordinary code-block fallback.
// - props：Mermaid 源码与普通代码块回退内容。
//
// Returns:
// 返回值：
// A loading frame, rendered SVG image, or the supplied source fallback.
// 加载框、渲染后的 SVG 图片或传入的源码回退内容。
export function MermaidDiagram(props: MermaidDiagramProps) {
  // shikiTheme reuses the live WebUI theme subscription already shared by code renderers.
  // shikiTheme 复用代码渲染器共享的实时 WebUI 主题订阅。
  const shikiTheme = useDiffShikiTheme();
  // reactId provides a stable component-local identifier across re-renders.
  // reactId 为多次重新渲染提供稳定的组件本地标识。
  const reactId = React.useId();
  // renderId converts the React identifier into Mermaid-compatible SVG id text.
  // renderId 将 React 标识转换为 Mermaid 兼容的 SVG 标识文本。
  const renderId = React.useMemo(() => mermaidDiagramRenderId(reactId), [reactId]);
  // state tracks whether the asynchronous render is loading, ready, or rejected.
  // state 跟踪异步渲染处于加载、完成还是拒绝状态。
  const [state, setState] = React.useState<MermaidDiagramState>({ status: "loading" });

  React.useEffect(() => {
    // disposed prevents obsolete asynchronous work from committing after unmount or source changes.
    // disposed 防止过期异步任务在卸载或源码变化后提交结果。
    let disposed = false;
    // imageUrl retains the Blob URL owned by this effect for deterministic cleanup.
    // imageUrl 保存当前 effect 持有的 Blob URL，以便确定性清理。
    let imageUrl: string | null = null;
    setState({ status: "loading" });

    void renderMermaidSvg(renderId, props.source, shikiTheme)
      .then((svg) => {
        // svgBlob isolates Mermaid output behind the browser image boundary.
        // svgBlob 将 Mermaid 输出隔离在浏览器图片边界之后。
        const svgBlob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
        // nextImageUrl exposes the isolated SVG Blob to the image element.
        // nextImageUrl 将隔离后的 SVG Blob 暴露给图片元素。
        const nextImageUrl = URL.createObjectURL(svgBlob);
        if (disposed) {
          URL.revokeObjectURL(nextImageUrl);
          return;
        }
        imageUrl = nextImageUrl;
        setState({ status: "ready", imageUrl: nextImageUrl });
      })
      .catch(() => {
        if (!disposed) {
          setState({ status: "failed" });
        }
      });

    return () => {
      disposed = true;
      if (imageUrl) {
        URL.revokeObjectURL(imageUrl);
      }
    };
  }, [props.source, renderId, shikiTheme]);

  if (state.status === "failed") {
    return props.fallback;
  }
  return (
    <MarkdownDiagramFrame
      alt={props.source}
      copySource={props.copySource}
      iconPath="diagram.mermaid"
      imageUrl={state.status === "ready" ? state.imageUrl : null}
      loading={state.status === "loading"}
      onImageError={() => setState({ status: "failed" })}
      title="Mermaid"
    />
  );
}

// loadMermaidRuntime imports Mermaid once when the first finalized diagram becomes visible.
// loadMermaidRuntime 在首个最终图表可见时仅导入一次 Mermaid。
//
// Returns:
// 返回值：
// A shared promise resolving to the Mermaid browser runtime.
// 一个解析为 Mermaid 浏览器运行时的共享 Promise。
async function loadMermaidRuntime(): Promise<Mermaid> {
  if (!mermaidRuntimePromise) {
    mermaidRuntimePromise = import("mermaid").then((module) => module.default);
  }
  return mermaidRuntimePromise;
}

// renderMermaidSvg renders one source block with configuration matching the active WebUI theme.
// renderMermaidSvg 使用匹配当前 WebUI 主题的配置渲染单个源码块。
//
// Parameters:
// 参数：
// - renderId: Unique SVG identifier owned by the diagram component.
// - renderId：图表组件持有的唯一 SVG 标识。
// - source: Complete Mermaid definition.
// - source：完整 Mermaid 定义。
// - shikiTheme: Current WebUI light or dark theme identifier.
// - shikiTheme：当前 WebUI 亮色或暗色主题标识。
//
// Returns:
// 返回值：
// A promise resolving to Mermaid's generated SVG text.
// 一个解析为 Mermaid 生成 SVG 文本的 Promise。
function renderMermaidSvg(
  renderId: string,
  source: string,
  shikiTheme: DiffShikiTheme,
): Promise<string> {
  // renderTask waits for the previous global Mermaid configuration and render to finish.
  // renderTask 等待上一个 Mermaid 全局配置与渲染完成。
  const renderTask = mermaidRenderQueue.then(async () => {
    // mermaid is the cached browser runtime used for the current render.
    // mermaid 是当前渲染使用的缓存浏览器运行时。
    const mermaid = await loadMermaidRuntime();
    mermaid.initialize({
      securityLevel: "strict",
      startOnLoad: false,
      suppressErrorRendering: true,
      theme: shikiTheme === DIFF_SHIKI_DARK_THEME ? "dark" : "default",
    });
    // result contains the generated SVG and optional interaction bindings disabled by strict mode.
    // result 包含生成的 SVG，以及严格模式下禁用的可选交互绑定。
    const result = await mermaid.render(renderId, source);
    return result.svg;
  });
  mermaidRenderQueue = renderTask.then(
    () => undefined,
    () => undefined,
  );
  return renderTask;
}

// mermaidDiagramRenderId converts a React useId value into a valid Mermaid SVG identifier.
// mermaidDiagramRenderId 将 React useId 值转换为有效的 Mermaid SVG 标识。
//
// Parameters:
// 参数：
// - reactId: Stable identifier returned by React.
// - reactId：React 返回的稳定标识。
//
// Returns:
// 返回值：
// A component-unique identifier containing only Mermaid-safe characters.
// 一个仅包含 Mermaid 安全字符的组件唯一标识。
function mermaidDiagramRenderId(reactId: string): string {
  // normalizedId removes React's punctuation while retaining its uniqueness payload.
  // normalizedId 移除 React 标点，同时保留其唯一性载荷。
  const normalizedId = reactId.replace(/[^a-zA-Z0-9_-]/gu, "");
  return `vulcan-mermaid-${normalizedId}`;
}
