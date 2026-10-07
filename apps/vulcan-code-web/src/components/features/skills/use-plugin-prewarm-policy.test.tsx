// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import wire from "@/service/__fixtures__/plugin-prewarm-policy-wire.json";
import { parsePluginPrewarmPolicyStatus } from "@/service/plugin-prewarm-policy-contract";
import { prewarmReceiptOffers } from "@/service/__fixtures__/plugin-pending-offers";
import { executePluginPrewarmPolicyCommand, readPluginPrewarmPolicyObservation, type PluginPrewarmPolicyCommand } from "@/service/plugin-prewarm-policy-service";
import type { PluginPrewarmPolicyReceipt, PluginPrewarmPolicyStatus } from "@/types/plugin-prewarm-policy";
import { usePluginPrewarmPolicy } from "./use-plugin-prewarm-policy";

vi.mock("@/service/plugin-prewarm-policy-service", () => ({ executePluginPrewarmPolicyCommand: vi.fn(), readPluginPrewarmPolicyObservation: vi.fn() }));
// readStatus supplies the atomic status read; the observation pairs it with the registry's declared receipt offers.
// readStatus 提供原子状态读取；观测将其与登记表声明的回执动作组合。
const readStatus = vi.fn<() => Promise<PluginPrewarmPolicyStatus>>();
// Real service wire fixtures supply policy identities; only asynchronous delivery is mocked.
// 真实服务线协议夹具提供策略身份；仅模拟异步传递。
const initial = parsePluginPrewarmPolicyStatus(wire.initial);
const published = parsePluginPrewarmPolicyStatus(wire.published);
const receipt = published.update!;
const command: PluginPrewarmPolicyCommand = { kind: "begin", request: receipt.request };
// The observer root exposes public hook actions without mutating its internal state.
// 观测根暴露公开钩子操作，不修改其内部状态。
let host: HTMLDivElement;
let root: Root;
let state: ReturnType<typeof usePluginPrewarmPolicy>;

// Probe mounts the production hook and exposes its current controls; returns no DOM.
// Probe 挂载生产钩子并暴露当前控件；不返回 DOM。
function Probe() { state = usePluginPrewarmPolicy(); return null; }

// deferred returns explicit completion controls for one original transport operation.
// deferred 返回单个原传输操作的明确完成控制。
function deferred<T>() {
  // resolve and reject are assigned during synchronous promise construction.
  // resolve 与 reject 在同步 Promise 构造时赋值。
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  vi.mocked(readPluginPrewarmPolicyObservation).mockImplementation(async () => {
    const status = await readStatus();
    return { status, receiptActions: prewarmReceiptOffers(status.update) };
  });
  readStatus.mockResolvedValue(initial);
  vi.mocked(executePluginPrewarmPolicyCommand).mockResolvedValue(receipt);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<Probe />));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Only an explicit complete read can authorize a mutation, including within a single render tick.
// 仅明确完整读取能授权修改，包括同一渲染时刻。
it("stays inert on mount and blocks commands during an in-flight read", async () => {
  expect(readStatus).not.toHaveBeenCalled();
  await act(async () => state.execute(command));
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
  await act(async () => state.refresh());
  const waiting = deferred<PluginPrewarmPolicyStatus>();
  readStatus.mockReturnValueOnce(waiting.promise);
  await act(async () => { void state.refresh(); void state.execute(command); });
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
  await act(async () => waiting.resolve(initial));
  expect(state.observation.fresh).toBe(true);
});

