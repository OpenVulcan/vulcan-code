// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { GeneratedMediaPlayerWindow } from "./generated-media-player";
import type { GeneratedMediaAttachment } from "./generated-media-types";

vi.mock("@/resources/localMedia", async (importOriginal) => ({
  ...await importOriginal<typeof import("@/resources/localMedia")>(),
  localMediaSourceUrl: (path: string) => `/synthetic/${path}`,
}));

// root and host mount real portal controls without loading any local user media.
// root 与 host 挂载真实门户控件，而不加载任何用户本地媒体。
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 400, height: 300, x: 0, y: 0, top: 0, left: 0, right: 400, bottom: 300, toJSON() {} });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(300);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

// mount renders a typed synthetic attachment and returns after browser controls commit.
// mount 渲染有类型的合成附件，并在浏览器控件提交后返回。
async function mount(kind: GeneratedMediaAttachment["kind"], localPath = "sample", close = vi.fn()) {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><GeneratedMediaPlayerWindow attachment={{ kind, localPath, fileName: "合成媒体" }} onClose={close} /></TooltipProvider></I18nProvider>));
}

// action resolves a real toolbar action by its localized accessible label.
// action 按本地化无障碍标签定位真实工具栏操作。
function action(label: string) {
  return document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
}

it.each(["image", "audio", "video"] as const)("shows a recoverable %s load failure and isolates replacement media", async (kind) => {
  await mount(kind);
  const element = document.querySelector(kind === "image" ? "img" : kind)!;
  await act(async () => element.dispatchEvent(new Event("error")));
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("无法加载此媒体文件");
  if (kind === "image") {
    // label enumerates unavailable image actions while failure keeps the zoom readout unknown.
    // label 枚举不可用图片操作；失败时缩放读数保持未知。
    for (const label of ["自适应窗口", "原始尺寸", "缩小图片", "放大图片"]) expect(action(label).disabled).toBe(true);
    expect(document.querySelector(".generated-media-image-zoom-control strong")?.textContent).toBe("-");
  }
  const retry = [...document.querySelectorAll("button")].find((button) => button.textContent === "重试")!;
  await act(async () => retry.click());
  const retried = document.querySelector(kind === "image" ? "img" : kind)!;
  expect(retried).not.toBe(element);
  expect(retried.getAttribute("src")).toBe("/synthetic/sample");
  await mount(kind, "second");
  await act(async () => retried.dispatchEvent(new Event("error")));
  expect(document.querySelector('[role="alert"]')).toBeNull();
  expect(document.querySelector(kind === "image" ? "img" : kind)?.getAttribute("src")).toBe("/synthetic/second");
});

it("pans a zoomed image with the keyboard and restores the fitted viewport", async () => {
  await mount("image");
  const image = document.querySelector("img")!;
  Object.defineProperties(image, { naturalWidth: { value: 1000 }, naturalHeight: { value: 800 } });
  await act(async () => image.dispatchEvent(new Event("load")));
  expect(action("原始尺寸").disabled).toBe(false);
  await act(async () => action("原始尺寸").click());
  expect(document.querySelector('.generated-media-image-zoom-control strong')?.textContent).toBe("100%");
  const viewer = document.querySelector<HTMLElement>('.generated-media-image-viewer')!;
  expect(viewer.tabIndex).toBe(0);
  await act(async () => viewer.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true })));
  expect(document.querySelector<HTMLElement>('.generated-media-image-canvas')?.style.transform).toContain("-40px");
  await act(async () => viewer.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true, cancelable: true })));
  expect(document.querySelector<HTMLElement>('.generated-media-image-canvas')?.style.transform).toContain("0px");
  expect(action("自适应窗口").disabled).toBe(true);
});

