// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { MathFormula } from "./answer-components/answer-math";
import { MermaidDiagram } from "./conversation-mermaid";

// Use the real renderers, with only the browser services jsdom cannot supply.
// 使用真实渲染器，仅替代 jsdom 无法提供的浏览器服务。
it("renders a Mermaid flowchart as an isolated image and a KaTeX formula as MathML", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Own one object URL per rendered image without emulating SVG serialization.
  // 每张图仅持有一个对象 URL，实际执行 SVG 序列化。
  const createObjectURL = vi.fn(() => "blob:rendered-diagram");
  vi.stubGlobal("URL", class extends URL {
    static createObjectURL = createObjectURL;
    static revokeObjectURL = vi.fn();
  });
  vi.stubGlobal("ResizeObserver", class {
    // jsdom has no layout engine, so there is no resize lifecycle to deliver.
    // jsdom 没有布局引擎，因此无需传递尺寸变化。
    observe() {}
    disconnect() {}
  });
  // Supply text geometry for Mermaid's SVG layout without replacing any renderer code.
  // 为 Mermaid 的 SVG 布局提供文本尺寸，保留真实渲染代码。
  const previousBox = Object.getOwnPropertyDescriptor(SVGElement.prototype, "getBBox");
  Object.defineProperty(SVGElement.prototype, "getBBox", {
    configurable: true,
    value: () => ({ x: 0, y: 0, width: 80, height: 24 }),
  });
  // Mount both production renderers into the actual document used by Mermaid and DOMPurify.
  // 将两个生产渲染器挂载到 Mermaid 和 DOMPurify 实际使用的文档。
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  try {
    await act(async () => {
      root.render(
        <I18nProvider initialLocale="en-US">
          <TooltipProvider>
            <MermaidDiagram source={"flowchart LR\nA[Start] --> B[Finish]"} copySource={async () => {}} fallback={<code>source</code>} />
            <MathFormula tex={String.raw`\frac{1}{2}`} />
          </TooltipProvider>
        </I18nProvider>,
      );
    });
    await act(async () => {
      await vi.waitFor(() => expect(createObjectURL).toHaveBeenCalledTimes(1), { timeout: 10_000 });
    });
    expect(host.querySelector('img[src="blob:rendered-diagram"]')).not.toBeNull();
    expect(host.querySelector("svg .flowchart-link")).toBeNull();
    expect(host.querySelector("math mfrac")?.textContent).toBe("12");
  } finally {
    await act(async () => root.unmount());
    host.remove();
    if (previousBox) {
      Object.defineProperty(SVGElement.prototype, "getBBox", previousBox);
    } else {
      Reflect.deleteProperty(SVGElement.prototype, "getBBox");
    }
    vi.unstubAllGlobals();
  }
}, 15_000);
