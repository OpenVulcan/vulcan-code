import { describe, expect, it, vi } from "vitest";
import { SessionDraftWriteCoordinatorV1 } from "@/service/conversation-session-draft-write-coordinator-v1";
import type { SessionDraftRpcV1 } from "@/service/conversation-session-drafts-v1";
import type { ComposerAttachmentSummary } from "@/types";

/** One externally controlled promise used to prove serialized write ordering.
 * 一份由测试外部控制的 Promise，用于证明串行写入顺序。
 */
interface Deferred<T> {
  /** Promise observed by the production coordinator.
   * 生产协调器观察到的 Promise。
   */
  promise: Promise<T>;
  /** Resolves the controlled promise.
   * 完成受控 Promise。
   */
  resolve: (value: T) => void;
  /** Rejects the controlled promise.
   * 拒绝受控 Promise。
   */
  reject: (reason: unknown) => void;
}

/** Creates one deferred promise without hidden timers.
 * 创建一份不包含隐藏计时器的 deferred Promise。
 */
function deferred<T>(): Deferred<T> {
  let resolvePromise!: (value: T) => void;
  let rejectPromise!: (reason: unknown) => void;
  const promise = new Promise<T>((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });
  return { promise, resolve: resolvePromise, reject: rejectPromise };
}

/** Creates one detached attachment fixture for payload ownership assertions.
 * 创建一个脱离引用的附件夹具，用于载荷所有权断言。
 */
function attachment(name: string): ComposerAttachmentSummary {
  return {
    kind: "file",
    fileName: name,
    localPath: `staged://${name}`,
    mimeType: "text/plain",
  };
}

describe("SessionDraftWriteCoordinatorV1", () => {
  it("serializes one session and coalesces a typing burst to the newest complete draft", async () => {
    const first = deferred<void>();
    const calls: Array<{ sessionId: string; text: string; names: string[] }> = [];
    const client: SessionDraftRpcV1 = {
      read: async () => ({ sessionId: "unused", text: "", attachments: [] }),
      write: async (sessionId, text, attachments) => {
        calls.push({
          sessionId,
          text,
          names: attachments.map((value) => value.fileName),
        });
        if (calls.length === 1) {
          await first.promise;
        }
        return { sessionId, text, attachments: [...attachments] };
      },
    };
    const coordinator = new SessionDraftWriteCoordinatorV1(client);

    coordinator.enqueue("session-a", "a", [attachment("a.txt")]);
    coordinator.enqueue("session-a", "ab", [attachment("ab.txt")]);
    coordinator.enqueue("session-a", "abc", [attachment("abc.txt")]);
    expect(calls).toEqual([{
      sessionId: "session-a",
      text: "a",
      names: ["a.txt"],
    }]);

    first.resolve();
    await coordinator.flush("session-a");

    expect(calls).toEqual([
      { sessionId: "session-a", text: "a", names: ["a.txt"] },
      { sessionId: "session-a", text: "abc", names: ["abc.txt"] },
    ]);
  });

  it("keeps different session writers independent", async () => {
    const blocked = deferred<void>();
    const calls: string[] = [];
    const client: SessionDraftRpcV1 = {
      read: async () => ({ sessionId: "unused", text: "", attachments: [] }),
      write: async (sessionId, text, attachments) => {
        calls.push(`${sessionId}:${text}`);
        if (sessionId === "session-a") {
          await blocked.promise;
        }
        return { sessionId, text, attachments: [...attachments] };
      },
    };
    const coordinator = new SessionDraftWriteCoordinatorV1(client);

    coordinator.enqueue("session-a", "blocked", []);
    coordinator.enqueue("session-b", "ready", []);
    await coordinator.flush("session-b");

    expect(calls).toEqual(["session-a:blocked", "session-b:ready"]);
    blocked.resolve();
    await coordinator.flush("session-a");
  });

  it("surfaces a failed durable write and permits a later exact retry", async () => {
    let shouldFail = true;
    const client: SessionDraftRpcV1 = {
      read: async () => ({ sessionId: "unused", text: "", attachments: [] }),
      write: async (sessionId, text, attachments) => {
        if (shouldFail) {
          throw new Error("disk unavailable");
        }
        return { sessionId, text, attachments: [...attachments] };
      },
    };
    const coordinator = new SessionDraftWriteCoordinatorV1(client);

    coordinator.enqueue("session-a", "draft", []);
    await expect(coordinator.flush("session-a")).rejects.toThrow(/disk unavailable/);

    shouldFail = false;
    coordinator.enqueue("session-a", "draft", []);
    await expect(coordinator.flush("session-a")).resolves.toBeUndefined();
  });

  it("writes the first keystroke at once and the rest of a typing burst as one later write", async () => {
    vi.useFakeTimers();
    try {
      const calls: string[] = [];
      const client: SessionDraftRpcV1 = {
        read: async () => ({ sessionId: "unused", text: "", attachments: [] }),
        write: async (sessionId, text, attachments) => {
          calls.push(text);
          return { sessionId, text, attachments: [...attachments] };
        },
      };
      const coordinator = new SessionDraftWriteCoordinatorV1(client, 500);
      coordinator.enqueue("session-a", "h", [], true);
      for (const text of ["he", "hel", "hell", "hello"]) {
        await vi.advanceTimersByTimeAsync(50);
        coordinator.enqueue("session-a", text, [], true);
      }
      expect(calls).toEqual(["h"]);
      await vi.advanceTimersByTimeAsync(500);
      expect(calls).toEqual(["h", "hello"]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("never holds back a change that is not typing, nor a flush", async () => {
    vi.useFakeTimers();
    try {
      const calls: string[] = [];
      const client: SessionDraftRpcV1 = {
        read: async () => ({ sessionId: "unused", text: "", attachments: [] }),
        write: async (sessionId, text, attachments) => {
          calls.push(text);
          return { sessionId, text, attachments: [...attachments] };
        },
      };
      const coordinator = new SessionDraftWriteCoordinatorV1(client, 500);
      coordinator.enqueue("session-a", "a", [], true);
      await vi.advanceTimersByTimeAsync(10);
      coordinator.enqueue("session-a", "ab", [], true);
      coordinator.enqueue("session-a", "ab + file", [attachment("file.txt")]);
      await vi.advanceTimersByTimeAsync(0);
      expect(calls).toEqual(["a", "ab + file"]);
      coordinator.enqueue("session-a", "ab + file!", [attachment("file.txt")], true);
      await coordinator.flush("session-a");
      expect(calls).toEqual(["a", "ab + file", "ab + file!"]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("rejects new writes after disposal", () => {
    const client: SessionDraftRpcV1 = {
      read: async () => ({ sessionId: "unused", text: "", attachments: [] }),
      write: async (sessionId, text, attachments) => ({
        sessionId,
        text,
        attachments: [...attachments],
      }),
    };
    const coordinator = new SessionDraftWriteCoordinatorV1(client);

    coordinator.dispose();

    expect(() => coordinator.enqueue("session-a", "draft", []))
      .toThrow(/has been disposed/);
  });
});
