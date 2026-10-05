import { Buffer } from "node:buffer";
import { describe, expect, it } from "vitest";
import { translateActive } from "@/i18n";

import {
  mergeVisibleTerminalChunks,
  pruneRemovedProjectTerminalUiState,
  splitBinaryTerminalInput,
  splitUtf8TerminalInput,
} from "@/components/project/terminal/project-floating-terminal-layer";
import { acquireVisibleTerminalDisplayListener } from "@/components/terminal/terminal-display-listener";
import {
  clearProjectTerminalInputBuffer,
  clearProjectTerminalInputBuffers,
  getOrCreateProjectTerminalInputBuffer,
} from "@/components/project/terminal/project-terminal-input-buffer";

describe("project terminal data contracts", () => {
  // Verifies Unicode fragments never split a code point and preserve exact UTF-8 bytes.
  // 验证 Unicode 分片不会拆开码点，并保留精确 UTF-8 字节。
  it("preserves Unicode code-point boundaries", () => {
    const fragments = splitUtf8TerminalInput("a中🙂b", 4);
    expect(fragments.map((fragment) => fragment.event.utf8Data)).toEqual(["a中", "🙂", "b"]);
    expect(fragments.map((fragment) => fragment.byteLength)).toEqual([4, 4, 1]);
  });

  // Verifies xterm binary code units preserve their exact low-eight-bit representation.
  // 验证 xterm 二进制码元保留其精确低八位表示。
  it("preserves binary low-eight-bit bytes", () => {
    const fragments = splitBinaryTerminalInput(String.fromCharCode(0, 255, 0x1234), 2);
    const decoded = fragments.flatMap((fragment) => [
      ...Buffer.from(fragment.event.binaryDataBase64 ?? "", "base64"),
    ]);
    expect(decoded).toEqual([0, 255, 0x34]);
    expect(fragments.map((fragment) => fragment.byteLength)).toEqual([2, 1]);
  });

  // Verifies unacknowledged input survives visible-pane remounts and explicit close releases it.
  // 验证未确认输入可跨可见面板重挂载保留，且显式关闭会释放它。
  it("retains queued input across pane remounts until explicit close", () => {
    const projectId = "project-buffer-test";
    const terminalId = "terminal-buffer-test";
    clearProjectTerminalInputBuffer(projectId, terminalId);

    const firstMount = getOrCreateProjectTerminalInputBuffer(projectId, terminalId, 4);
    firstMount.pending.push({ seq: 5, event: { utf8Data: "中" }, byteLength: 3 });
    firstMount.pendingBytes = 3;
    firstMount.nextClientSeq = 6;
    firstMount.sending = true;
    firstMount.batchSettled = Promise.resolve();

    const remounted = getOrCreateProjectTerminalInputBuffer(projectId, terminalId, 1);
    expect(remounted).toBe(firstMount);
    expect(remounted.pending).toEqual([
      { seq: 5, event: { utf8Data: "中" }, byteLength: 3 },
    ]);
    expect(remounted.pendingBytes).toBe(3);
    expect(remounted.nextClientSeq).toBe(6);
    expect(remounted.sending).toBe(true);

    clearProjectTerminalInputBuffer(projectId, terminalId);
    const afterClose = getOrCreateProjectTerminalInputBuffer(projectId, terminalId, 9);
    expect(afterClose).not.toBe(firstMount);
    expect(afterClose.pending).toEqual([]);
    expect(afterClose.pendingBytes).toBe(0);
    expect(afterClose.nextClientSeq).toBe(10);
    expect(afterClose.sending).toBe(false);

    const secondTerminal = getOrCreateProjectTerminalInputBuffer(projectId, "terminal-buffer-test-2", 0);
    secondTerminal.pending.push({ seq: 1, event: { utf8Data: "x" }, byteLength: 1 });
    clearProjectTerminalInputBuffers(projectId);
    expect(getOrCreateProjectTerminalInputBuffer(projectId, terminalId, 12)).not.toBe(afterClose);
    expect(getOrCreateProjectTerminalInputBuffer(projectId, "terminal-buffer-test-2", 8)).not.toBe(secondTerminal);

    clearProjectTerminalInputBuffers(projectId);
  });

  // Verifies authoritative project removal prunes stale floating UI and open-request state.
  // 验证权威项目移除会裁剪陈旧浮动界面与打开请求状态。
  it("prunes floating terminal state for removed projects", () => {
    const projectStates = new Map([
      ["retained", { expanded: true }],
      ["removed", { expanded: false }],
    ]);
    const seenRequests = new Map([
      ["retained", 2],
      ["removed", 1],
    ]);

    pruneRemovedProjectTerminalUiState(projectStates, seenRequests, [{ id: "retained" }]);

    expect([...projectStates.keys()]).toEqual(["retained"]);
    expect([...seenRequests.keys()]).toEqual(["retained"]);
  });

  // Verifies React batching deduplicates output and exposes a bounded-history gap explicitly.
  // 验证 React 批处理去重输出，并明确显示有界历史缺口。
  it("deduplicates pushed chunks and marks an eviction gap", () => {
    const merged = mergeVisibleTerminalChunks(
      [{ seq: 3, stream: "stdout", text: "old" }],
      [
        { seq: 3, stream: "stdout", text: "duplicate" },
        { seq: 4, stream: "stdout", text: "new" },
      ],
      0,
    );
    expect(merged.map((chunk) => chunk.seq)).toEqual([3, 4]);

    const gap = mergeVisibleTerminalChunks(
      [],
      [{ seq: 6, stream: "stdout", text: "retained" }],
      5,
    );
    expect(gap.map((chunk) => chunk.seq)).toEqual([5, 6]);
    expect(gap.find((chunk) => chunk.seq === 5)?.text).toContain(translateActive("project.terminal.outputTruncated"));
  });

  // Verifies a pane hidden during asynchronous topic setup releases the late listener instead of leaking it.
  // 验证面板在异步主题建立期间隐藏时，会释放迟到的监听器而不是泄漏它。
  it("releases a display listener that resolves after tab switch", async () => {
    // resolveSubscription completes the deliberately delayed browser topic subscription.
    // resolveSubscription 完成被刻意延迟的浏览器主题订阅。
    let resolveSubscription: (unsubscribe: () => void) => void = () => {
      throw new Error("Display-listener test subscription was not initialized.");
    };
    // subscription creates the same pending promise shape returned by the browser transport.
    // subscription 创建与浏览器传输返回值相同形态的待完成 Promise。
    const subscription = new Promise<() => void>((resolve) => {
      resolveSubscription = resolve;
    });
    // cancelled models selecting a different terminal before the listener becomes available.
    // cancelled 模拟监听器可用前已选择另一个终端。
    let cancelled = false;
    // releaseCount records disposal of the listener that arrived too late.
    // releaseCount 记录迟到监听器的释放次数。
    let releaseCount = 0;
    // retainedCount records whether an invalid listener reached the active cleanup slot.
    // retainedCount 记录无效监听器是否进入活动清理槽位。
    let retainedCount = 0;
    // acquisition is the in-flight lifecycle operation under test.
    // acquisition 是本测试中的进行中生命周期操作。
    const acquisition = acquireVisibleTerminalDisplayListener(
      () => subscription,
      () => cancelled,
      () => {
        retainedCount += 1;
      },
    );

    cancelled = true;
    resolveSubscription(() => {
      releaseCount += 1;
    });

    await expect(acquisition).resolves.toBe(false);
    expect(releaseCount).toBe(1);
    expect(retainedCount).toBe(0);
  });

  // Verifies a still-visible terminal retains exactly one listener for its normal cleanup path.
  // 验证仍可见的终端恰好保留一个监听器供正常清理路径使用。
  it("retains a display listener while the surface remains visible", async () => {
    // releaseCount records disposal after the owner invokes its retained cleanup handle.
    // releaseCount 记录所有者调用保留的清理句柄后的释放次数。
    let releaseCount = 0;
    // retainedListeners stores the exact cleanup handle accepted by the visible surface.
    // retainedListeners 保存可见表面接收的精确清理句柄。
    const retainedListeners: Array<() => void> = [];

    await expect(acquireVisibleTerminalDisplayListener(
      async () => () => {
        releaseCount += 1;
      },
      () => false,
      (unsubscribe) => retainedListeners.push(unsubscribe),
    )).resolves.toBe(true);

    expect(retainedListeners).toHaveLength(1);
    retainedListeners[0]();
    expect(releaseCount).toBe(1);
  });
});
