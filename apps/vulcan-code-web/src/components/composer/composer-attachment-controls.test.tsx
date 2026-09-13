import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import type {
  ComposerAttachmentPreparationSummary,
  ComposerAttachmentSummary,
} from "@/types";
import { ComposerAttachmentStrip } from "./composer-attachment-controls";

describe("ComposerAttachmentStrip", () => {
  it.each(["uploading", "awaiting_provider_confirmation"] as const)("does not invent a percentage for %s without byte totals", (phase) => {
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><TooltipProvider>
      <ComposerAttachmentStrip attachments={[{ assetId: "file", kind: "file", fileName: "archive.zip", localPath: "synthetic/archive.zip", mimeType: "application/zip" }]}
        preparations={{ file: { attachmentId: "file", blocksSubmission: true, deliveryPlan: "upload_reference", generation: 1, phase, reason: "upload", transferId: "transfer" } }}
        resolvePreview={() => null} onOpenMedia={() => undefined} />
    </TooltipProvider></I18nProvider>);
    expect(markup).not.toContain("0%"); expect(markup).not.toContain("99%");
    expect(markup).toContain("motion-safe:animate-spin");
    expect(markup).toContain("Preview is unavailable for this file type");
    expect(markup).toMatch(/composer-attachment-open[^>]*disabled/);
  });
  it("keeps metadata-only detail behind an accessible tooltip trigger", () => {
    // attachment is one exact staged video shown in the composer strip.
    // attachment 是输入区附件条中展示的一条精确已暂存视频。
    const attachment: ComposerAttachmentSummary = {
      assetId: "video-asset",
      byteLen: 700_000,
      fileName: "sample.mp4",
      kind: "video",
      localPath: "D:/project/.vulcan/video.mp4",
      mimeType: "video/mp4",
      resourcePath: ".vulcan/video.mp4",
    };
    // preparation freezes the metadata-only route and its user-facing explanation.
    // preparation 冻结仅元数据路线及其面向用户的说明。
    const preparation: ComposerAttachmentPreparationSummary = {
      attachmentId: "video-asset",
      blocksSubmission: false,
      deliveryPlan: "metadata_only",
      error: null,
      generation: 1,
      phase: "metadata_only",
      reason: "provider_limit",
      transferId: "transfer-video",
    };
    // markup captures the closed tooltip trigger without opening a persistent explanation row.
    // markup 捕获关闭状态的提示触发器，且不会打开常驻说明行。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider delayDuration={0}>
          <ComposerAttachmentStrip
            attachments={[attachment]}
            preparations={{ "video-asset": preparation }}
            resolvePreview={() => ({ kind: "video" })}
            onOpenMedia={() => undefined}
          />
        </TooltipProvider>
      </I18nProvider>,
    );

    expect(markup).toContain("composer-attachment-warning");
    expect(markup).toContain('tabindex="0"');
    expect(markup).toContain("此附件不会直接交给模型理解");
    expect(markup).not.toContain('title="此附件不会直接交给模型理解');
    expect(markup).not.toContain("1 个附件将仅作为附件清单告知模型");
  });
});
