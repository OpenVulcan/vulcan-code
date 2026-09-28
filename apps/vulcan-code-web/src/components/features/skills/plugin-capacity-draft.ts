import { capacityCount, parseCapacityConfig } from "@/service/plugin-runtime-contract";
import type { PluginCapacityConfig, PluginCapacityPredecessor, PluginCapacityStatus } from "@/types/plugin-runtime";

// PluginCapacityDraft preserves original identity separately from human-editable numeric strings.
// PluginCapacityDraft 将原身份与可编辑数字字符串分开保留。
export interface PluginCapacityDraft {
  // predecessor remains immutable until the user explicitly discards and recreates the draft.
  // predecessor 在用户明确丢弃并重新创建草稿之前保持不变。
  predecessor: PluginCapacityPredecessor;
  // kind selects the native shared or dedicated reservation policy.
  // kind 选择原生公共或专用预留策略。
  kind: PluginCapacityConfig["resources"]["kind"];
  // numbers preserves unsubmitted input without rounding or browser number-input normalization.
  // numbers 保留未提交输入，不舍入或使用浏览器数字输入归一化。
  numbers: Record<"min_resident_vms" | "max_resident_vms" | "max_running_calls" | "max_queued_calls" | "max_queued_bytes", string>;
}

// capacityPredecessor extracts an exact observed owner and policy token from status.
// capacityPredecessor 从 status 提取精确观测所有者及策略令牌。
export function capacityPredecessor(status: PluginCapacityStatus): PluginCapacityPredecessor {
  return {
    runtimeId: status.runtimeId, capacityId: status.snapshot.capacity_id, expectedRevision: status.policyRevision,
    pluginId: status.snapshot.plugin_id, purpose: status.purpose,
  };
}

// createCapacityDraft copies the observed full config; it never supplies host budget defaults.
// createCapacityDraft 复制观测完整配置；绝不提供宿主预算默认值。
export function createCapacityDraft(status: PluginCapacityStatus): PluginCapacityDraft {
  // config is validated by the service contract before it becomes an editable form.
  // config 在成为可编辑表单前由服务契约校验。
  const config = status.snapshot.config;
  return { predecessor: capacityPredecessor(status), kind: config.resources.kind, numbers: {
    min_resident_vms: String(config.resources.min_resident_vms), max_resident_vms: String(config.resources.max_resident_vms),
    max_running_calls: String(config.resources.max_running_calls), max_queued_calls: String(config.max_queued_calls),
    max_queued_bytes: String(config.max_queued_bytes),
  } };
}

// draftStillCurrent compares every original identity field with status and never rebases a draft.
// draftStillCurrent 将全部原身份字段与 status 比较，绝不重置草稿基准。
export function draftStillCurrent(draft: PluginCapacityDraft, status: PluginCapacityStatus | undefined): boolean {
  if (!status) return false;
  // original is the user's observed authorization, not the latest selectable row.
  // original 为用户观测授权，而非最新可选行。
  const original = draft.predecessor;
  return original.runtimeId === status.runtimeId && original.capacityId === status.snapshot.capacity_id
    && original.expectedRevision === status.policyRevision && original.pluginId === status.snapshot.plugin_id
    && original.purpose === status.purpose && !status.snapshot.closing && !status.retirementRequested;
}

// readCapacityDraft validates decimal input and complete constraints, returning a replacement configuration.
// readCapacityDraft 校验十进制输入及完整约束，返回替换配置。
export function readCapacityDraft(draft: PluginCapacityDraft): PluginCapacityConfig {
  // read rejects blank, exponential and imprecise numbers rather than silently changing the user's value.
  // read 拒绝空白、指数及精度不足数字，避免静默改变用户值。
  const read = (key: keyof PluginCapacityDraft["numbers"]): number => {
    if (!/^[0-9]+$/u.test(draft.numbers[key])) throw new Error(`${key}: expected decimal integer`);
    return capacityCount(Number(draft.numbers[key]), key);
  };
  return parseCapacityConfig({
    resources: { kind: draft.kind, min_resident_vms: read("min_resident_vms"), max_resident_vms: read("max_resident_vms"), max_running_calls: read("max_running_calls") },
    max_queued_calls: read("max_queued_calls"), max_queued_bytes: read("max_queued_bytes"),
  });
}
