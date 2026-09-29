import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";
import { assertPluginPrewarmOwner, parsePluginPrewarmCatalog, parsePluginPrewarmReceipt } from "./plugin-prewarm-contract";
import type { PluginPrewarmCatalog, PluginPrewarmIdentity, PluginPrewarmOwner, PluginPrewarmPreparation, PluginPrewarmReceipt } from "@/types/plugin-prewarm";

// PluginPrewarmCommand retains explicit intent; uncertain preparation must never be automatically repeated.
// PluginPrewarmCommand 保留明确意图；结果不确定的准备绝不能自动重复。
export type PluginPrewarmCommand =
  | { kind: "prepare"; request: PluginPrewarmPreparation }
  | { kind: "start" | "cancel" | "recover" | "forget"; request: PluginPrewarmIdentity };

// requireService rejects preview mode before dispatch; returns nothing and creates no runtime.
// requireService 在分发前拒绝预览模式；无返回值且不创建运行时。
function requireService(): void {
  if (!hasServiceRuntime()) throw new Error("Plugin prewarm management requires an attached Core service");
}

// readPluginPrewarmCatalog reads original receipts for owner without preparing or starting work.
// readPluginPrewarmCatalog 读取 owner 的原回执，不准备或启动工作。
export async function readPluginPrewarmCatalog(owner: PluginPrewarmOwner): Promise<PluginPrewarmCatalog> {
  requireService();
  // result must belong to the requested scope even when reconnection races the read.
  // 即使重连与读取竞争，result 也必须属于请求范围。
  const result = parsePluginPrewarmCatalog(await serviceClient.request<unknown>("service_plugin_prewarm_targets", {
    runtimeId: owner.runtimeId, ownerSessionId: owner.ownerSessionId,
  }));
  assertPluginPrewarmOwner(result, owner);
  return result;
}

// receiptRequest strips display-only fields and returns the sole accepted action parameter shape.
// receiptRequest 去除展示字段，返回动作唯一接受的参数形状。
function receiptRequest(identity: PluginPrewarmIdentity): PluginPrewarmIdentity {
  return { runtimeId: identity.runtimeId, ownerSessionId: identity.ownerSessionId, invocationId: identity.invocationId };
}

// acknowledgement checks original receipt identity and returns a validated host response.
// acknowledgement 核对原回执身份并返回已校验宿主响应。
function acknowledgement(value: unknown, identity: PluginPrewarmIdentity): PluginPrewarmReceipt {
  // receipt may have advanced in lifecycle but may never change routing identity.
  // receipt 的生命周期可以推进，但路由身份绝不能变化。
  const receipt = parsePluginPrewarmReceipt(value);
  assertPluginPrewarmOwner(receipt, identity);
  if (receipt.invocationId !== identity.invocationId) throw new Error("prewarm: response invocation mismatch");
  return receipt;
}

// readPluginPrewarmReceipt queries exactly identity; a missing original is an error, never a replacement search.
// readPluginPrewarmReceipt 精确查询 identity；原回执缺失时报错，绝不搜索替代。
export async function readPluginPrewarmReceipt(identity: PluginPrewarmIdentity): Promise<PluginPrewarmReceipt> {
  requireService();
  return acknowledgement(await serviceClient.request<unknown>("service_plugin_prewarm_status", { ...receiptRequest(identity) }), identity);
}

// executePluginPrewarmCommand sends one intent and validates its acknowledgement; null means successful forget.
// executePluginPrewarmCommand 发送单个意图并校验确认结果；空值表示成功遗忘。
// Shared authentication transport can resend identical parameters; rejection never proves non-execution.
// 共享认证传输可能重发相同参数；拒绝结果绝不证明未执行。
export async function executePluginPrewarmCommand(command: PluginPrewarmCommand): Promise<PluginPrewarmReceipt | null> {
  requireService();
  if (command.kind === "prepare") {
    // request copies only wire fields, excluding workspace paths and catalog limits.
    // request 仅复制线协议字段，排除工作区路径及目录上限。
    const { runtimeId, ownerSessionId, pluginId, toolId, pluginGeneration, targetIdle } = command.request;
    const request = { runtimeId, ownerSessionId, pluginId, toolId, pluginGeneration, targetIdle };
    const receipt = parsePluginPrewarmReceipt(await serviceClient.request<unknown>("service_prepare_plugin_prewarm", { request }));
    assertPluginPrewarmOwner(receipt, request);
    if (receipt.pluginId !== pluginId || receipt.toolId !== toolId || receipt.pluginGeneration !== pluginGeneration || receipt.targetIdle !== targetIdle) {
      throw new Error("prewarm: response preparation mismatch");
    }
    return receipt;
  }
  // methods maps declared actions directly; no business fallback or inferred method name is used.
  // methods 直接映射声明动作；不使用业务回退或推断的方法名称。
  const methods = { start: "service_start_plugin_prewarm", cancel: "service_cancel_plugin_prewarm",
    recover: "service_recover_plugin_prewarm_cleanup", forget: "service_forget_plugin_prewarm" } as const;
  const value = await serviceClient.request<unknown>(methods[command.kind], { ...receiptRequest(command.request) });
  if (command.kind === "forget") {
    if (value !== null) throw new Error("prewarm forget: expected null");
    return null;
  }
  return acknowledgement(value, command.request);
}
