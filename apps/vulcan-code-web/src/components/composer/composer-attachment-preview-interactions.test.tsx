// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ComposerPanelAttachmentPreviewStrip } from "./composer-panel/attachment-preview-strip";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ComposerAttachmentSummary } from "@/types";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

// host and root isolate each real attachment strip without a service or model.
// host 与 root 隔离各个真实附件条，不使用服务或模型。
let host: HTMLDivElement;
let root: Root;
// open, remove and retry record exact attachment targets instead of replacing rendered controls.
// open、remove 与 retry 记录精确附件目标，不替换渲染控件。
const open = vi.fn();
const remove = vi.fn();
const retry = vi.fn();
// emptyFocus records the explicit handoff to the owner after its last attachment disappears.
// emptyFocus 记录最后一个附件消失后显式交回所属输入区的焦点请求。
const emptyFocus = vi.fn();
// resolve records the precise local path requested for an image thumbnail.
// resolve 记录图片缩略图请求的精确本地路径。
const resolve = vi.fn((path: string) => `https://synthetic.invalid/${path}`);
// image retains both canonical and explicitly generated preview paths, as returned by staging.
// image 保留暂存返回的规范路径及明确生成的预览路径。
const image: ComposerAttachmentSummary = { assetId: "preview-image", kind: "image", fileName: "original.png", mimeType: "image/png", localPath: "canonical/original.png", previewLocalPath: "previews/original.jpg" };

/** Renders the exact wrapper and its production controls with caller-specified preparation states.
 * 按调用方指定的准备状态渲染精确包装器及生产控件。
 * Parameters: attachments are staged files and preparations are keyed by their stable IDs; returns after committing effects.
 * 参数：attachments 是暂存文件，preparations 按稳定标识索引；提交副作用后返回。
 */
