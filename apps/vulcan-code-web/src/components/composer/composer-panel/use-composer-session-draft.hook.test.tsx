// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useComposerSessionDraft, type UseComposerSessionDraftState } from "./use-composer-session-draft";
import type { SessionDraftV1 } from "@/service/conversation-session-drafts-v1";

// draftRpc is hoisted so the production process-wide coordinator receives one deterministic transport.
// draftRpc 被提升，使生产进程级协调器获得一个确定性的传输实现。
const draftRpc = vi.hoisted(() => ({ read: vi.fn(), write: vi.fn() }));

// The hook keeps its production coordinator while only the external durable transport is replaced.
// Hook 保留生产协调器，仅替换外部持久传输。
vi.mock("@/service/conversation-session-drafts-v1", () => ({ sessionDraftClientV1: draftRpc }));
// Tests do not load locale resources or emit user-visible toasts.
// 测试不加载语言资源，也不发出用户可见提示。
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
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
// root and host own one isolated React tree per test.
// root 与 host 为每项测试持有一棵隔离 React 树。
let root: Root;
let host: HTMLDivElement;

/** Harness mounts the exact materialized-session draft hook used by ComposerPanel.
 * Harness 挂载 ComposerPanel 使用的精确已物化会话草稿 Hook。
 */
function Harness({ sessionId = "session-order" }: { sessionId?: string }) {
  controller = useComposerSessionDraft({
    attachmentCapabilities: [],
    projectId: "project-order",
    conversationId: sessionId,
    onStageAttachmentBytes: async () => null,
  });
  return null;
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  draftRpc.read.mockResolvedValue({
    sessionId: "session-order",
    text: "initial",
    attachments: [],
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  host.remove();
  vi.unstubAllGlobals();
});

describe("queued prompt draft replacement ordering", () => {
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
