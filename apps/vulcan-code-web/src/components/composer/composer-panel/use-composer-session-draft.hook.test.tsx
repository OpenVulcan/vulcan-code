// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { useComposerSessionDraft, type UseComposerSessionDraftState } from "./use-composer-session-draft";
import { useComposerSendController, type ComposerSendController } from "./use-composer-send-controller";
import type { ComposerSubmission } from "@/components/composer/composer-model";
import type { SessionDraftV1 } from "@/service/conversation-session-drafts-v1";
import type { ComposerAttachmentSummary } from "@/types";

// draftRpc is hoisted so the production process-wide coordinator receives one deterministic transport.
// draftRpc 被提升，使生产进程级协调器获得一个确定性的传输实现。
const draftRpc = vi.hoisted(() => ({ read: vi.fn(), write: vi.fn() }));

// The hook keeps its production coordinator while only the external durable transport is replaced.
// Hook 保留生产协调器，仅替换外部持久传输。
vi.mock("@/service/conversation-session-drafts-v1", () => ({ sessionDraftClientV1: draftRpc }));
// Tests do not load locale resources or emit user-visible toasts.
// 测试不加载语言资源，也不发出用户可见提示。
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
// Capture visible persistence diagnostics without rendering toast UI.
// 捕获可见持久化诊断，不渲染提示界面。
vi.mock("sonner", () => ({ toast: { error: vi.fn(), info: vi.fn() } }));
// No image preview is created because these transaction tests use no files.
// 事务测试不使用文件，因此不会创建图片预览。
vi.mock("@/service/conversation-actions-service", () => ({
  stageComposerAttachmentPreviewBytes: vi.fn(),
}));

/** Deferred exposes one externally controlled durable acknowledgement.
 * Deferred 暴露一个由外部控制的持久确认。
 */
interface Deferred<T> {
  /** Promise observed by the production writer.
   * 生产 writer 观察到的 Promise。
   */
  promise: Promise<T>;
  /** Resolves the durable acknowledgement.
   * 完成持久确认。
   */
  resolve: (value: T) => void;
}

/** Creates a deterministic promise without timers.
 * 创建一个不使用计时器的确定性 Promise。
 */
function deferred<T>(): Deferred<T> {
  let resolvePromise!: (value: T) => void;
  const promise = new Promise<T>((resolve) => { resolvePromise = resolve; });
  return { promise, resolve: resolvePromise };
}

// controller exposes the committed production hook state to each assertion.
// controller 向各项断言暴露已提交的生产 Hook 状态。
let controller: UseComposerSessionDraftState;
// sender exposes the real send transaction composed with the real draft hook.
// sender 暴露与真实草稿 Hook 组合的真实发送事务。
let sender: ComposerSendController;
// submitPrompt records external submission only; tests never call a model or write a user database.
// submitPrompt 仅记录外部提交；测试不调用模型，也不写入用户数据库。
const submitPrompt = vi.fn<(submission: ComposerSubmission) => Promise<boolean>>();
// projectAttachments models the owner-local preparation projection before clearing resets its state.
// projectAttachments 表示清空草稿重置准备状态前的所有者局部附件投影。
const projectAttachments = vi.fn<(attachments: ComposerAttachmentSummary[]) => ComposerAttachmentSummary[]>();
// captureSettings records the session-setting snapshot timing relative to draft persistence.
// captureSettings 记录会话设置相对于草稿持久化的快照时机。
const captureSettings = vi.fn<() => ComposerSubmission["sessionSettings"]>();
// clearSettings records successful consumption of the original draft settings.
// clearSettings 记录原草稿设置被成功消费。
const clearSettings = vi.fn();
// root and host own one isolated React tree per test.
// root 与 host 为每项测试持有一棵隔离 React 树。
let root: Root;
let host: HTMLDivElement;
// stage supplies the controlled canonical staging response without writing files.
// stage 提供受控的规范暂存响应，不写入文件。
const stage = vi.fn<() => Promise<ComposerAttachmentSummary | null>>();

/** Harness mounts the exact materialized-session draft hook used by ComposerPanel.
 * Harness 挂载 ComposerPanel 使用的精确已物化会话草稿 Hook。
 */
