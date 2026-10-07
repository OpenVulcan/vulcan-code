// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import wire from "@/service/__fixtures__/plugin-runtime-wire.json";
import { parseCapacityCatalog, parsePluginRuntime } from "@/service/plugin-runtime-contract";
import { executePluginRuntimeCommand, readPluginRuntime, type PluginRuntimeCommand, type PluginRuntimeObservation } from "@/service/plugin-runtime-service";
import { usePluginRuntimeManagement } from "./use-plugin-runtime-management";

vi.mock("@/service/plugin-runtime-service", () => ({ executePluginRuntimeCommand: vi.fn(), readPluginRuntime: vi.fn() }));
// host/root isolate the view lifecycle; state exposes only the hook's public actions to this fixture.
// host/root 隔离视图生命周期；state 仅向此夹具暴露钩子的公开操作。
let host: HTMLDivElement;
let root: Root;
let state: ReturnType<typeof usePluginRuntimeManagement>;
// observation reuses the Rust wire contract, including its globally retained receipt.
// observation 复用 Rust 线协议契约，包括全局保留回执。
const observation: PluginRuntimeObservation = { runtime: parsePluginRuntime(wire.runtime), catalog: parseCapacityCatalog(wire.catalog), receiptActions: [] };

// Probe renders a real hook instance and captures its current return value for behavioral assertions.
// Probe 渲染真实钩子实例，捕获当前返回值用于行为断言。
function Probe() { state = usePluginRuntimeManagement(); return null; }

// deferred returns a controllable promise, allowing a test to reverse actual completion order.
// deferred 返回可控 Promise，使测试能逆转实际完成顺序。
function deferred<T>() {
  // resolve/reject are assigned synchronously by Promise and used only after construction.
  // resolve/reject 由 Promise 同步赋值，仅在构造后使用。
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  vi.mocked(readPluginRuntime).mockResolvedValue(observation);
  vi.mocked(executePluginRuntimeCommand).mockResolvedValue();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Mounting discovers a retained global checkpoint but must never advance or acknowledge it.
// 挂载发现保留全局检查点，但绝不推进或确认它。
it("reads retained transactions on mount without any mutation", async () => {
  await act(async () => root.render(<Probe />));
  expect(state.fresh).toBe(true);
  expect(state.observation?.runtime?.policyUpdate).toEqual(observation.runtime!.policyUpdate);
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
});

// A delayed earlier read cannot overwrite a newer complete observation.
// 较早的延迟读取不得覆盖较新的完整观测。
it("ignores out-of-order reads and refuses same-tick writes during refresh", async () => {
  // oldRead holds the mount response until an explicit later read has completed.
  // oldRead 保持挂载响应，直到明确后续读取完成。
  const oldRead = deferred<PluginRuntimeObservation>();
  vi.mocked(readPluginRuntime).mockReturnValueOnce(oldRead.promise);
  await act(async () => root.render(<Probe />));
  await act(async () => state.refresh());
  expect(state.observation).toEqual(observation);
  await act(async () => { oldRead.resolve({ runtime: null, catalog: null, receiptActions: [] }); });
  expect(state.observation).toEqual(observation);
  // nextRead revokes eligibility before React has updated the disabled controls.
  // nextRead 在 React 更新禁用控件之前撤销资格。
  const nextRead = deferred<PluginRuntimeObservation>();
  vi.mocked(readPluginRuntime).mockReturnValueOnce(nextRead.promise);
  await act(async () => {
    void state.refresh();
    await state.execute({ kind: "forget", updateId: wire.runtime.policyUpdate.request.updateId });
  });
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  await act(async () => nextRead.resolve(observation));
});

// A failed write retains exactly one intent; neither duplicate clicks nor new requests may replace it.
// 失败写入保留唯一精确意图；重复点击和新请求均不得替换它。
it("retains an uncertain original intent and permits only an explicit identical retry", async () => {
  await act(async () => root.render(<Probe />));
  // original is the same object across the failed attempt, inspection and explicit retry.
  // original 在失败尝试、检查及明确重试之间为同一对象。
  const original: PluginRuntimeCommand = { kind: "resume", updateId: wire.runtime.policyUpdate.request.updateId };
  const response = deferred<void>();
  vi.mocked(executePluginRuntimeCommand).mockReturnValueOnce(response.promise);
  await act(async () => { void state.execute(original); void state.execute(original); });
  expect(executePluginRuntimeCommand).toHaveBeenCalledTimes(1);
  await act(async () => response.reject(new Error("response lost")));
  expect(state.pending).toBe(original);
  expect(state.error).toContain("response lost");
  expect(state.fresh).toBe(false);
  await act(async () => state.execute({ ...original }));
  expect(executePluginRuntimeCommand).toHaveBeenCalledTimes(1);
  await act(async () => state.refresh());
  expect(state.pending).toBe(original);
  await act(async () => state.execute(original));
  expect(executePluginRuntimeCommand).toHaveBeenCalledTimes(2);
  expect(vi.mocked(executePluginRuntimeCommand).mock.calls[1][0]).toBe(original);
  expect(state.pending).toBeNull();
});

// Read failures retain visible evidence but revoke all new mutation eligibility.
// 读取失败保留可见证据，但撤销全部新修改资格。
it("does not submit from stale state after a failed refresh", async () => {
  await act(async () => root.render(<Probe />));
  vi.mocked(readPluginRuntime).mockRejectedValueOnce(new Error("offline"));
  await act(async () => state.refresh());
  expect(state.observation).toEqual(observation);
  expect(state.fresh).toBe(false);
  await act(async () => state.execute({ kind: "forget", updateId: wire.runtime.policyUpdate.request.updateId }));
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
});

// A lost read after a confirmed write is not a reason to resubmit that write.
// 已确认写入后的读取丢失不构成重新提交该写入的理由。
it("keeps a confirmed mutation separate from subsequent observation failure", async () => {
  await act(async () => root.render(<Probe />));
  vi.mocked(readPluginRuntime).mockRejectedValueOnce(new Error("refresh failed"));
  await act(async () => state.execute({ kind: "forget", updateId: wire.runtime.policyUpdate.request.updateId }));
  expect(state.pending).toBeNull();
  expect(state.fresh).toBe(false);
  expect(state.error).toContain("refresh failed");
  expect(executePluginRuntimeCommand).toHaveBeenCalledTimes(1);
});

// Closing a view must not imply cancellation of an already accepted host mutation.
// 关闭视图不得意味着取消宿主已接纳的修改。
it("unmounts without cancelling or replaying accepted work", async () => {
  await act(async () => root.render(<Probe />));
  // response completes after the view has gone away.
  // response 在视图离开后完成。
  const response = deferred<void>();
  vi.mocked(executePluginRuntimeCommand).mockReturnValueOnce(response.promise);
  await act(async () => { void state.execute({ kind: "forget", updateId: wire.runtime.policyUpdate.request.updateId }); });
  await act(async () => root.render(null));
  await act(async () => response.resolve());
  expect(executePluginRuntimeCommand).toHaveBeenCalledTimes(1);
  expect(readPluginRuntime).toHaveBeenCalledTimes(1);
});
