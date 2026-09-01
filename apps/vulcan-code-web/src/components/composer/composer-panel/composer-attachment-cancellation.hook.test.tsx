// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useComposerAttachments } from "../composer-attachments";
import { useComposerAttachmentPreparation, type ComposerAttachmentPreparationController } from "./use-composer-attachment-preparation";
import { useComposerAttachmentRemoval } from "./use-composer-attachment-removal";
import {
  cancelConversationSubmission,
  discardComposerAttachmentPlans,
  planComposerAttachments,
  prepareComposerAttachment,
  reconcileComposerAttachmentDraftLease,
} from "@/service/conversation-actions-service";
import type { ComposerAttachmentPreparationSummary, ComposerAttachmentSummary } from "@/types";

// All service boundaries are local test doubles; React state, effects and production hooks remain real.
// 所有服务边界均为本地测试替身；React 状态、effect 与生产 Hook 保持真实。
vi.mock("@/service/conversation-actions-service", () => ({
  cancelConversationSubmission: vi.fn(),
  discardComposerAttachmentPlans: vi.fn(),
  planComposerAttachments: vi.fn(),
  prepareComposerAttachment: vi.fn(),
  reconcileComposerAttachmentDraftLease: vi.fn(),
  stageComposerAttachmentPreviewBytes: vi.fn(),
}));
// Attachment removal does not exercise translation loading or connect to a workspace.
// 附件移除不执行翻译加载，也不连接工作区。
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

