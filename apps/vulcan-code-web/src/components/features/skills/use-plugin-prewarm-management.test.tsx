// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import wire from "@/service/__fixtures__/plugin-prewarm-wire.json";
import { parsePluginPrewarmCatalog, parsePluginPrewarmReceipt } from "@/service/plugin-prewarm-contract";
import { executePluginPrewarmCommand, readPluginPrewarmCatalog, type PluginPrewarmCommand } from "@/service/plugin-prewarm-service";
import type { PluginPrewarmCatalog, PluginPrewarmOwner, PluginPrewarmReceipt } from "@/types/plugin-prewarm";
import { usePluginPrewarmManagement } from "./use-plugin-prewarm-management";

vi.mock("@/service/plugin-prewarm-service", () => ({ executePluginPrewarmCommand: vi.fn(), readPluginPrewarmCatalog: vi.fn() }));
// host/root own only this observer; the fixture's service receipts survive component selection changes.
// host/root 仅拥有此观测者；夹具服务回执跨组件选择变化保留。
let host: HTMLDivElement;
let root: Root;
let state: ReturnType<typeof usePluginPrewarmManagement>;
const catalog = parsePluginPrewarmCatalog(wire.catalog);
const receipt = parsePluginPrewarmReceipt(wire.prepared);
const owner: PluginPrewarmOwner = { runtimeId: catalog.runtimeId, ownerSessionId: catalog.ownerSessionId };
const other: PluginPrewarmOwner = { ...owner, ownerSessionId: "00000000-0000-4000-8000-000000000002" };
const preparation: PluginPrewarmCommand = { kind: "prepare", request: { ...owner, pluginId: receipt.pluginId,
  toolId: receipt.toolId, pluginGeneration: receipt.pluginGeneration, targetIdle: receipt.targetIdle } };

// Probe mounts the real hook for a selected owner and exposes only its public actions to assertions.
// Probe 为选中归属挂载真实钩子，仅向断言暴露其公开操作。
function Probe(props: { owner: PluginPrewarmOwner }) { state = usePluginPrewarmManagement(props.owner); return null; }

// deferred returns explicit resolution controls for the original async response.
// deferred 返回原异步响应的明确完成控制。
function deferred<T>() {
  // resolve/reject are assigned synchronously before the returned object is used.
  // resolve/reject 在返回对象被使用前同步赋值。
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  vi.mocked(readPluginPrewarmCatalog).mockImplementation(async (requested) => ({ ...catalog, ...requested }));
  vi.mocked(executePluginPrewarmCommand).mockResolvedValue(receipt);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<Probe owner={owner} />));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// No effect or selection change may initialize a plugin, including before a catalog read.
// 任何副作用或选择变化都不能初始化插件，包括目录读取之前。
it("is inert until explicit reads and prevents writes during a same-tick refresh", async () => {
  expect(readPluginPrewarmCatalog).not.toHaveBeenCalled();
  await act(async () => state.execute(preparation));
  expect(executePluginPrewarmCommand).not.toHaveBeenCalled();
  await act(async () => state.refresh());
  const waiting = deferred<PluginPrewarmCatalog>();
  vi.mocked(readPluginPrewarmCatalog).mockReturnValueOnce(waiting.promise);
  await act(async () => { void state.refresh(); void state.execute(preparation); });
  expect(executePluginPrewarmCommand).not.toHaveBeenCalled();
  await act(async () => waiting.resolve(catalog));
  expect(state.observation.fresh).toBe(true);
});

// Transport rejection cannot justify a duplicate preparation, even after a successful recovery read.
// 即使恢复读取成功，传输拒绝也不能成为重复准备的依据。
it("retains uncertainty through refresh and never offers implicit retry", async () => {
  await act(async () => state.refresh());
  const waiting = deferred<PluginPrewarmReceipt | null>();
  vi.mocked(executePluginPrewarmCommand).mockReturnValueOnce(waiting.promise);
  await act(async () => { void state.execute(preparation); void state.execute(preparation); });
  expect(executePluginPrewarmCommand).toHaveBeenCalledTimes(1);
  await act(async () => waiting.reject(new Error("response lost")));
  expect(state.observation.pending).toBe(preparation);
  await act(async () => state.acknowledgeUncertainty());
  expect(state.observation.pending).toBe(preparation);
  await act(async () => state.refresh());
  await act(async () => state.execute(preparation));
  expect(executePluginPrewarmCommand).toHaveBeenCalledTimes(1);
  expect(state.observation.pending).toBe(preparation);
  await act(async () => state.acknowledgeUncertainty());
  expect(state.observation.pending).toBeNull();
});

// Switching sessions does not redirect a delayed response or forget the original uncertainty.
// 切换会话不会重定向延迟响应，也不会遗忘原不确定结果。
it("routes late completions only to the original owner across session and runtime changes", async () => {
  await act(async () => state.refresh());
  const waiting = deferred<PluginPrewarmReceipt | null>();
  vi.mocked(executePluginPrewarmCommand).mockReturnValueOnce(waiting.promise);
  await act(async () => { void state.execute(preparation); });
  await act(async () => root.render(<Probe owner={other} />));
  await act(async () => waiting.reject(new Error("original response lost")));
  expect(state.observation.pending).toBeNull();
  expect(state.observation.catalog).toBeNull();
  await act(async () => state.refresh());
  expect(state.observation.catalog?.ownerSessionId).toBe(other.ownerSessionId);
  await act(async () => root.render(<Probe owner={owner} />));
  expect(state.observation.pending).toBe(preparation);
  expect(state.observation.error).toContain("original response lost");
  await act(async () => root.render(<Probe owner={{ ...owner, runtimeId: "replacement" }} />));
  expect(state.observation.catalog).toBeNull();
  await act(async () => state.execute(preparation));
  expect(executePluginPrewarmCommand).toHaveBeenCalledTimes(1);
});

// Acknowledged preparation is retained if the subsequent catalog read fails; it is never repeated.
// 后续目录读取失败时保留已确认准备；绝不重复准备。
it("keeps an acknowledged original receipt when the following read fails", async () => {
  await act(async () => state.refresh());
  vi.mocked(readPluginPrewarmCatalog).mockRejectedValueOnce(new Error("read offline"));
  await act(async () => state.execute(preparation));
  expect(state.observation.pending).toBeNull();
  expect(state.observation.catalog?.targets).toEqual([receipt]);
  expect(state.observation.fresh).toBe(false);
  expect(state.observation.error).toContain("read offline");
  await act(async () => state.execute(preparation));
  expect(executePluginPrewarmCommand).toHaveBeenCalledTimes(1);
});

// Original receipt actions stay fenced, and removing a receipt never fabricates a successful replacement.
// 原回执操作保持身份约束；移除回执绝不构造成功替代物。
it("forgets only the acknowledged original and keeps unrelated evidence", async () => {
  const second = { ...receipt, invocationId: "second" };
  vi.mocked(readPluginPrewarmCatalog).mockResolvedValueOnce({ ...catalog, targets: [receipt, second] });
  await act(async () => state.refresh());
  vi.mocked(executePluginPrewarmCommand).mockResolvedValueOnce(null);
  vi.mocked(readPluginPrewarmCatalog).mockRejectedValueOnce(new Error("after forget offline"));
  await act(async () => state.execute({ kind: "forget", request: receipt }));
  expect(state.observation.catalog?.targets).toEqual([second]);
  expect(state.observation.pending).toBeNull();
});