function Harness({ sessionId = "session-order", beforeSubmit }: { sessionId?: string; beforeSubmit?: () => Promise<boolean> }) {
  controller = useComposerSessionDraft({
    attachmentCapabilities: [{ kind: "file", label: "Files", enabled: true, native: false, bridged: true, accept: "*/*" }],
    projectId: "project-order",
    conversationId: sessionId,
    onStageAttachmentBytes: stage,
  });
  sender = useComposerSendController({
    draftSlotKey: controller.draftSlotKey,
    draftInput: controller.draftInput,
    attachments: controller.attachments,
    projectAttachmentsForSubmission: projectAttachments,
    executionMode: "agent",
    reasoningEffort: null,
    modelSelection: null,
    captureSessionSettings: captureSettings,
    clearSessionSettings: clearSettings,
    beforeSubmit,
    isConversationRunning: false,
    isDraftReplacementActive: () => false,
    clearForSubmission: controller.clearForSubmission,
    restoreFailedSubmission: controller.restoreFailedSubmission,
    restoreInputToSlot: controller.restoreInputToSlot,
    onSubmitPrompt: submitPrompt,
    onStopConversationRun: async () => true,
  });
  return null;
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  stage.mockResolvedValue(null);
  submitPrompt.mockResolvedValue(true);
  projectAttachments.mockImplementation((attachments) => attachments.map((attachment) => ({ ...attachment })));
  captureSettings.mockReturnValue(null);
  draftRpc.read.mockResolvedValue({
    sessionId: "session-order",
    text: "initial",
    attachments: [],
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

describe("composer send preflight ownership", () => {
  // Converted assets must persist through the real draft writer while unrelated text and attachments survive.
  // 转换资产必须通过真实草稿写入器持久化，同时保留无关文本和附件。
  it("persists an exact converted attachment replacement without overwriting newer draft content", async () => {
    // The service owns immutable source and derivative identities.
    // 服务持有不可变来源及派生产物身份。
    const source: ComposerAttachmentSummary = { assetId: "source-tiff", kind: "image", fileName: "source.tiff", localPath: "/fixture/source.tiff", mimeType: "image/tiff" };
    const png: ComposerAttachmentSummary = { assetId: "derived-png", kind: "image", fileName: "source.png", localPath: "/fixture/source.png", mimeType: "image/png" };
    draftRpc.read.mockResolvedValue({ sessionId: "session-order", text: "keep my prompt", attachments: [source] });
    draftRpc.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
    await act(async () => root.render(<Harness />));
    // Use the hydrated object identity exactly as the preparation hook does.
    // 与准备Hook相同，使用精确的加载后对象身份。
    const original = controller.attachments.find((attachment) => attachment.assetId === source.assetId)!;
    await act(async () => controller.installConvertedAttachments(new Map([[original, png]])));
    expect(controller.draftInput).toBe("keep my prompt");
    expect(controller.attachments).toEqual([png]);
    expect(draftRpc.write).toHaveBeenLastCalledWith("session-order", "keep my prompt", [png]);
    await act(async () => controller.installConvertedAttachments(new Map([[original, source]])));
    expect(controller.attachments).toEqual([png]);
  });
  // A configuration acknowledgement cannot turn an old send into a new-conversation draft mutation.
  // 配置确认不能把旧发送变成对新会话草稿的修改。
  it("keeps the new conversation untouched when an earlier preflight settles", async () => {
    // preflight controls the same configuration wait used by ComposerPanel.
    // preflight 控制与 ComposerPanel 相同的配置等待边界。
    const preflight = deferred<boolean>();
    draftRpc.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: `${sessionId} prompt`, attachments: [] }));
    draftRpc.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
    await act(async () => root.render(<Harness sessionId="preflight-a" beforeSubmit={() => preflight.promise} />));
    // pendingSend retains the original send callback across the owner transition.
    // pendingSend 在所有者切换期间保留原发送回调。
    let pendingSend!: Promise<void>;
    await act(async () => { pendingSend = sender.sendDraft(); });
    await act(async () => root.render(<Harness sessionId="preflight-b" />));
    draftRpc.write.mockClear();
    await act(async () => { preflight.resolve(true); await pendingSend; });
    expect(submitPrompt).not.toHaveBeenCalled();
    expect(draftRpc.write).not.toHaveBeenCalled();
    expect(controller.draftInput).toBe("preflight-b prompt");
    expect(sender.isSubmissionActive()).toBe(false);
  });

  // A same-owner edit invalidates only the pending send, leaving the complete new text available.
  // 同一所有者的编辑仅使待发送操作失效，完整新文本仍可使用。
  it("preserves newer text typed while configuration is pending", async () => {
    // preflight holds the configuration result without timers.
    // preflight 不使用计时器，保持配置结果未完成。
    const preflight = deferred<boolean>();
    draftRpc.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
    await act(async () => root.render(<Harness beforeSubmit={() => preflight.promise} />));
    // pendingSend retains the original text until preflight resolves.
    // pendingSend 在预检完成前保留原文本。
    let pendingSend!: Promise<void>;
    await act(async () => { pendingSend = sender.sendDraft(); });
    await act(async () => controller.setDraftInput("edited while waiting"));
    await act(async () => { preflight.resolve(true); await pendingSend; });
    expect(submitPrompt).not.toHaveBeenCalled();
    expect(controller.draftInput).toBe("edited while waiting");
    expect(sender.isSubmissionActive()).toBe(false);
  });

  // Failed configuration leaves the draft intact and releases the synchronous duplicate-send lock.
  // 配置失败保留完整草稿，并释放同步重复发送锁。
  it("does not consume the draft when configuration persistence fails", async () => {
    await act(async () => root.render(<Harness beforeSubmit={async () => false} />));
    await act(async () => sender.sendDraft());
    expect(controller.draftInput).toBe("initial");
    expect(submitPrompt).not.toHaveBeenCalled();
    expect(clearSettings).not.toHaveBeenCalled();
    expect(sender.isSubmissionActive()).toBe(false);
  });

  // Delivery choices must survive both a durable write wait and another conversation's preparation state.
  // 传输选择必须跨过持久写入等待，并与另一会话的附件准备状态隔离。
  it("freezes attachment delivery and settings before awaiting the original draft flush", async () => {
    // attachment is a synthetic canonical source without any filesystem or network access.
    // attachment 是不访问文件系统或网络的合成规范来源。
    const attachment: ComposerAttachmentSummary = { assetId: "send-freeze", kind: "file", fileName: "fixture.txt", localPath: "synthetic/fixture.txt", mimeType: "text/plain" };
    // persistedWrite delays the original owner's durable acknowledgement after UI clearing.
    // persistedWrite 在界面清空后延迟原所有者的持久确认。
    const persistedWrite = deferred<SessionDraftV1>();
    draftRpc.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: `${sessionId} prompt`, attachments: sessionId === "freeze-a" ? [attachment] : [] }));
    draftRpc.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => {
      if (sessionId === "freeze-a") await persistedWrite.promise;
      return { sessionId, text, attachments };
    });
    projectAttachments.mockImplementation((attachments) => attachments.map((item) => ({ ...item, deliveryPlan: "upload_reference" })));
    captureSettings.mockReturnValue({ outputStyleId: "captured-style" });
    await act(async () => root.render(<Harness sessionId="freeze-a" />));
    // pendingSend spans the exact clear-and-flush boundary.
    // pendingSend 跨越精确的清空及刷新边界。
    let pendingSend!: Promise<void>;
    await act(async () => { pendingSend = sender.sendDraft(); });
    expect(controller.draftInput).toBe("");
    expect(projectAttachments).toHaveBeenCalledTimes(1);
    projectAttachments.mockImplementation((attachments) => attachments.map((item) => ({ ...item, deliveryPlan: "metadata_only" })));
    captureSettings.mockReturnValue({ outputStyleId: "later-style" });
    await act(async () => root.render(<Harness sessionId="freeze-b" />));
    await act(async () => {
      persistedWrite.resolve({ sessionId: "freeze-a", text: "freeze-a prompt", attachments: [attachment] });
      await pendingSend;
    });
    expect(submitPrompt).toHaveBeenCalledExactlyOnceWith({
      draftSlotKey: "conversation:freeze-a",
      prompt: "freeze-a prompt",
      attachments: [{ ...attachment, deliveryPlan: "upload_reference" }],
      executionMode: "agent",
      reasoningEffort: null,
      modelSelection: null,
      sessionSettings: { outputStyleId: "captured-style" },
    });
    expect(controller.draftInput).toBe("freeze-b prompt");
    expect(clearSettings).toHaveBeenCalledTimes(1);
  });

  // A storage validation error must retain the unsent text and expose the actual failure.
  // 存储校验错误必须保留未发送文本并展示实际失败原因。
  it("preserves unsent text and reports the checkpoint error when draft persistence fails", async () => {
    // failure is a synthetic backend error; no real session data is changed.
    // failure 是合成后端错误；不修改真实会话数据。
    const failure = new Error("provider_source_fingerprint_mismatch: checkpoint seq 31");
    draftRpc.read.mockResolvedValue({ sessionId: "fingerprint-error", text: "unsent question", attachments: [] });
    draftRpc.write.mockRejectedValue(failure);
    await act(async () => root.render(<Harness sessionId="fingerprint-error" />));
    await act(async () => sender.sendDraft());
    expect(controller.draftInput).toBe("unsent question");
    expect(submitPrompt).not.toHaveBeenCalled();
    expect(sender.isSubmissionActive()).toBe(false);
    expect(toast.error).toHaveBeenCalledWith("composer.error.draftPersistFailed", {
      description: failure.message,
    });
    // Let owner cleanup persist successfully after the failure assertion.
    // 失败断言后允许所有者清理正常持久化。
    draftRpc.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
  });

  // Backend rejection restores the submitted snapshot while retaining text typed after consumption.
  // 后端拒绝时恢复提交快照，同时保留消费后新输入的文本。
  it("keeps later edits and rejects duplicate sends while the backend acknowledgement is pending", async () => {
    // acknowledgement holds the exact external submission response.
    // acknowledgement 保持精确外部提交响应未完成。
    const acknowledgement = deferred<boolean>();
    submitPrompt.mockReturnValue(acknowledgement.promise);
    draftRpc.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
    await act(async () => root.render(<Harness />));
    // pendingSend remains the unique transaction while a second click is attempted.
    // pendingSend 在尝试第二次点击期间保持唯一事务。
    let pendingSend!: Promise<void>;
    await act(async () => { pendingSend = sender.sendDraft(); await sender.sendDraft(); });
    await act(async () => controller.setDraftInput("next message"));
    await act(async () => { acknowledgement.resolve(false); await pendingSend; });
    expect(submitPrompt).toHaveBeenCalledTimes(1);
    expect(controller.draftInput).toBe("next message");
    expect(clearSettings).not.toHaveBeenCalled();
    expect(sender.isSubmissionActive()).toBe(false);
  });
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  host.remove();
  vi.unstubAllGlobals();
});