// Lost responses preserve a copied original intent and require an explicit read before acknowledging uncertainty.
// 响应丢失保留已复制原意图，确认不确定性前必须明确读取。
it("retains unknown intent without same-tick duplication or automatic replay", async () => {
  await act(async () => state.refresh());
  const waiting = deferred<PluginPrewarmPolicyReceipt | null>();
  const submitted = structuredClone(command);
  vi.mocked(executePluginPrewarmPolicyCommand).mockReturnValueOnce(waiting.promise);
  await act(async () => { void state.execute(submitted); void state.execute(submitted); });
  submitted.request.rules.length = 0;
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
  expect(state.observation.pending).toEqual(command);
  await act(async () => waiting.reject(new Error("response lost")));
  await act(async () => state.acknowledgeUncertainty());
  expect(state.observation.pending).toEqual(command);
  readStatus.mockResolvedValue(published);
  await act(async () => state.refresh());
  await act(async () => state.execute(command));
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
  await act(async () => state.acknowledgeUncertainty());
  expect(state.observation.pending).toBeNull();
  expect(state.observation.status?.update).toEqual(receipt);
});

// Acknowledged writes remain visible if a later read fails; stale evidence never permits another write.
// 后续读取失败时已确认写入仍可见；陈旧证据绝不允许另一写入。
it("keeps acknowledgement separate from a failed post-write read", async () => {
  await act(async () => state.refresh());
  readStatus.mockRejectedValueOnce(new Error("read failed"));
  await act(async () => state.execute(command));
  expect(state.observation.pending).toBeNull();
  expect(state.observation.acknowledged).toEqual(receipt);
  expect(state.observation.fresh).toBe(false);
  expect(state.observation.error).toContain("read failed");
  await act(async () => state.execute(command));
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
});

// Parent replacement, changed revision and retained capacity or policy transactions are independent write fences.
// 父级替换、修订变化及保留容量或策略事务分别构成独立写入屏障。
it("rejects stale drafts and competing retained transactions", async () => {
  for (const status of [{ ...initial, runtimeId: "replacement" }, { ...initial, revision: "newer" },
    { ...initial, capacityUpdateRetained: true }, published]) {
    readStatus.mockResolvedValue(status);
    await act(async () => state.refresh());
    await act(async () => state.execute(command));
  }
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
});

// Recovery uses exact retained identity even after restart; publishing cannot be cancelled or forgotten.
// 重启后恢复仍使用精确保留身份；发布中不能取消或遗忘。
it("allows only checkpoint-appropriate operations on the exact historical receipt", async () => {
  readStatus.mockResolvedValue({ ...published, runtimeId: "replacement",
    update: { ...receipt, phase: "publishing", persistenceError: "confirmation lost" } });
  await act(async () => state.refresh());
  await act(async () => state.execute({ kind: "cancel", request: receipt.request }));
  await act(async () => state.execute({ kind: "forget", request: receipt.request }));
  await act(async () => state.execute({ kind: "resume", request: { ...receipt.request, runtimeId: "replacement" } }));
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
  await act(async () => state.execute({ kind: "resume", request: receipt.request }));
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "resume", request: receipt.request });
  readStatus.mockResolvedValue(published);
  await act(async () => state.refresh());
  vi.mocked(executePluginPrewarmPolicyCommand).mockResolvedValueOnce(null);
  readStatus.mockResolvedValue({ ...published, update: null });
  await act(async () => state.execute({ kind: "forget", request: receipt.request }));
  expect(state.observation.status?.update).toBeNull();
});

// Unmounting stops observation only; it cannot advance a transaction or publish a late response into a new view.
// 卸载仅停止观测；不能推进事务或将迟到响应发布到新视图。
it("drops late observer responses without cancelling admitted host work", async () => {
  await act(async () => state.refresh());
  const waiting = deferred<PluginPrewarmPolicyReceipt | null>();
  vi.mocked(executePluginPrewarmPolicyCommand).mockReturnValueOnce(waiting.promise);
  await act(async () => { void state.execute(command); });
  await act(async () => root.render(null));
  await act(async () => root.render(<Probe />));
  await act(async () => waiting.resolve(receipt));
  expect(state.observation.status).toBeNull();
  expect(readStatus).toHaveBeenCalledTimes(1);
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
});