async function render(attachments: ComposerAttachmentSummary[], preparations: React.ComponentProps<typeof ComposerPanelAttachmentPreviewStrip>["preparations"] = {}, owner = "owner-a") {
  await act(async () => root.render(<TooltipProvider delayDuration={0}>
    <ComposerPanelAttachmentPreviewStrip key={owner} attachments={attachments} preparations={preparations} progresses={{}}
      resolveMediaSourceUrl={resolve} onOpenMedia={open} onRemove={remove} onRetry={retry} onEmptyAfterRemoval={emptyFocus} />
  </TooltipProvider>));
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // JSDOM has no layout observer; the browser fixture validates real popover dimensions separately.
  // JSDOM 无布局观察器；浏览器夹具另行验证真实浮层尺寸。
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("loads the declared lightweight preview while opening the unchanged canonical media descriptor", async () => {
  await render([image]);
  expect(resolve).toHaveBeenCalledWith("previews/original.jpg");
  expect(resolve).not.toHaveBeenCalledWith("canonical/original.png");
  expect(host.querySelector("img")?.getAttribute("src")).toBe("https://synthetic.invalid/previews/original.jpg");
  await act(async () => host.querySelector<HTMLButtonElement>(".composer-attachment-open")!.click());
  expect(open).toHaveBeenCalledWith({ kind: "image", fileName: "original.png", localPath: "canonical/original.png", previewLocalPath: "previews/original.jpg", mimeType: "image/png" });
});

it("uses the canonical image only when staging did not produce a preview path", async () => {
  await render([{ ...image, previewLocalPath: undefined }]);
  expect(resolve).toHaveBeenCalledWith("canonical/original.png");
});

it("names removal and retry actions by their exact file and does not open unsupported files", async () => {
  // archive is a declared unsupported preview type with a retryable upload failure.
  // archive 是明确不支持预览、且上传失败可重试的文件类型。
  const archive: ComposerAttachmentSummary = { assetId: "preview-archive", kind: "file", fileName: "archive.zip", mimeType: "application/zip", localPath: "canonical/archive.zip" };
  await render([image, archive], { "preview-archive": {
    attachmentId: "preview-archive", blocksSubmission: true, deliveryPlan: "upload_reference", error: "Synthetic upload refusal",
    generation: 1, phase: "failed", reason: "upload", transferId: "synthetic-transfer",
  } });
  expect(Array.from(host.querySelectorAll(".composer-attachment-remove"), (button) => button.getAttribute("aria-label"))).toEqual([
    "composer.attachment.remove · original.png", "composer.attachment.remove · archive.zip",
  ]);
  expect(host.querySelector(".composer-attachment-retry")?.getAttribute("aria-label")).toBe("composer.attachment.retry · archive.zip");
  expect(host.querySelectorAll<HTMLButtonElement>(".composer-attachment-open")[1].disabled).toBe(true);
  await act(async () => host.querySelectorAll<HTMLButtonElement>(".composer-attachment-open")[1].click());
  expect(open).not.toHaveBeenCalled();
  await act(async () => host.querySelector<HTMLButtonElement>(".composer-attachment-retry")!.click());
  expect(retry).toHaveBeenCalledWith("preview-archive");
  await act(async () => host.querySelectorAll<HTMLButtonElement>(".composer-attachment-remove")[1].click());
  expect(remove).toHaveBeenCalledWith(1);
  expect(open).not.toHaveBeenCalled();
});

it("keeps raw failures out of the closed strip and restores focus after inspecting details", async () => {
  // failureText represents the verbatim service failure, including unbroken identifiers and line breaks.
  // failureText 表示原样服务错误，包含连续标识符与换行。
  const failureText = `SyntheticFailure${"LongIdentifier".repeat(24)}\nRetry remains a separate action`;
  await render([image], { "preview-image": {
    attachmentId: "preview-image", blocksSubmission: true, deliveryPlan: "upload_reference", error: failureText,
    generation: 1, phase: "failed", reason: "upload", transferId: "synthetic-transfer",
  } });
  expect(document.body.textContent).not.toContain(failureText);
  // details is the real Radix trigger; opening it must not activate media preview or retry.
  // details 是真实 Radix 触发器；打开时不得触发媒体预览或重试。
  const details = host.querySelector<HTMLButtonElement>("button.composer-attachment-warning")!;
  await act(async () => { details.focus(); details.click(); });
  // dialog exposes a filename-specific accessible name and keyboard-scrollable original diagnostics.
  // dialog 提供带文件名的无障碍名称，以及可用键盘滚动的原始诊断。
  const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
  expect(dialog.getAttribute("aria-label")).toBe("composer.attachment.details · original.png");
  expect(dialog.querySelector("pre")?.textContent).toBe(failureText);
  expect(dialog.querySelector("pre")?.tabIndex).toBe(0);
  expect(open).not.toHaveBeenCalled();
  expect(retry).not.toHaveBeenCalled();
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  // Radix restores trigger focus after the dismissed focus scope has unmounted.
  // Radix 在已关闭的焦点范围卸载后恢复触发器焦点。
  await vi.waitFor(() => expect(document.activeElement).toBe(details));
  expect(document.body.textContent).not.toContain(failureText);
});

it("keeps focus during cancellation, moves to the next remaining file, and hands off after the final removal", async () => {
  // second is a distinct staged object whose index changes after removing the first file.
  // second 是独立暂存对象，首个文件移除后其索引会变化。
  const second = { ...image, assetId: "second-image", fileName: "second.png", localPath: "canonical/second.png" };
  await render([image, second]);
  // firstTrigger remains mounted until the confirmed removal reaches the canonical attachment list.
  // firstTrigger 在确认移除到达规范附件列表前持续挂载。
  const firstTrigger = host.querySelector<HTMLButtonElement>(".composer-attachment-remove")!;
  await act(async () => { firstTrigger.focus(); firstTrigger.click(); });
  await render([image, second]);
  expect(document.activeElement).toBe(firstTrigger);
  expect(emptyFocus).not.toHaveBeenCalled();
  await render([second]);
  expect(document.activeElement).toBe(host.querySelector(".composer-attachment-remove"));
  expect(document.activeElement?.getAttribute("aria-label")).toBe("composer.attachment.remove · second.png");
  await act(async () => (document.activeElement as HTMLButtonElement).click());
  await render([]);
  expect(emptyFocus).toHaveBeenCalledTimes(1);
  expect(host.querySelector(".composer-attachment-strip")).toBeNull();
});

it("does not take focus back from another control during delayed removal", async () => {
  await render([image]);
  // outside models a user deliberately navigating elsewhere while cancellation is pending.
  // outside 模拟取消等待期间用户主动移向其他位置。
  const outside = document.createElement("button"); document.body.append(outside);
  try {
    // trigger is the exact removal origin captured before the owner confirms cancellation.
    // trigger 是所属组件确认取消前捕获的精确移除来源。
    const trigger = host.querySelector<HTMLButtonElement>(".composer-attachment-remove")!;
    await act(async () => { trigger.focus(); trigger.click(); outside.focus(); });
    await render([]);
    expect(document.activeElement).toBe(outside);
    expect(emptyFocus).not.toHaveBeenCalled();
  } finally { outside.remove(); }
});

it("does not restore focus into a different draft owner", async () => {
  await render([image]);
  // trigger belongs to owner-a and must not move focus after owner-b replaces the entire strip.
  // trigger 属于 owner-a，owner-b 替换整个附件条后不得再移动焦点。
  const trigger = host.querySelector<HTMLButtonElement>(".composer-attachment-remove")!;
  await act(async () => { trigger.focus(); trigger.click(); });
  await render([], {}, "owner-b");
  expect(emptyFocus).not.toHaveBeenCalled();
});