describe("queued prompt draft replacement ordering", () => {
  // Retained send callbacks must never consume a different conversation after route persistence waits.
  // 等待路由持久化期间保留的发送回调绝不能消费另一会话。
  it("rejects a retained clear callback after changing the draft owner without touching the new draft", async () => {
    draftRpc.read.mockImplementation(async (sessionId: string) => ({ sessionId, text: `${sessionId} text`, attachments: [] }));
    draftRpc.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
    await act(async () => root.render(<Harness sessionId="send-owner-a" />));
    // retainedClear represents the callback captured before awaiting model configuration.
    // retainedClear 表示等待模型配置前捕获的回调。
    const retainedClear = controller.clearForSubmission;
    await act(async () => root.render(<Harness sessionId="send-owner-b" />));
    draftRpc.write.mockClear();
    await act(async () => { await expect(retainedClear()).rejects.toThrow(); });
    expect(controller.draftInput).toBe("send-owner-b text");
    expect(draftRpc.write).not.toHaveBeenCalled();
  });

  // Editing during preflight retains the latest text instead of silently replacing it with an old snapshot.
  // 预检期间的编辑必须保留最新文本，不能被旧快照静默覆盖。
  it("rejects a retained clear callback when newer text has been entered in the same draft", async () => {
    draftRpc.write.mockImplementation(async (sessionId: string, text: string, attachments: ComposerAttachmentSummary[]) => ({ sessionId, text, attachments }));
    await act(async () => root.render(<Harness />));
    // retainedClear represents the original send intent before the user continues typing.
    // retainedClear 表示用户继续输入前的原发送意图。
    const retainedClear = controller.clearForSubmission;
    await act(async () => controller.setDraftInput("newer unsent text"));
    draftRpc.write.mockClear();
    await act(async () => { await expect(retainedClear()).rejects.toThrow(); });
    expect(controller.draftInput).toBe("newer unsent text");
    expect(draftRpc.write).not.toHaveBeenCalled();
  });

  it("blocks same-frame draft clearing until the exact accepted attachment has staged", async () => {
    const staging = deferred<ComposerAttachmentSummary | null>();
    stage.mockReturnValue(staging.promise);
    await act(async () => root.render(<Harness />));
    await act(async () => {
      controller.handleAttachmentInputChange({ currentTarget: { files: [new File(["fixture"], "file.txt")], value: "" } } as unknown as React.ChangeEvent<HTMLInputElement>);
      await expect(controller.clearForSubmission()).rejects.toThrow("composer.attachment.stagingPending");
    });
    expect(controller.pendingAttachmentCount).toBe(1);
    expect(controller.draftInput).toBe("initial");
    const attachment = { assetId: "fixture", kind: "file", fileName: "file.txt", localPath: "synthetic/file.txt", mimeType: "text/plain" };
    await act(async () => staging.resolve(attachment));
    expect(controller.pendingAttachmentCount).toBe(0);
    let captured!: Awaited<ReturnType<typeof controller.clearForSubmission>>;
    await act(async () => { captured = await controller.clearForSubmission(); });
    expect(captured.attachments).toEqual([attachment]);
  });

  it("releases failed staging without clearing the existing text", async () => {
    stage.mockRejectedValue(new Error("staging failed"));
    await act(async () => root.render(<Harness />));
    await act(async () => controller.handleAttachmentInputChange({ currentTarget: { files: [new File(["fixture"], "file.txt")], value: "" } } as unknown as React.ChangeEvent<HTMLInputElement>));
    expect(controller.hasPendingAttachmentStaging()).toBe(false);
    expect(controller.pendingAttachmentCount).toBe(0);
    expect(controller.draftInput).toBe("initial");
  });

  it("ignores late staging after changing the exact conversation owner", async () => {
    const staging = deferred<ComposerAttachmentSummary | null>();
    stage.mockReturnValue(staging.promise);
    await act(async () => root.render(<Harness />));
    await act(async () => controller.handleAttachmentInputChange({ currentTarget: { files: [new File(["fixture"], "file.txt")], value: "" } } as unknown as React.ChangeEvent<HTMLInputElement>));
    await act(async () => root.render(<Harness sessionId="other" />));
    expect(controller.pendingAttachmentCount).toBe(0);
    await act(async () => staging.resolve({ assetId: "late", kind: "file", fileName: "late.txt", localPath: "synthetic/late.txt", mimeType: "text/plain" }));
    expect(controller.attachments).toEqual([]);
    expect(controller.hasPendingAttachmentStaging()).toBe(false);
  });
  // The queue RPC boundary cannot be reached until every earlier complete draft write is acknowledged.
  // 全部较早完整草稿写入获得确认前，不能到达队列 RPC 边界。
  it("flushes earlier writes before admitting the replacement and writes the committed target afterwards", async () => {
    await act(async () => { root.render(<Harness />); });
    expect(controller.draftInput).toBe("initial");
    const firstWrite = deferred<SessionDraftV1>();
    const writes: string[] = [];
    draftRpc.write.mockImplementation(async (sessionId: string, text: string) => {
      writes.push(text);
      if (writes.length === 1) {
        await firstWrite.promise;
      }
      return { sessionId, text, attachments: [] };
    });

    await act(async () => { controller.setDraftInput("current"); });
    expect(writes).toEqual(["current"]);
    let tokenPromise!: ReturnType<UseComposerSessionDraftState["prepareQueuedDraftReplacement"]>;
    await act(async () => {
      tokenPromise = controller.prepareQueuedDraftReplacement();
      await Promise.resolve();
    });
    let boundaryReached = false;
    void tokenPromise.then(() => { boundaryReached = true; });
    await Promise.resolve();
    expect(boundaryReached).toBe(false);

    firstWrite.resolve({ sessionId: "session-order", text: "current", attachments: [] });
    let token!: Awaited<typeof tokenPromise>;
    await act(async () => { token = await tokenPromise; });
    expect(boundaryReached).toBe(true);
    expect(writes).toEqual(["current", "current"]);

    await act(async () => {
      expect(controller.completeQueuedDraftReplacement(token, "queued target", [])).toBe(true);
    });
    expect(controller.draftInput).toBe("queued target");
    expect(writes).toEqual(["current", "current", "queued target"]);
  });

  // An unknown RPC acknowledgement reloads the exact durable owner before local editing may resume.
  // 不明确的RPC确认会先重新加载精确持久owner，之后才可恢复本地编辑。
  it("recovers the authoritative draft after an ambiguous edit acknowledgement", async () => {
    draftRpc.read.mockResolvedValueOnce({
      sessionId: "session-recovery",
      text: "before edit",
      attachments: [],
    });
    draftRpc.write.mockImplementation(async (sessionId: string, text: string) => ({
      sessionId,
      text,
      attachments: [],
    }));
    await act(async () => { root.render(<Harness sessionId="session-recovery" />); });
    const token = await controller.prepareQueuedDraftReplacement();
    draftRpc.read.mockResolvedValueOnce({
      sessionId: "session-recovery",
      text: "committed while acknowledgement was lost",
      attachments: [],
    });
    await act(async () => { await controller.recoverQueuedDraftReplacement(token); });
    expect(controller.draftInput).toBe("committed while acknowledgement was lost");
    expect(draftRpc.write).toHaveBeenLastCalledWith(
      "session-recovery",
      "committed while acknowledgement was lost",
      [],
    );
  });
});
