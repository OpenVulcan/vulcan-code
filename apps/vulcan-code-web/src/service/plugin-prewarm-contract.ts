import { capacityCount, flag, optionalDiagnostic, record, text, usage } from "./plugin-runtime-contract";
import type { PluginPrewarmCatalog, PluginPrewarmOwner, PluginPrewarmReceipt, PluginPrewarmSnapshot, PluginPrewarmTool } from "@/types/plugin-prewarm";

// parsePluginPrewarmSessionId returns the host's canonical UUID or rejects malformed recovery input.
// parsePluginPrewarmSessionId 返回宿主规范 UUID，或拒绝格式错误的恢复输入。
export function parsePluginPrewarmSessionId(value: unknown): string {
  // identity follows lowercase hyphenated UUID serialization without restricting its version.
  // identity 遵循小写连字符 UUID 序列化形式，不限制其版本。
  const identity = text(value, "ownerSessionId");
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(identity)) {
    throw new Error("ownerSessionId: expected canonical UUID");
  }
  return identity;
}

// generation validates the host's canonical u64 wire identity and returns it without numeric rounding.
// generation 校验宿主规范 u64 线协议身份并返回原值，不作数值舍入。
function generation(value: unknown): string {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]*)$/.test(value) || value.length > 20
    || BigInt(value) > (1n << 64n) - 1n) throw new Error("pluginGeneration: expected canonical u64 string");
  return value;
}

// owner validates the canonical parent/controller pair in a response and returns its exact identity.
// owner 校验响应中的规范父级／控制器组合并返回精确身份。
function owner(value: Record<string, unknown>): PluginPrewarmOwner {
  return { runtimeId: text(value.runtimeId, "runtimeId"), ownerSessionId: parsePluginPrewarmSessionId(value.ownerSessionId) };
}

// tool validates the enabled reusable declaration and returns its exact host projection.
// tool 校验已启用复用声明并返回精确宿主投影。
function tool(value: unknown): PluginPrewarmTool {
  // declaration provides its own bound; the browser introduces no pool default.
  // declaration 提供自身上限；浏览器不引入池默认值。
  const declaration = record(value, "tool");
  return { pluginId: text(declaration.pluginId, "pluginId"), toolId: text(declaration.toolId, "toolId"),
    pluginGeneration: generation(declaration.pluginGeneration), maxResidentVms: capacityCount(declaration.maxResidentVms, "maxResidentVms") };
}

// snapshot validates native completion counters while preserving their original snake_case spelling.
// snapshot 校验原生完成计数，同时保留原 snake_case 拼写。
function snapshot(value: unknown): PluginPrewarmSnapshot {
  // observed is historical evidence and is never transformed into policy or live availability.
  // observed 为历史证据，绝不转换为策略或实时可用量。
  const observed = record(value, "satisfied");
  return { pool_id: text(observed.pool_id, "pool_id"), ready: capacityCount(observed.ready, "ready"),
    unavailable: capacityCount(observed.unavailable, "unavailable"), physical: usage(observed.physical),
    max_resident_vms: capacityCount(observed.max_resident_vms, "max_resident_vms"),
    closing: flag(observed.closing, "closing"), admission_blocked: flag(observed.admission_blocked, "admission_blocked") };
}

// parsePluginPrewarmReceipt validates one response; opaque operation objects remain diagnostic-only.
// parsePluginPrewarmReceipt 校验单个响应；不透明操作对象保持仅供诊断。
export function parsePluginPrewarmReceipt(value: unknown): PluginPrewarmReceipt {
  // receipt is the sole host ledger projection, not a browser-inferred lifecycle state.
  // receipt 为唯一宿主台账投影，不是浏览器推断的生命周期状态。
  const receipt = record(value, "prewarm receipt");
  if (!Array.isArray(receipt.completed)) throw new Error("completed: expected array");
  return { ...owner(receipt), invocationId: text(receipt.invocationId, "invocationId"),
    pluginId: text(receipt.pluginId, "pluginId"), toolId: text(receipt.toolId, "toolId"),
    pluginGeneration: generation(receipt.pluginGeneration), targetIdle: capacityCount(receipt.targetIdle, "targetIdle"),
    workspaceRoot: text(receipt.workspaceRoot, "workspaceRoot"), started: flag(receipt.started, "started"),
    retired: flag(receipt.retired, "retired"), requiresCleanupRecovery: flag(receipt.requiresCleanupRecovery, "requiresCleanupRecovery"),
    succeeded: flag(receipt.succeeded, "succeeded"), error: optionalDiagnostic(receipt.error, "error"),
    operation: receipt.operation === null ? null : record(receipt.operation, "operation"),
    completed: receipt.completed.map((operation) => record(operation, "completed operation")),
    satisfied: receipt.satisfied === null ? null : snapshot(receipt.satisfied) };
}

// assertPluginPrewarmOwner rejects a response from another runtime or controller; returns nothing.
// assertPluginPrewarmOwner 拒绝其他运行时或控制器的响应；无返回值。
export function assertPluginPrewarmOwner(actual: PluginPrewarmOwner, expected: PluginPrewarmOwner): void {
  if (actual.runtimeId !== expected.runtimeId || actual.ownerSessionId !== expected.ownerSessionId) {
    throw new Error("prewarm: response owner mismatch");
  }
}

// parsePluginPrewarmCatalog validates one coherent owner and rejects duplicate tools or receipt identities.
// parsePluginPrewarmCatalog 校验一致归属，并拒绝重复工具或回执身份。
export function parsePluginPrewarmCatalog(value: unknown): PluginPrewarmCatalog {
  // catalog, tools and targets preserve the response's declared ownership and ordering.
  // catalog、tools 及 targets 保留响应声明的归属与顺序。
  const catalog = record(value, "prewarm catalog");
  if (!Array.isArray(catalog.tools) || !Array.isArray(catalog.targets)) throw new Error("prewarm catalog: expected arrays");
  const parsed = { ...owner(catalog), tools: catalog.tools.map(tool), targets: catalog.targets.map(parsePluginPrewarmReceipt) };
  const identities = new Set<string>();
  const toolIds = new Set<string>();
  for (const target of parsed.targets) {
    assertPluginPrewarmOwner(target, parsed);
    if (identities.has(target.invocationId)) throw new Error("prewarm catalog: duplicate invocationId");
    identities.add(target.invocationId);
  }
  for (const declaration of parsed.tools) {
    if (toolIds.has(declaration.toolId)) throw new Error("prewarm catalog: duplicate toolId");
    toolIds.add(declaration.toolId);
  }
  return parsed;
}
