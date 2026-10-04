import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { parseCapacityCatalog, parseCapacityReceipt, parseCapacityStatus, parsePluginRuntime } from "./plugin-runtime-contract";
import type {
  PluginCapacityCatalog, PluginCapacityPredecessor, PluginCapacityRegistration, PluginCapacityUpdate, PluginRuntimeView,
} from "@/types/plugin-runtime";

// PluginRuntimeObservation pairs separate reads after verifying their native parent identity.
// PluginRuntimeObservation 核对原生父级身份后组合独立读取结果。
export interface PluginRuntimeObservation {
  // runtime is absent only when the service has no attached formal parent.
  // runtime 仅在服务尚未附加正式父级时为空。
  runtime: PluginRuntimeView | null;
  // catalog is the host mapping observation, not a global atomic resource sample.
  // catalog 为宿主映射观测，而非全局原子资源样本。
  catalog: PluginCapacityCatalog | null;
}

// PluginRuntimeCommand preserves one original explicit mutation through transport uncertainty.
// PluginRuntimeCommand 跨传输不确定性保留单个原始明确修改。
export type PluginRuntimeCommand =
  | { kind: "register"; request: PluginCapacityRegistration }
  | { kind: "update"; request: PluginCapacityUpdate }
  | { kind: "control"; request: PluginCapacityPredecessor & { action: "close" | "forget" } }
  | { kind: "resume" | "cancel" | "forget"; updateId: string };

// readPluginRuntime reads existing state, verifies matching parents and returns it without creating resources.
// readPluginRuntime 读取现有状态、核对父级一致性后返回，不创建资源。
export async function readPluginRuntime(): Promise<PluginRuntimeObservation> {
  if (!hasServiceRuntime()) throw new Error("Plugin runtime management requires an attached Core service");
  // Independent RPCs may race a restart; reject mixed generations instead of combining them.
  // 独立 RPC 可能与重启竞争；拒绝混合代际，而非拼接。
  const [runtime, catalog] = await Promise.all([
    serviceClient.request<unknown>("service_plugin_runtime_status", {}).then(parsePluginRuntime),
    serviceClient.request<unknown>("service_plugin_capacity_catalog", {}).then(parseCapacityCatalog),
  ]);
  if (runtime?.runtimeId !== catalog?.runtimeId) throw new Error("Plugin runtime changed during observation; refresh explicitly");
  return { runtime, catalog };
}

// executePluginRuntimeCommand makes one service-client call for the exact intent and validates its acknowledgement.
// executePluginRuntimeCommand 为精确意图调用一次服务客户端，并校验确认结果。
// The shared transport may refresh authentication after HTTP 401 and resend the identical parameters.
// 共享传输层可能在 HTTP 401 后刷新认证，并重发完全相同的参数。
// A rejected promise is not proof of non-execution; callers retain command for explicit recovery.
// Promise 拒绝不代表未执行；调用方保留 command 用于明确恢复。
export async function executePluginRuntimeCommand(command: PluginRuntimeCommand): Promise<void> {
  if (!hasServiceRuntime()) throw new Error("Plugin runtime management requires an attached Core service");
  switch (command.kind) {
    case "register":
      parseCapacityStatus(await serviceClient.request<unknown>("service_register_plugin_capacity", { request: command.request }));
      return;
    case "update":
      parseCapacityReceipt(await serviceClient.request<unknown>("service_begin_plugin_capacity_policy_update", { request: command.request }));
      return;
    case "control": {
      // found denotes mapping presence only; neither value proves physical drain.
      // found 仅表示映射存在性；任何值都不证明物理排空。
      const found = await serviceClient.request<unknown>("service_control_plugin_capacity", { request: command.request });
      if (typeof found !== "boolean") throw new Error("capacity control: expected boolean");
      return;
    }
    case "resume":
    case "cancel":
      parseCapacityReceipt(await serviceClient.request<unknown>(
        command.kind === "resume" ? "service_resume_plugin_capacity_policy_update" : "service_cancel_plugin_capacity_policy_update",
        { updateId: command.updateId },
      ));
      return;
    case "forget":
      if (await serviceClient.request<unknown>("service_forget_plugin_capacity_policy_update", { updateId: command.updateId }) !== null) {
        throw new Error("policy acknowledgement: expected null");
      }
  }
}
