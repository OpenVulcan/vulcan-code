import { describe, expect, it } from "vitest";

import {
  composerAttachmentCancelledState,
  composerAttachmentPlansMatchDraft,
  composerAttachmentRouteReadyForPlanning,
  composerAttachmentUploadTransferIds,
  queueComposerDraftLeaseAfterCancellation,
} from "@/components/composer/composer-panel/use-composer-attachment-preparation";
import type { ComposerAttachmentPreparationSummary } from "@/types";

// MODEL_SELECTION is one exact provider, model, and credential tuple used by planning tests.
// MODEL_SELECTION 是规划测试使用的一组精确供应商、模型与凭据元组。
const MODEL_SELECTION = {
  providerId: "provider-a",
  modelId: "model-a",
  connectionId: "connection-a",
};

// Verifies planning starts only after persistence and an authoritative route revision both exist.
// 验证仅在持久化完成且权威路线修订存在后才开始规划。
describe("composerAttachmentRouteReadyForPlanning", () => {
  // Rejects optimistic, unresolved, and missing-selection windows without fallback planning.
  // 在不降级规划的前提下拒绝乐观、未解析与缺失选择窗口。
  it("rejects every route window that is not durable", () => {
    expect(composerAttachmentRouteReadyForPlanning(MODEL_SELECTION, "sha256:route", true)).toBe(false);
    expect(composerAttachmentRouteReadyForPlanning(MODEL_SELECTION, "", false)).toBe(false);
    expect(composerAttachmentRouteReadyForPlanning(null, "sha256:route", false)).toBe(false);
  });

  // Accepts the exact durable route tuple projected by the backend.
  // 接受由后端投影的精确持久路线元组。
  it("accepts an authoritative durable route", () => {
    expect(composerAttachmentRouteReadyForPlanning(MODEL_SELECTION, "sha256:route", false)).toBe(true);
  });
});

// PLANNED_ATTACHMENT is one exact current-generation service plan row.
// PLANNED_ATTACHMENT 是一条精确的当前代际服务规划记录。
const PLANNED_ATTACHMENT: ComposerAttachmentPreparationSummary = {
  attachmentId: "asset-a",
  generation: 7,
  transferId: "draft-media:transfer-a",
  deliveryPlan: "upload_reference",
  phase: "upload_queued",
  blocksSubmission: true,
  routeRevision: "sha256:route",
  reason: "provider_upload_required",
  error: null,
};

// Verifies planning responses cannot release the barrier unless every identity remains exact.
// 验证规划响应仅在每项身份均保持精确时才能解除屏障。
describe("composerAttachmentPlansMatchDraft", () => {
  // Accepts one exact one-to-one plan.
  // 接受一份精确的一一对应规划。
  it("accepts the exact draft generation and route", () => {
    expect(composerAttachmentPlansMatchDraft(
      [PLANNED_ATTACHMENT],
      ["asset-a"],
      7,
      "sha256:route",
    )).toBe(true);
  });

  // Rejects duplicate, missing, stale-generation, and stale-route responses.
  // 拒绝重复、缺失、陈旧代际与陈旧路线响应。
  it("rejects every incomplete or stale planning response", () => {
    expect(composerAttachmentPlansMatchDraft([], ["asset-a"], 7, "sha256:route")).toBe(false);
    expect(composerAttachmentPlansMatchDraft(
      [PLANNED_ATTACHMENT, PLANNED_ATTACHMENT],
      ["asset-a", "asset-b"],
      7,
      "sha256:route",
    )).toBe(false);
    expect(composerAttachmentPlansMatchDraft(
      [{ ...PLANNED_ATTACHMENT, generation: 6 }],
      ["asset-a"],
      7,
      "sha256:route",
    )).toBe(false);
    expect(composerAttachmentPlansMatchDraft(
      [{ ...PLANNED_ATTACHMENT, routeRevision: "sha256:stale" }],
      ["asset-a"],
      7,
      "sha256:route",
    )).toBe(false);
  });
});

// Verifies stale planning cleanup targets only unique service-owned upload plans.
// 验证陈旧规划清理仅针对唯一的服务端上传规划。
describe("composerAttachmentUploadTransferIds", () => {
  // Filters local plans and deduplicates repeated upload identities before cancellation.
  // 在取消前过滤本地规划并去重重复上传身份。
  it("returns unique upload plan identities only", () => {
    expect(composerAttachmentUploadTransferIds([
      PLANNED_ATTACHMENT,
      PLANNED_ATTACHMENT,
      {
        ...PLANNED_ATTACHMENT,
        attachmentId: "asset-b",
        transferId: "draft-media:local-b",
        deliveryPlan: "inline_original",
        phase: "local_ready",
        blocksSubmission: false,
      },
    ])).toEqual(["draft-media:transfer-a"]);
  });
});

// Verifies cancellation revokes the former transfer identity before asynchronous provider completion.
// 验证取消在供应商异步完成前撤销原传输身份。
describe("composerAttachmentCancelledState", () => {
  // Replaces the transfer identity and closes the send barrier while retaining metadata-only delivery.
  // 替换传输身份并关闭发送屏障，同时保留仅元数据传递。
  it("revokes late completion from the cancelled transfer", () => {
    const cancelled = composerAttachmentCancelledState(
      PLANNED_ATTACHMENT,
      "draft-media-cancelled:terminal-a",
    );

    expect(cancelled.transferId).toBe("draft-media-cancelled:terminal-a");
    expect(cancelled.transferId).not.toBe(PLANNED_ATTACHMENT.transferId);
    expect(cancelled.phase).toBe("cancelled");
    expect(cancelled.blocksSubmission).toBe(false);
    expect(cancelled.reason).toBe("draft_upload_cancelled_metadata_only");
  });
});

// Verifies draft ownership release cannot overtake cancellation of the active provider transfer.
// 验证草稿所有权释放不能超越活跃供应商传输的取消。
describe("queueComposerDraftLeaseAfterCancellation", () => {
  // Keeps the queue pending until both the prior lease mutation and cancellation have settled.
  // 在旧租约变更与取消都结束前保持队列未完成。
  it("orders resource release after cancellation", async () => {
    let releaseCurrent!: () => void;
    let releaseCancellation!: () => void;
    const current = new Promise<void>((resolve) => { releaseCurrent = resolve; });
    const cancellation = new Promise<void>((resolve) => { releaseCancellation = resolve; });
    const tail = queueComposerDraftLeaseAfterCancellation(current, cancellation);
    let settled = false;
    void tail.then(() => { settled = true; });

    releaseCancellation();
    await Promise.resolve();
    expect(settled).toBe(false);

    releaseCurrent();
    await tail;
    expect(settled).toBe(true);
  });
});
