import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";
import { continuePluginPendingOperation, pendingOffers, readPluginPendingOperations, type PluginPendingActionOffer } from "./plugin-pending-operations-service";
import { parsePluginPrewarmPolicyReceipt, parsePluginPrewarmPolicyStatus, samePluginPrewarmPolicyRequest } from "./plugin-prewarm-policy-contract";
import type { PluginPrewarmPolicyReceipt, PluginPrewarmPolicyStatus, PluginPrewarmPolicyUpdate } from "@/types/plugin-prewarm-policy";

// PluginPrewarmPolicyCommand retains the original full intent even when recovery only transmits its UUID.
// PluginPrewarmPolicyCommand 即使恢复仅传输 UUID，也保留原完整意图。
export type PluginPrewarmPolicyCommand = {
  // kind selects one declared policy operation, separate from VM execution.
  // kind 选择一个声明的策略操作，与 VM 执行分离。
  kind: "begin" | "resume" | "cancel" | "forget";
  // request verifies acknowledgements against the original caller-owned intent.
  // request 对照调用方拥有的原意图校验确认结果。
  request: PluginPrewarmPolicyUpdate;
};

// requireService rejects preview mode before any transport access; returns nothing.
// requireService 在任何传输访问前拒绝预览模式；无返回值。
function requireService(): void {
  if (!hasServiceRuntime()) throw new Error("Plugin prewarm policy requires an attached Core service");
}

// readPluginPrewarmPolicyStatus observes current parent and retained policy; it performs no write or initialization.
// readPluginPrewarmPolicyStatus 观测当前父级及保留策略；不写入或初始化。
export async function readPluginPrewarmPolicyStatus(): Promise<PluginPrewarmPolicyStatus> {
  requireService();
  return parsePluginPrewarmPolicyStatus(await serviceClient.request<unknown>("service_plugin_prewarm_policy_status", {}));
}

// PluginPrewarmPolicyObservation pairs the policy status with the service-declared continuations of its receipt.
// PluginPrewarmPolicyObservation 组合策略状态及其回执的服务端声明继续方式。
export interface PluginPrewarmPolicyObservation {
  // status is the atomic host projection.
  // status 为原子宿主投影。
  status: PluginPrewarmPolicyStatus;
  // receiptActions describe status.update in the same phase; empty without a receipt.
  // receiptActions 描述同一阶段的 status.update；无回执时为空。
  receiptActions: PluginPendingActionOffer[];
}

// readPluginPrewarmPolicyObservation reads status and the unified registry together; it performs no write.
// readPluginPrewarmPolicyObservation 同时读取状态与统一登记表；不执行写入。
export async function readPluginPrewarmPolicyObservation(): Promise<PluginPrewarmPolicyObservation> {
  const [status, operations] = await Promise.all([readPluginPrewarmPolicyStatus(), readPluginPendingOperations()]);
  const receipt = status.update;
  return { status, receiptActions: receipt === null ? [] : pendingOffers(operations, "prewarmPolicy", receipt.request.updateId, receipt.phase) };
}

// executePluginPrewarmPolicyCommand sends one explicit original command; returns the validated receipt of begin, otherwise null.
// executePluginPrewarmPolicyCommand 发送单个明确原命令；begin 返回已校验回执，其余返回空值。
// Failed transport never implies rollback; the original UUID must be observed before another change.
// 传输失败绝不表示回滚；另一变更前必须观测原 UUID。
export async function executePluginPrewarmPolicyCommand(command: PluginPrewarmPolicyCommand): Promise<PluginPrewarmPolicyReceipt | null> {
  requireService();
  const { updateId, runtimeId, expectedRevision } = command.request;
  if (command.kind !== "begin") {
    // Receipt continuations share the unified pending-operation entry and carry only the original UUID.
    // 回执继续共用统一待处理操作入口，仅携带原 UUID。
    if (await continuePluginPendingOperation({ kind: "prewarmPolicy", operationId: updateId, action: command.kind }) !== null) {
      throw new Error("prewarm policy continuation: unexpected catalog");
    }
    return null;
  }
  const rules = command.request.rules.map(({ pluginId, toolId, targetIdle }) => ({ pluginId, toolId, targetIdle }));
  const value = await serviceClient.request<unknown>("service_begin_plugin_prewarm_policy_update", { request: { updateId, runtimeId, expectedRevision, rules } });
  const receipt = parsePluginPrewarmPolicyReceipt(value);
  if (!samePluginPrewarmPolicyRequest(receipt.request, command.request)) throw new Error("prewarm policy: response request mismatch");
  return receipt;
}
