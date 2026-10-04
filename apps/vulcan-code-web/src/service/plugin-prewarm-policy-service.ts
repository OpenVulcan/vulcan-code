import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";
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

// executePluginPrewarmPolicyCommand sends one explicit original command; returns validated receipt or null on forget.
// executePluginPrewarmPolicyCommand 发送单个明确原命令；返回已校验回执，遗忘时返回空值。
// Failed transport never implies rollback; the original UUID must be observed before another change.
// 传输失败绝不表示回滚；另一变更前必须观测原 UUID。
export async function executePluginPrewarmPolicyCommand(command: PluginPrewarmPolicyCommand): Promise<PluginPrewarmPolicyReceipt | null> {
  requireService();
  // methods uses exact registered names rather than deriving an unchecked service method.
  // methods 使用精确注册名称，不推导未经检查的服务方法。
  const methods = { begin: "service_begin_plugin_prewarm_policy_update", resume: "service_resume_plugin_prewarm_policy_update",
    cancel: "service_cancel_plugin_prewarm_policy_update", forget: "service_forget_plugin_prewarm_policy_update" } as const;
  const { updateId, runtimeId, expectedRevision } = command.request;
  const rules = command.request.rules.map(({ pluginId, toolId, targetIdle }) => ({ pluginId, toolId, targetIdle }));
  const params = command.kind === "begin" ? { request: { updateId, runtimeId, expectedRevision, rules } } : { updateId };
  const value = await serviceClient.request<unknown>(methods[command.kind], params);
  if (command.kind === "forget") {
    if (value !== null) throw new Error("prewarm policy forget: expected null");
    return null;
  }
  const receipt = parsePluginPrewarmPolicyReceipt(value);
  if (!samePluginPrewarmPolicyRequest(receipt.request, command.request)) throw new Error("prewarm policy: response request mismatch");
  return receipt;
}
