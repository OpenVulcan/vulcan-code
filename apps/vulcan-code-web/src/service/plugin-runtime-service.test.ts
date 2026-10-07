import { beforeEach, expect, it, vi } from "vitest";
import wire from "./__fixtures__/plugin-runtime-wire.json";
import { executePluginRuntimeCommand, readPluginRuntime, type PluginRuntimeCommand } from "./plugin-runtime-service";
import { parseCapacityReceipt, parseCapacityStatus } from "./plugin-runtime-contract";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

vi.mock("./webui-service-runtime", () => ({ hasServiceRuntime: vi.fn(), serviceClient: { request: vi.fn() } }));
beforeEach(() => { vi.clearAllMocks(); vi.mocked(hasServiceRuntime).mockReturnValue(true); });

// Separate reads must agree on their parent before either can authorize an edit.
// 独立读取须在父级上一致，才能授权编辑。
it("rejects a restart between read RPCs and never creates a parent in preview", async () => {
  vi.mocked(serviceClient.request).mockImplementation(async (method) => method === "service_plugin_runtime_status" ? wire.runtime
    : method === "service_plugin_pending_operations" ? [] : { ...wire.catalog, runtimeId: "new-runtime", capacities: [] });
  await expect(readPluginRuntime()).rejects.toThrow("changed");
  expect(serviceClient.request).toHaveBeenCalledTimes(3);
  vi.mocked(hasServiceRuntime).mockReturnValue(false);
  await expect(readPluginRuntime()).rejects.toThrow("requires");
  expect(serviceClient.request).toHaveBeenCalledTimes(3);
});

// Receipt actions come from the unified registry only when it describes the same receipt in the same phase.
// 仅当统一登记表描述相同阶段的同一回执时，回执动作才来自该登记表。
it("takes receipt actions from the registry row of the same receipt and phase", async () => {
  // listed is the registry row of the observed receipt; offers are whatever the service declared.
  // listed 是已观测回执的登记表行；可用动作即服务所声明的内容。
  const receipt = wire.runtime.policyUpdate;
  const offers = [{ action: "cancel", requiresAcknowledgement: false }, { action: "resume", requiresAcknowledgement: false }];
  const listed = { kind: "capacityPolicy", operationId: receipt.request.updateId, pluginId: receipt.request.pluginId, actions: offers, detail: receipt };
  for (const [operations, expected] of [
    [[listed], offers],
    [[{ ...listed, detail: { ...receipt, phase: receipt.phase === "prepared" ? "applied" : "prepared" } }], []],
    [[], []],
  ] as const) {
    vi.mocked(serviceClient.request).mockImplementation(async (method) => method === "service_plugin_runtime_status" ? wire.runtime
      : method === "service_plugin_pending_operations" ? operations : wire.catalog);
    expect((await readPluginRuntime()).receiptActions).toEqual(expected);
  }
});

// Every command carries its original intent to the sole declared RPC; null is an explicit reset.
// 每个操作携带原意图到唯一声明 RPC；空值为明确继承重置。
it("sends each exact management command and validates acknowledgements", async () => {
  // receipt and registration come from the shared real-wire fixture.
  // receipt 和 registration 来自共享真实线协议夹具。
  const receipt = parseCapacityReceipt(wire.runtime.policyUpdate);
  const registration = parseCapacityStatus(wire.initialCapacity).managementRequest!;
  const reset = { ...receipt.request, config: null };
  const cases: Array<[PluginRuntimeCommand, string, unknown]> = [
    [{ kind: "register", request: registration }, "service_register_plugin_capacity", wire.initialCapacity],
    [{ kind: "update", request: reset }, "service_begin_plugin_capacity_policy_update", { ...wire.runtime.policyUpdate, request: reset }],
    [{ kind: "control", request: { runtimeId: receipt.request.runtimeId, capacityId: receipt.request.capacityId, expectedRevision: receipt.request.expectedRevision, pluginId: receipt.request.pluginId, purpose: receipt.request.purpose, action: "close" } }, "service_control_plugin_capacity", false],
    [{ kind: "resume", updateId: receipt.request.updateId }, "service_continue_plugin_pending_operation", { catalog: null }],
    [{ kind: "cancel", updateId: receipt.request.updateId }, "service_continue_plugin_pending_operation", { catalog: null }],
    [{ kind: "forget", updateId: receipt.request.updateId }, "service_continue_plugin_pending_operation", { catalog: null }],
  ];
  for (const [command, method, acknowledgement] of cases) {
    vi.mocked(serviceClient.request).mockResolvedValueOnce(acknowledgement);
    await executePluginRuntimeCommand(command);
    expect(serviceClient.request).toHaveBeenLastCalledWith(method, "request" in command ? { request: command.request }
      : { kind: "capacityPolicy", operationId: command.updateId, action: command.kind });
  }
});

// A lost transport or invalid response remains uncertain without adding a service-layer retry.
// 传输丢失或无效响应保持不确定，不增加服务层重试。
it("does not add a service-layer retry after transport or response validation failure", async () => {
  // command identifies one retained transaction throughout both explicit attempts.
  // command 在两次明确尝试中标识同一个保留事务。
  const command: PluginRuntimeCommand = { kind: "resume", updateId: wire.runtime.policyUpdate.request.updateId };
  vi.mocked(serviceClient.request).mockRejectedValueOnce(new Error("response lost"));
  await expect(executePluginRuntimeCommand(command)).rejects.toThrow("response lost");
  expect(serviceClient.request).toHaveBeenCalledTimes(1);
  vi.mocked(serviceClient.request).mockResolvedValueOnce({});
  await expect(executePluginRuntimeCommand(command)).rejects.toThrow();
  expect(serviceClient.request).toHaveBeenCalledTimes(2);
});
