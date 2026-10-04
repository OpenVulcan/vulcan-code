import { capacityCount, flag, optionalDiagnostic, record, text } from "./plugin-runtime-contract";
import type { PluginAutomaticPrewarmRule, PluginPrewarmPolicyPhase, PluginPrewarmPolicyReceipt, PluginPrewarmPolicyStatus, PluginPrewarmPolicyUpdate } from "@/types/plugin-prewarm-policy";

// rules validates a complete non-duplicated collection and returns explicitly declared positive targets.
// rules 校验完整无重复集合，返回明确声明的正数目标。
function rules(value: unknown): PluginAutomaticPrewarmRule[] {
  if (!Array.isArray(value)) throw new Error("prewarm policy rules: expected array");
  // toolIds applies the host's globally unique tool ownership, including conflicting plugin declarations.
  // toolIds 应用宿主全局唯一工具归属，包括冲突插件声明。
  const toolIds = new Set<string>();
  return value.map((item) => {
    // rule and targetIdle are validated before any caller can use them to submit a mutation.
    // rule 及 targetIdle 在调用方用于提交变更前完成校验。
    const rule = record(item, "prewarm policy rule");
    const pluginId = text(rule.pluginId, "pluginId");
    const toolId = text(rule.toolId, "toolId");
    const targetIdle = capacityCount(rule.targetIdle, "targetIdle");
    if (targetIdle === 0) throw new Error("targetIdle: expected positive count");
    if (toolIds.has(toolId)) throw new Error("prewarm policy rules: duplicate toolId");
    toolIds.add(toolId);
    return { pluginId, toolId, targetIdle };
  });
}

// request decodes original transaction identity and explicit rules; returns no inferred reset or predecessor.
// request 解码原事务身份及明确规则；不返回推断的重置或前驱。
function request(value: unknown): PluginPrewarmPolicyUpdate {
  // update owns every field required to recover a lost publication acknowledgement.
  // update 拥有恢复丢失发布确认所需的全部字段。
  const update = record(value, "prewarm policy request");
  return { updateId: text(update.updateId, "updateId"), runtimeId: text(update.runtimeId, "runtimeId"),
    expectedRevision: text(update.expectedRevision, "expectedRevision"), rules: rules(update.rules) };
}

// samePluginPrewarmPolicyRequest compares exact original intent, including rule order; returns identity equality.
// samePluginPrewarmPolicyRequest 比较精确原意图，包括规则顺序；返回身份一致性。
export function samePluginPrewarmPolicyRequest(a: PluginPrewarmPolicyUpdate, b: PluginPrewarmPolicyUpdate): boolean {
  return a.updateId === b.updateId && a.runtimeId === b.runtimeId && a.expectedRevision === b.expectedRevision
    && a.rules.length === b.rules.length && a.rules.every((rule, index) => {
      // counterpart exists because both collection lengths match.
      // counterpart 存在，因为两个集合长度一致。
      const counterpart = b.rules[index];
      return rule.pluginId === counterpart.pluginId && rule.toolId === counterpart.toolId && rule.targetIdle === counterpart.targetIdle;
    });
}

// parsePluginPrewarmPolicyReceipt validates one exact publication receipt and returns its host-owned checkpoint.
// parsePluginPrewarmPolicyReceipt 校验单个精确发布回执，返回宿主拥有的检查点。
export function parsePluginPrewarmPolicyReceipt(value: unknown): PluginPrewarmPolicyReceipt {
  // receipt is metadata evidence and grants no permission to initialize a plugin.
  // receipt 为元数据证据，不授予初始化插件的权限。
  const receipt = record(value, "prewarm policy receipt");
  const phases: readonly PluginPrewarmPolicyPhase[] = ["prepared", "publishing", "committed", "cancelled", "interrupted"];
  const phase = phases.find((candidate) => candidate === receipt.phase);
  if (!phase) throw new Error("prewarm policy phase: unknown checkpoint");
  return { request: request(receipt.request), phase, resultRevision: text(receipt.resultRevision, "resultRevision"),
    error: optionalDiagnostic(receipt.error, "error"),
    ...(receipt.persistenceError === undefined ? {} : { persistenceError: text(receipt.persistenceError, "persistenceError") }) };
}

// parsePluginPrewarmPolicyStatus decodes current policy while retaining historical owners without rebinding.
// parsePluginPrewarmPolicyStatus 解码当前策略，保留历史归属且不重新绑定。
export function parsePluginPrewarmPolicyStatus(value: unknown): PluginPrewarmPolicyStatus {
  // status preserves null receipt semantics and the host's cross-transaction exclusion flag.
  // status 保留空回执语义及宿主跨事务排除标志。
  const status = record(value, "prewarm policy status");
  const update = status.update === null ? null : parsePluginPrewarmPolicyReceipt(status.update);
  const capacityUpdateRetained = flag(status.capacityUpdateRetained, "capacityUpdateRetained");
  if (update !== null && capacityUpdateRetained) throw new Error("prewarm policy status: conflicting retained transactions");
  return { runtimeId: text(status.runtimeId, "runtimeId"), revision: text(status.revision, "revision"),
    rules: rules(status.rules), update, capacityUpdateRetained };
}
