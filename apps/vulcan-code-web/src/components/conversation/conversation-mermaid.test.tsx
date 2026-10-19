// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { MermaidDiagram } from "./conversation-mermaid";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";

// renderSvg controls the upstream renderer while retaining the actual component and XML conversion.
// renderSvg 控制上游渲染器，同时保留真实组件与 XML 转换。
const { renderSvg } = vi.hoisted(() => ({ renderSvg: vi.fn() }));
vi.mock("mermaid", () => ({ default: { initialize: vi.fn(), render: renderSvg } }));
vi.mock("@/diff/diff-highlight", () => ({ useDiffShikiTheme: () => "github-light" }));

// host and root isolate each diagram lifecycle from the other tests.
// host 与 root 将每个图表生命周期与其他测试隔离。
let host: HTMLDivElement;
let root: Root;
// createUrl captures the exact Blob sent to the browser image decoder.
// createUrl 捕获传给浏览器图片解码器的精确 Blob。
const createUrl = vi.fn(() => "blob:mermaid-test");

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // TestURL preserves native URL parsing while controlling only Blob ownership.
  // TestURL 保留原生 URL 解析，仅控制 Blob 所有权。
  vi.stubGlobal("URL", class extends URL {
    static createObjectURL = createUrl;
    static revokeObjectURL = vi.fn();
  });
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(300);
  renderSvg.mockReset();
  createUrl.mockClear();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

// renderDiagram mounts a finalized diagram with a recognizable unchanged-source fallback.
// renderDiagram 挂载最终图表，并提供可识别的原始源码回退。
// Parameters: source is the literal diagram, and copySource handles its existing clipboard action.
// 参数：source 是字面图表源码，copySource 处理既有剪贴板操作。
// Returns after React settles the asynchronous render.
// React 完成异步渲染后返回。
async function renderDiagram(source = "flowchart TB\nA --> B", copySource = async () => {}): Promise<void> {
  await act(async () => root.render(
    <I18nProvider initialLocale="zh-CN">
      <TooltipProvider>
        <MermaidDiagram source={source} copySource={copySource} fallback={<pre>original source</pre>} />
      </TooltipProvider>
    </I18nProvider>,
  ));
}

it("uses the image frame after serialization instead of the source fallback", async () => {
  renderSvg.mockResolvedValue({ svg: '<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><div xmlns="http://www.w3.org/1999/xhtml">A<br>B</div></foreignObject></svg>' });
  await renderDiagram();
  expect(host.querySelector("img")?.getAttribute("src")).toBe("blob:mermaid-test");
  expect(createUrl.mock.calls).toHaveLength(1);
  expect(host.textContent).not.toContain("original source");
});