// Very large images must fit completely, and zoom-out must never enlarge an already fitted image.
// 超大图片必须完整适应窗口，缩小操作绝不能反而放大已经适配的图片。
it("fits a large image below ten percent and preserves the zoom direction", async () => {
  // Dialog entrance transforms change visual bounds without changing the actual layout viewport.
  // 弹窗入场变换会改变视觉边界，但不会改变实际布局视口。
  vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockReturnValue({ width: 380, height: 285, x: 0, y: 0, top: 0, left: 0, right: 380, bottom: 285, toJSON() {} });
  await mount("image", "large-image");
  // loadingImage supplies natural dimensions while the production viewer retains its measured 400x300 stage.
  // loadingImage 提供原始尺寸，生产查看器仍使用测得的 400x300 舞台。
  const loadingImage = document.querySelector("img")!;
  Object.defineProperties(loadingImage, { naturalWidth: { value: 10000 }, naturalHeight: { value: 8000 } });
  await act(async () => loadingImage.dispatchEvent(new Event("load")));
  // scale reads the rendered transformation, so this exercises the real toolbar and wheel handlers.
  // scale 读取实际渲染的变换，以覆盖真实工具栏与滚轮处理链。
  const scale = () => Number(document.querySelector<HTMLImageElement>("img")!.style.transform.match(/^scale\((.+)\)$/)![1]);
  expect(scale()).toBeCloseTo(0.0375, 8);
  expect(action("缩小图片").disabled).toBe(true);
  // viewer receives a real DOM wheel event; shrinking at the fitted lower limit must stay there.
  // viewer 接收真实 DOM 滚轮事件；在适配下限继续缩小时必须保持原比例。
  const viewer = document.querySelector<HTMLElement>(".generated-media-image-viewer")!;
  await act(async () => viewer.dispatchEvent(new WheelEvent("wheel", { deltaY: 100, bubbles: true, cancelable: true })));
  expect(scale()).toBeCloseTo(0.0375);
  await act(async () => action("放大图片").click());
  expect(scale()).toBeCloseTo(0.1375);
  expect(action("缩小图片").disabled).toBe(false);
  await act(async () => action("缩小图片").click());
  expect(scale()).toBeCloseTo(0.0375);
  await act(async () => action("原始尺寸").click());
  expect(scale()).toBe(1);
  await act(async () => action("自适应窗口").click());
  expect(scale()).toBeCloseTo(0.0375);
  // A larger viewport must not turn the next shrinking wheel gesture into enlargement.
  // 视口扩大后，下一次缩小滚轮操作不能反而放大图片。
  vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockReturnValue({ width: 800, height: 600, x: 0, y: 0, top: 0, left: 0, right: 800, bottom: 600, toJSON() {} });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(600);
  await act(async () => viewer.dispatchEvent(new WheelEvent("wheel", { deltaY: 100, bubbles: true, cancelable: true })));
  expect(scale()).toBeCloseTo(0.0375);
  await act(async () => action("自适应窗口").click());
  expect(scale()).toBeCloseTo(0.075);
});

// Verify the production upper bound and the recovery path without manipulating component state.
// 不直接操作组件状态，验证生产缩放上限及恢复路径。
it("caps wheel enlargement, disables zoom-in at the bound, and restores original size", async () => {
  await mount("image");
  // image supplies a small natural size so its original size is the initial fit.
  // image 提供小图原始尺寸，使初始适配即为原始大小。
  const image = document.querySelector("img")!;
  Object.defineProperties(image, { naturalWidth: { value: 200 }, naturalHeight: { value: 100 } });
  await act(async () => image.dispatchEvent(new Event("load")));
  // viewer receives consecutive wheel events through the actual DOM listener, reaching the declared limit.
  // viewer 通过真实 DOM 监听接收连续滚轮事件，达到已声明上限。
  const viewer = document.querySelector<HTMLElement>(".generated-media-image-viewer")!;
  // gesture counts enough enlargement inputs to cross and then remain at the upper bound.
  // gesture 计数足够的放大输入，以达到上限并验证继续输入仍保持上限。
  for (let gesture = 0; gesture < 30; gesture += 1) {
    await act(async () => viewer.dispatchEvent(new WheelEvent("wheel", { deltaY: -100, bubbles: true, cancelable: true })));
  }
  expect(document.querySelector<HTMLImageElement>("img")!.style.transform).toBe("scale(10)");
  expect(action("放大图片").disabled).toBe(true);
  await act(async () => action("原始尺寸").click());
  expect(document.querySelector<HTMLImageElement>("img")!.style.transform).toBe("scale(1)");
  expect(action("原始尺寸").disabled).toBe(true);
  expect(action("放大图片").disabled).toBe(false);
});

// Document sources use the exact resolved URL; an absent source offers no misleading link or retry.
// 文档使用准确解析的网址；缺少来源时不提供误导的链接或重试。
it("renders a document link with an isolated browsing context and reports an absent source", async () => {
  await mount("document", "document-source");
  // link is the only document action and retains the browser's native keyboard semantics.
  // link 是唯一文档操作，并保留浏览器原生键盘语义。
  const link = document.querySelector<HTMLAnchorElement>(".generated-media-document-link")!;
  expect(link.getAttribute("href")).toBe("/synthetic/document-source");
  expect(link.target).toBe("_blank");
  expect(link.rel).toBe("noreferrer");
  await mount("document", "");
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("无法加载此媒体文件");
  expect(document.querySelector(".generated-media-document-link")).toBeNull();
  expect([...document.querySelectorAll("button")].some((button) => button.textContent === "重试")).toBe(false);
});