// deferred exposes deterministic completion controls without timers or provider requests.
// deferred 暴露确定性完成控制，不使用计时器或供应商请求。
// Returns a typed promise and its success/failure callbacks.
// 返回带类型 Promise 及其成功/失败回调。
function deferred<T>() {
  // resolve and reject are assigned synchronously by the Promise constructor.
  // resolve 与 reject 由 Promise 构造函数同步赋值。
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

// asset builds distinct canonical objects; paths are inert fixture strings, never opened.
// asset 构造独立规范对象；路径只是惰性夹具字符串，绝不打开。
// Parameter: id is the service-style identity; returns a staged image summary.
// 参数：id 是服务式身份；返回已暂存图片摘要。
function asset(id: string): ComposerAttachmentSummary {
  return { assetId: id, kind: "image", fileName: `${id}.png`, localPath: `/fixture/${id}.png`, mimeType: "image/png", byteLen: 9 };
}

// model is an exact, durable route selection shared by the isolated hook tests.
// model 是隔离 Hook 测试共用的精确持久路线选择。
const model = { providerId: "provider-a", modelId: "model-a", connectionId: "connection-a" };
// controller and attachments expose committed production hook state to assertions.
// controller 与 attachments 向断言暴露已提交的生产 Hook 状态。
let controller: ComposerAttachmentPreparationController;
let attachments: ComposerAttachmentSummary[];
// replace and remove drive actual React state and the production asynchronous removal hook.
// replace 与 remove 驱动真实 React 状态及生产异步移除 Hook。
let replace: React.Dispatch<React.SetStateAction<ComposerAttachmentSummary[]>>;
let remove: (index: number) => Promise<void>;
// invalidateStaging and selectFiles exercise the real async file-admission generation gate.
// invalidateStaging 与 selectFiles 执行真实异步文件准入代际门控。
let invalidateStaging: () => void;
let selectFiles: (files: File[]) => void;
// stageAttachmentBytes is the browser-to-service staging boundary used by generation tests.
// stageAttachmentBytes 是代际测试使用的浏览器到服务暂存边界。
const stageAttachmentBytes = vi.fn();
// root and host isolate each render tree and are always disposed by afterEach.
// root 与 host 隔离每棵渲染树，并始终由 afterEach 释放。
let root: Root;
let host: HTMLDivElement;
// initial supplies one stable staged object; upload stays pending until the test explicitly completes it.
// initial 提供一个稳定暂存对象；upload 保持在途直到测试显式完成。
let initial: ComposerAttachmentSummary[];
let upload: ReturnType<typeof deferred<ComposerAttachmentPreparationSummary>>;

// Harness mounts the same attachment state, preparation and removal hooks used by ComposerPanel.
// Harness 挂载与 ComposerPanel 相同的附件状态、准备和移除 Hook。
// Parameter: slot selects the draft owner; returns no visual UI because assertions inspect committed hook outputs.
// 参数：slot 选择草稿所有者；无需视觉界面，断言读取已提交 Hook 输出。
function Harness({
  slot = "draft-a",
  modelId = "model-a",
  enableStaging = false,
}: {
  slot?: string;
  modelId?: string;
  enableStaging?: boolean;
}) {
  // state uses the real functional removal mutation rather than emulating React updates.
  // state 使用真实函数式移除方法，而非模拟 React 更新。
  const state = useComposerAttachments({
    initialAttachments: initial,
    attachmentCapabilities: enableStaging
      ? [{
        kind: "image",
        label: "Image",
        enabled: true,
        native: true,
        bridged: false,
        accept: "image/*",
      }]
      : [],
    projectId: "project-a",
    conversationId: null,
    onStageAttachmentBytes: stageAttachmentBytes,
  });
  controller = useComposerAttachmentPreparation({
    draftSlotKey: slot,
    projectId: "project-a",
    conversationId: null,
    modelSelection: { ...model, modelId },
    routePlanningRevision: `sha256:route-${modelId}`,
    routeSelectionPending: false,
    attachments: state.attachments,
  });
  remove = useComposerAttachmentRemoval({
    draftSlotKey: slot,
    attachments: state.attachments,
    preparation: controller,
    remove: state.removeComposerAttachment,
  });
  attachments = state.attachments;
  replace = state.setAttachments;
  invalidateStaging = state.invalidatePendingAttachmentStaging;
  selectFiles = (files) => state.handleAttachmentInputChange({
    currentTarget: { files, value: "selected" },
  } as unknown as React.ChangeEvent<HTMLInputElement>);
  return null;
}

// mount flushes real effects and the deterministic mocked RPC microtasks.
// mount 刷新真实 effect 及确定性的模拟 RPC 微任务。
// Returns after the initial upload has actually entered the production preparation hook.
// 返回时初次上传已经进入生产准备 Hook。
async function mount() {
  await act(async () => { root.render(<Harness />); });
  expect(prepareComposerAttachment).toHaveBeenCalledTimes(1);
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  stageAttachmentBytes.mockResolvedValue(null);
  initial = [asset("asset-a")];
  upload = deferred<ComposerAttachmentPreparationSummary>();
  vi.mocked(cancelConversationSubmission).mockResolvedValue(true);
  vi.mocked(discardComposerAttachmentPlans).mockResolvedValue(0);
  vi.mocked(reconcileComposerAttachmentDraftLease).mockResolvedValue(true);
  vi.mocked(prepareComposerAttachment).mockImplementation(() => upload.promise);
  vi.mocked(planComposerAttachments).mockImplementation(async (_project, _conversation, _slot, generation, _model, files) => (
    files.map((file) => ({
      attachmentId: file.assetId!, generation, transferId: `draft-media:${generation}:${file.assetId}`,
      deliveryPlan: "upload_reference", phase: "upload_queued", blocksSubmission: true,
      routeRevision: `sha256:route-${_model!.modelId}`, reason: "provider_upload_required", error: null,
    }))
  ));
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  vi.mocked(cancelConversationSubmission).mockResolvedValue(true);
  await act(async () => { root.unmount(); });
  host.remove();
  vi.unstubAllGlobals();
});