it("switches to literal source and back without rerendering, while retaining copy and expansion", async () => {
  renderSvg.mockResolvedValue({ svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100" />' });
  // source includes HTML-looking labels that must remain selectable text, never executable markup.
  // source 包含类似 HTML 的标签，必须保持可选择文本，绝不能作为可执行标记。
  const source = 'flowchart TB\n  A["<br/><script>text</script>"] --> B[中文]';
  // copySource verifies that switching modes retains the original clipboard callback.
  // copySource 验证模式切换保留原始剪贴板回调。
  const copySource = vi.fn(async () => {});
  await renderDiagram(source, copySource);
  // modes contains the two keyboard-focusable controls in their visible display order.
  // modes 按显示顺序保存两个可由键盘聚焦的控件。
  const modes = host.querySelectorAll<HTMLButtonElement>('.markdown-diagram-modes button');
  expect(modes[0].getAttribute("aria-pressed")).toBe("true");
  await act(async () => modes[1].click());
  expect(modes[1].getAttribute("aria-pressed")).toBe("true");
  expect(host.querySelector(".markdown-diagram-source code")!.textContent).toBe(source);
  expect(host.querySelector("script")).toBeNull();
  expect(host.querySelector("img")).toBeNull();
  expect(host.querySelector('[aria-label="展开图表"]')).toBeNull();
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="复制代码"]')!.click());
  expect(copySource).toHaveBeenCalledTimes(1);
  await act(async () => modes[0].click());
  expect(host.querySelector(".markdown-diagram-source")).toBeNull();
  expect(host.querySelector("img")!.getAttribute("src")).toBe("blob:mermaid-test");
  expect(host.querySelectorAll('[aria-label="展开图表"]')).toHaveLength(2);
  expect(renderSvg).toHaveBeenCalledTimes(1);
  expect(createUrl).toHaveBeenCalledTimes(1);
  expect(URL.revokeObjectURL).not.toHaveBeenCalled();
});

it("shows a parser error as plain text and preserves the source", async () => {
  renderSvg.mockRejectedValue(new Error("Parse error: <script>bad input</script>"));
  await renderDiagram();
  expect(host.querySelector("summary")?.textContent).toContain("Mermaid");
  expect(host.textContent).toContain("Parse error: <script>bad input</script>");
  expect(host.textContent).toContain("original source");
  expect(host.querySelector("script")).toBeNull();
  expect(createUrl).not.toHaveBeenCalled();
});

it("explains image decoding failure instead of silently falling back", async () => {
  renderSvg.mockResolvedValue({ svg: '<svg xmlns="http://www.w3.org/2000/svg" />' });
  await renderDiagram();
  // image is the actual shared frame element whose native load failure triggers the fallback.
  // image 是共享展示框中的真实元素，其原生加载失败会触发回退。
  const image = host.querySelector("img");
  expect(image).not.toBeNull();
  await act(async () => image!.dispatchEvent(new Event("error")));
  expect(host.querySelector("summary")).not.toBeNull();
  expect(host.querySelector("details pre")?.textContent).toBeTruthy();
  expect(host.textContent).toContain("original source");
});

it.each([".markdown-diagram-preview", ".markdown-code-action[aria-label='展开图表']"])("opens the shared zoomable viewer from %s and retains the Blob on close", async (selector) => {
  renderSvg.mockResolvedValue({ svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 800" />' });
  await renderDiagram();
  // trigger exercises both actual user entry points without changing internal component state.
  // trigger 覆盖两个真实用户入口，不修改组件内部状态。
  const trigger = host.querySelector<HTMLButtonElement>(selector)!;
  await act(async () => { trigger.focus(); trigger.click(); });
  // dialog verifies that expansion uses the original Blob rather than a service asset request.
  // dialog 验证展开复用原始 Blob，而不是发起服务资源请求。
  const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
  expect(dialog).not.toBeNull();
  const image = dialog.querySelector("img")!;
  expect(image.getAttribute("src")).toBe("blob:mermaid-test");
  Object.defineProperties(image, { naturalWidth: { value: 1000 }, naturalHeight: { value: 800 } });
  await act(async () => image.dispatchEvent(new Event("load")));
  // original restores drawing units before testing keyboard pan and fit recovery.
  // original 恢复绘图单位，再验证键盘平移与适配恢复。
  const original = dialog.querySelector<HTMLButtonElement>('[aria-label="原始尺寸"]')!;
  await act(async () => original.click());
  expect(dialog.querySelector<HTMLImageElement>("img")!.style.transform).toBe("scale(1)");
  const viewer = dialog.querySelector<HTMLElement>(".generated-media-image-viewer")!;
  await act(async () => viewer.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
  expect(dialog.querySelector<HTMLElement>(".generated-media-image-canvas")!.style.transform).toContain("-40px");
  await act(async () => viewer.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true })));
  expect(dialog.querySelector<HTMLImageElement>("img")!.style.transform).toBe("scale(0.375)");
  await act(async () => dialog.querySelector<HTMLButtonElement>('[aria-label="关闭播放器"]')!.click());
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(host.querySelector("img")!.getAttribute("src")).toBe("blob:mermaid-test");
  expect(URL.revokeObjectURL).not.toHaveBeenCalled();
  expect(renderSvg).toHaveBeenCalledTimes(1);
});