describe("composer cancellation with real React effects", () => {
  // Completing an older file selection cannot append it after an explicit full-draft replacement begins.
  // 较早文件选择完成后，不能在显式完整草稿替换开始后追加该文件。
  it("discards staged bytes completed after the draft generation is invalidated", async () => {
    const staged = deferred<ComposerAttachmentSummary | null>();
    stageAttachmentBytes.mockReturnValue(staged.promise);
    await act(async () => { root.render(<Harness enableStaging />); });
    await act(async () => { selectFiles([new File(["image"], "late.png", { type: "image/png" })]); });
    expect(stageAttachmentBytes).toHaveBeenCalledTimes(1);
    await act(async () => {
      invalidateStaging();
      staged.resolve(asset("asset-late"));
      await staged.promise;
    });
    expect(attachments).toEqual(initial);
  });
  // Settling another attachment cannot erase the owner's earlier failed cancellation barrier.
  // 另一个附件结算不能清除同一所有者此前失败的取消屏障。
  it("does not let a successful second cancellation mask the first failure", async () => {
    // Both assets belong to one owner but have independent physical transfer identities.
    // 两个资产属于同一所有者，但拥有独立实体传输身份。
    initial = [asset("asset-a"), asset("asset-b")];
    await act(async () => { root.render(<Harness />); });
    expect(prepareComposerAttachment).toHaveBeenCalledTimes(2);
    vi.mocked(cancelConversationSubmission).mockRejectedValueOnce(new Error("first cancellation lost"));
    await act(async () => { await expect(controller.cancel("asset-a")).rejects.toThrow("first cancellation lost"); });
    await act(async () => { await controller.cancel("asset-b"); });
    await act(async () => { await expect(controller.reconcileDraft([])).rejects.toThrow("first cancellation lost"); });
    expect(controller.preparations["asset-a"].blocksSubmission).toBe(true);
    expect(controller.preparations["asset-b"].phase).toBe("cancelled");
    expect(vi.mocked(reconcileComposerAttachmentDraftLease).mock.calls.every((call) => call[3].length > 0)).toBe(true);
  });
  // The backend returns false for an already-protected exact set, not for a missing acknowledgement.
  // 后端对已经受保护的精确集合返回 false，而不是用它表示缺少确认。
  it("accepts an unchanged but successfully reconciled draft lease", async () => {
    vi.mocked(reconcileComposerAttachmentDraftLease).mockResolvedValue(false);
    await mount();
    expect(controller.blocksSubmission).toBe(true);
    expect(controller.preparations["asset-a"].phase).toBe("uploading");
    expect(controller.preparations["asset-a"].error).toBeNull();
  });

  // A failed release acknowledgement keeps the clicked file available for another explicit removal.
  // 释放确认失败会保留点击文件，允许再次明确移除。
  it("keeps the local attachment when its removal lease is not confirmed", async () => {
    await mount();
    vi.mocked(reconcileComposerAttachmentDraftLease).mockRejectedValueOnce(new Error("lease acknowledgement unavailable"));
    await act(async () => { await expect(remove(0)).rejects.toThrow("acknowledgement unavailable"); });
    expect(attachments).toEqual(initial);
    await act(async () => { await remove(0); });
    expect(attachments).toEqual([]);
  });

  // Switching the rendered model cannot discard a failed cancellation or start competing work.
  // 切换渲染模型不能丢弃取消失败，也不能开始竞争工作。
  it("retains the cancellation failure across route projection changes", async () => {
    await mount();
    const transferId = controller.preparations["asset-a"].transferId;
    vi.mocked(cancelConversationSubmission).mockRejectedValue(new Error("cancel unavailable"));
    await act(async () => { await expect(controller.cancel("asset-a")).rejects.toThrow("cancel unavailable"); });
    await act(async () => { root.render(<Harness modelId="model-b" />); });
    expect(controller.preparations["asset-a"]).toMatchObject({
      transferId, reason: "draft_upload_cancellation_failed", blocksSubmission: true,
    });
    expect(controller.blocksSubmission).toBe(true);
    expect(planComposerAttachments).toHaveBeenCalledTimes(1);
    expect(prepareComposerAttachment).toHaveBeenCalledTimes(1);
    await act(async () => { await expect(controller.reconcileDraft([])).rejects.toThrow("cancel unavailable"); });
    expect(vi.mocked(reconcileComposerAttachmentDraftLease).mock.calls.every((call) => call[3].length > 0)).toBe(true);
  });

  // Route views reuse the original pending operation; B never starts upload and A's observer keeps its own generation.
  // 路线视图复用原在途操作；B绝不启动上传，A观察者保留自己的代际。
  it("keeps A uploading across B and restores A through an observing view", async () => {
    await mount();
    // original is the actual first prepare RPC, still unresolved while B is rendered.
    // original 是真实首次准备RPC，渲染B时仍未完成。
    const original = controller.preparations["asset-a"];
    vi.mocked(planComposerAttachments).mockImplementation(async (_project, _conversation, _slot, generation, selected, files) => files.map((file) => ({
      attachmentId: file.assetId!, generation,
      transferId: selected.modelId === "model-b" ? original.transferId : `observer:${generation}`,
      physicalTransferId: original.transferId,
      deliveryPlan: selected.modelId === "model-b" ? "metadata_only" : "upload_reference",
      phase: selected.modelId === "model-b" ? "metadata_only" : "upload_queued",
      blocksSubmission: selected.modelId !== "model-b",
      routeRevision: `sha256:route-${selected.modelId}`, reason: "original_upload", error: null,
    })));
    vi.mocked(prepareComposerAttachment).mockImplementation(async (_project, _conversation, _model, _attachment, view) => {
      await upload.promise;
      return { ...view, phase: "direct_ready", blocksSubmission: false };
    });
    await act(async () => { root.render(<Harness modelId="model-b" />); });
    expect(controller.preparations["asset-a"].deliveryPlan).toBe("metadata_only");
    expect(controller.blocksSubmission).toBe(false);
    expect(prepareComposerAttachment).toHaveBeenCalledTimes(1);
    expect(cancelConversationSubmission).not.toHaveBeenCalled();
    await act(async () => { root.render(<Harness />); });
    expect(prepareComposerAttachment).toHaveBeenCalledTimes(2);
    expect(controller.preparations["asset-a"].physicalTransferId).toBe(original.transferId);
    expect(controller.blocksSubmission).toBe(true);
    // The observer's generation, not the original RPC generation, owns the returning A view.
    // 返回A视图由观察者代际而非原RPC代际持有。
    const observer = controller.preparations["asset-a"];
    await act(async () => { upload.resolve({ ...original, phase: "direct_ready", blocksSubmission: false }); });
    expect(controller.preparations["asset-a"]).toMatchObject({ generation: observer.generation, transferId: observer.transferId, phase: "direct_ready" });
    expect(controller.blocksSubmission).toBe(false);
    expect(cancelConversationSubmission).not.toHaveBeenCalled();
    expect(discardComposerAttachmentPlans).not.toHaveBeenCalled();
  });
  // A rejected cancellation must not be interpreted as a settled provider or an unleased draft.
  // 被拒绝的取消不能被解释为供应商已结算或草稿可以解除租约。
  it("retains the source and exact transfer id after cancellation fails", async () => {
    await mount();
    const transferId = controller.preparations["asset-a"].transferId;
    const leaseCalls = vi.mocked(reconcileComposerAttachmentDraftLease).mock.calls.length;
    vi.mocked(cancelConversationSubmission).mockRejectedValue(new Error("cancel acknowledgement lost"));
    await act(async () => { await expect(remove(0)).rejects.toThrow("acknowledgement lost"); });
    expect(attachments).toEqual(initial);
    expect(controller.preparations["asset-a"]).toMatchObject({ transferId, phase: "failed", blocksSubmission: true });
    expect(controller.blocksSubmission).toBe(true);
    await act(async () => {
      await expect(controller.reconcileDraft([])).rejects.toThrow("acknowledgement lost");
      await expect(controller.reconcileDraft([])).rejects.toThrow("acknowledgement lost");
    });
    expect(reconcileComposerAttachmentDraftLease).toHaveBeenCalledTimes(leaseCalls);
    expect(prepareComposerAttachment).toHaveBeenCalledTimes(1);
  });

  // Retrying cancellation targets the original operation, never a synthetic id or a second upload.
  // 重试取消针对原操作，绝不针对合成标识或第二次上传。
  it("retries the same cancellation and then permits removal", async () => {
    await mount();
    const transferId = controller.preparations["asset-a"].transferId;
    vi.mocked(cancelConversationSubmission).mockRejectedValueOnce(new Error("offline"));
    await act(async () => { await expect(remove(0)).rejects.toThrow("offline"); });
    await act(async () => { await controller.retry("asset-a"); });
    expect(vi.mocked(cancelConversationSubmission).mock.calls.map(([id]) => id)).toEqual([transferId, transferId]);
    expect(controller.preparations["asset-a"].phase).toBe("cancelled");
    expect(controller.blocksSubmission).toBe(false);
    expect(prepareComposerAttachment).toHaveBeenCalledTimes(1);
    await act(async () => { await remove(0); });
    expect(attachments).toEqual([]);
    expect(reconcileComposerAttachmentDraftLease).toHaveBeenLastCalledWith("project-a", null, "draft-a", []);
  });

  // A physical upload response cannot override an unresolved cancellation or its send barrier.
  // 实体上传响应不能覆盖未确认取消及其发送屏障。
  it("ignores late upload success after cancellation failure", async () => {
    await mount();
    const active = controller.preparations["asset-a"];
    vi.mocked(cancelConversationSubmission).mockRejectedValueOnce(new Error("lost"));
    await act(async () => { await expect(controller.cancel("asset-a")).rejects.toThrow("lost"); });
    await act(async () => { upload.resolve({ ...active, phase: "direct_ready", blocksSubmission: false }); });
    expect(controller.preparations["asset-a"].reason).toBe("draft_upload_cancellation_failed");
    expect(controller.blocksSubmission).toBe(true);
  });

  // Multiple removal clicks observe one pending cancellation and cannot prematurely release its source.
  // 多次移除点击观察同一个待确认取消，不能提前释放其来源。
  it("joins duplicate cancellation requests until the acknowledgement arrives", async () => {
    await mount();
    const cancellation = deferred<boolean>();
    vi.mocked(cancelConversationSubmission).mockReturnValue(cancellation.promise);
    let first!: Promise<void>;
    let second!: Promise<void>;
    await act(async () => { first = controller.cancel("asset-a"); second = controller.cancel("asset-a"); });
    expect(cancelConversationSubmission).toHaveBeenCalledTimes(1);
    expect(controller.blocksSubmission).toBe(true);
    await act(async () => { cancellation.resolve(true); await Promise.all([first, second]); });
    expect(controller.preparations["asset-a"].phase).toBe("cancelled");
  });

  // Additions during cancellation are preserved both in the replacement lease and local state.
  // 取消期间新增项会同时保留在替换租约与本地状态中。
  it("keeps an attachment added while removal waits for cancellation", async () => {
    await mount();
    const cancellation = deferred<boolean>();
    const added = asset("asset-b");
    vi.mocked(cancelConversationSubmission).mockReturnValue(cancellation.promise);
    let removing!: Promise<void>;
    await act(async () => { removing = remove(0); });
    await act(async () => { replace((current) => [...current, added]); });
    expect(attachments).toEqual([...initial, added]);
    await act(async () => { cancellation.resolve(true); await removing; });
    expect(attachments).toEqual([added]);
    expect(reconcileComposerAttachmentDraftLease).toHaveBeenLastCalledWith("project-a", null, "draft-a", [added]);
  });

  // A later object at the same index must not be removed by an older click's acknowledgement.
  // 较早点击的确认不能移除后来位于同一索引的对象。
  it("does not remove a replacement occupying the old index", async () => {
    await mount();
    const cancellation = deferred<boolean>();
    const replacement = asset("asset-b");
    vi.mocked(cancelConversationSubmission).mockReturnValue(cancellation.promise);
    let removing!: Promise<void>;
    await act(async () => { removing = remove(0); replace([replacement]); });
    await act(async () => { cancellation.resolve(true); await removing; });
    expect(attachments).toEqual([replacement]);
    expect(vi.mocked(reconcileComposerAttachmentDraftLease).mock.calls.every((call) => call[3].length > 0)).toBe(true);
  });

  // A set replacement during the lease RPC restores the new exact set and discards the stale click.
  // 租约 RPC 期间替换集合会恢复新的精确集合并丢弃陈旧点击。
  it("restores the current lease after a concurrent replacement", async () => {
    await mount();
    const lease = deferred<boolean>();
    const added = asset("asset-b");
    vi.mocked(reconcileComposerAttachmentDraftLease).mockReturnValueOnce(lease.promise);
    let removing!: Promise<void>;
    await act(async () => { removing = remove(0); });
    await act(async () => { replace((current) => [...current, added]); });
    await act(async () => { lease.resolve(true); await removing; });
    expect(attachments).toEqual([...initial, added]);
    expect(reconcileComposerAttachmentDraftLease).toHaveBeenLastCalledWith("project-a", null, "draft-a", attachments);
  });

  // Owner changes do not authorize the old cancellation to mutate the newly rendered draft.
  // 所有者变化不允许旧取消修改新渲染草稿。
  it("does not release or remove attachments from another draft slot", async () => {
    await mount();
    const cancellation = deferred<boolean>();
    vi.mocked(cancelConversationSubmission).mockReturnValue(cancellation.promise);
    let removing!: Promise<void>;
    await act(async () => { removing = remove(0); root.render(<Harness slot="draft-b" />); });
    await act(async () => { cancellation.resolve(true); await removing; });
    expect(attachments).toEqual(initial);
    expect(vi.mocked(reconcileComposerAttachmentDraftLease).mock.calls.every((call) => call[3].length > 0)).toBe(true);
  });
});
