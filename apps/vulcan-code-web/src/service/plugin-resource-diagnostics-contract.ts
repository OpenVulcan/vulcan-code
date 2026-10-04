import { capacityCount, flag, record, text } from "./plugin-runtime-contract";
import type { PluginOperationPhase, PluginResourceDiagnostic, PluginResourceDiagnostics, PluginResourceError, PluginResourceErrorCode, PluginResourceKind, PluginResourceOperation, PluginUninstallDrainDiagnostics, PluginUpdateDrainDiagnostics } from "@/types/plugin-resource-diagnostics";

// fields validates the exact metadata boundary; value and keys are declared DTO fields, returning their object.
// fields 校验精确元数据边界；value 与 keys 为声明 DTO 字段，返回其对象。
function fields(value: unknown, keys: string[]): Record<string, unknown> {
  // result retains only the declared wire object and rejects unexpected payloads.
  // result 保留声明线协议对象并拒绝意外载荷。
  const result = record(value, "diagnostics");
  if (Object.keys(result).some((key) => !keys.includes(key))) throw new Error("diagnostics: unexpected metadata field");
  return result;
}
// variant validates value against the actual native enum choices and returns the narrowed wire value.
// variant 根据实际原生枚举 choices 校验 value，返回收窄线协议值。
function variant<T extends string>(value: unknown, choices: readonly T[]): T {
  if (typeof value !== "string" || !choices.includes(value as T)) throw new Error("diagnostics: unknown enum variant");
  return value as T;
}
// nullableText requires explicit null or an original nonempty identity, returning the exact value.
// nullableText 要求明确空值或原非空身份，返回精确值。
function nullableText(value: unknown): string | null { return value === null ? null : text(value, "identity"); }
// generation validates the host u64 decimal string without converting it to a rounded number.
// generation 校验宿主 u64 十进制字符串，不将其转换为舍入数字。
function generation(value: unknown): string {
  // result is validated before any display or identity comparison.
  // result 在展示或身份比较前校验。
  const result = text(value, "pluginGeneration");
  if (!/^(0|[1-9][0-9]*)$/.test(result)) throw new Error("pluginGeneration: expected decimal string");
  return result;
}
// list requires an array at value and parses each item through the supplied authoritative parser.
// list 要求 value 为数组，并通过提供的权威解析器解析每个元素。
function list<T>(value: unknown, parse: (item: unknown) => T): T[] {
  if (!Array.isArray(value)) throw new Error("diagnostics: expected array");
  return value.map(parse);
}
// error parses source stage and nullable native code, returning no raw message or business data.
// error 解析来源阶段及可空原生错误码，不返回原始文案或业务数据。
function error(value: unknown): PluginResourceError {
  // source has only the two Rust metadata fields.
  // source 仅包含两个 Rust 元数据字段。
  const source = fields(value, ["stage", "code"]);
  return { stage: text(source.stage, "stage"), code: source.code === null ? null : variant<PluginResourceErrorCode>(source.code,
    ["invalid_argument", "not_found", "stale_generation", "capacity_exceeded", "busy", "already_completed", "closed", "cancelled", "deadline_exceeded", "permission_denied", "unsupported", "execution_failed", "cleanup_failed", "internal"]) };
}
// operation parses original identity and phase from value, returning safe independent-stage errors.
// operation 从 value 解析原身份及阶段，返回安全独立阶段错误。
function operation(value: unknown): PluginResourceOperation {
  // source mirrors VulcanPluginResourceOperationDiagnostic exactly.
  // source 精确对应 VulcanPluginResourceOperationDiagnostic。
  const source = fields(value, ["operationId", "phase", "cancellationRequested", "errors"]);
  return { operationId: text(source.operationId, "operationId"),
    phase: variant<PluginOperationPhase>(source.phase, ["queued", "initializing", "running", "waiting_for_host", "cleaning", "succeeded", "failed", "cancelled"]),
    cancellationRequested: flag(source.cancellationRequested, "cancellationRequested"), errors: list(source.errors, error) };
}
// resource parses one retained host row and returns distinct host/core identities and lifecycle facts.
// resource 解析单个保留宿主行，返回区分的宿主与核心身份及生命周期事实。
function resource(value: unknown): PluginResourceDiagnostic {
  // source follows the public Rust DTO without alternate spellings or inferred identities.
  // source 遵循公开 Rust DTO，没有替代拼写或推断身份。
  const source = fields(value, ["resourceKind", "resourceId", "pluginGeneration", "ownerSessionId", "originalPoolId", "coreSessionId", "corePhase", "activeOperationId", "finalizationOperationId", "preparing", "closing", "retired", "isDrainBlocking", "cleanupRecovery", "queuedCalls", "borrowers", "pendingFinalizations", "finalizationOperationIds", "operations", "errors"]);
  return {
    resourceKind: variant<PluginResourceKind>(source.resourceKind, ["pinned_session", "formal_call", "reusable_domain", "hook_runtime", "ui_action_runtime"]),
    resourceId: text(source.resourceId, "resourceId"), pluginGeneration: generation(source.pluginGeneration),
    ownerSessionId: nullableText(source.ownerSessionId), originalPoolId: nullableText(source.originalPoolId), coreSessionId: nullableText(source.coreSessionId),
    corePhase: source.corePhase === null ? null : variant(source.corePhase, ["opening", "ready", "running", "closing", "closed"] as const),
    activeOperationId: nullableText(source.activeOperationId), finalizationOperationId: nullableText(source.finalizationOperationId),
    preparing: flag(source.preparing, "preparing"), closing: flag(source.closing, "closing"), retired: flag(source.retired, "retired"),
    isDrainBlocking: flag(source.isDrainBlocking, "isDrainBlocking"), cleanupRecovery: flag(source.cleanupRecovery, "cleanupRecovery"),
    queuedCalls: source.queuedCalls === null ? null : capacityCount(source.queuedCalls, "queuedCalls"),
    borrowers: source.borrowers === null ? null : capacityCount(source.borrowers, "borrowers"),
    pendingFinalizations: flag(source.pendingFinalizations, "pendingFinalizations"),
    finalizationOperationIds: list(source.finalizationOperationIds, (item) => text(item, "finalizationOperationId")),
    operations: list(source.operations, operation), errors: list(source.errors, error),
  };
}
// parsePluginResourceDiagnostics validates value and returns an explicit absence or the safe public report.
// parsePluginResourceDiagnostics 校验 value，返回明确缺失或安全公开报告。
export function parsePluginResourceDiagnostics(value: unknown): PluginResourceDiagnostics | null {
  if (value === null) return null;
  // source preserves individually synchronized observations rather than pretending atomicity.
  // source 保留独立同步观测，不伪装原子性。
  const source = fields(value, ["runtimeId", "serviceEpoch", "pluginId", "pluginGeneration", "nonAtomicSnapshot", "pinnedSessionCount", "resources"]);
  if (source.nonAtomicSnapshot !== true) throw new Error("diagnostics: expected non-atomic snapshot");
  return { runtimeId: text(source.runtimeId, "runtimeId"), serviceEpoch: text(source.serviceEpoch, "serviceEpoch"), pluginId: text(source.pluginId, "pluginId"),
    pluginGeneration: source.pluginGeneration === null ? null : generation(source.pluginGeneration), nonAtomicSnapshot: true,
    pinnedSessionCount: capacityCount(source.pinnedSessionCount, "pinnedSessionCount"), resources: list(source.resources, resource) };
}
// verifyDrain compares resources with the wrapper's original owner; returns nothing on success.
// verifyDrain 比较 resources 与包装的原所有者；成功时无返回值。
// Actual plugin drain spans retained generations, so catalog transition generations do not filter resource rows.
// 实际插件排空跨保留代次，因此目录转换代次不筛选资源行。
function verifyDrain(resources: PluginResourceDiagnostics | null, pluginId: string, serviceEpoch: string): void {
  if (resources !== null && (resources.pluginId !== pluginId || resources.serviceEpoch !== serviceEpoch)) {
    throw new Error("diagnostics: original drain identity mismatch");
  }
}
// parsePluginUpdateDrainDiagnostics validates the exact original request, returning null only for unavailable live handles.
// parsePluginUpdateDrainDiagnostics 校验精确原请求，仅在实时句柄不可用时返回空值。
export function parsePluginUpdateDrainDiagnostics(value: unknown): PluginUpdateDrainDiagnostics | null {
  if (value === null) return null;
  // source is the retained predecessor wrapper, never the current same-name plugin.
  // source 是保留前驱包装，绝不是当前同名插件。
  const source = fields(value, ["requestId", "pluginId", "serviceEpoch", "predecessorGeneration", "drainingGeneration", "resources"]);
  // result retains original route generations alongside nullable live observations.
  // result 在可空实时观测旁保留原路由代次。
  const result = { requestId: text(source.requestId, "requestId"), pluginId: text(source.pluginId, "pluginId"), serviceEpoch: text(source.serviceEpoch, "serviceEpoch"),
    predecessorGeneration: generation(source.predecessorGeneration), drainingGeneration: generation(source.drainingGeneration), resources: parsePluginResourceDiagnostics(source.resources) };
  verifyDrain(result.resources, result.pluginId, result.serviceEpoch);
  if (result.resources !== null && (result.resources.pluginGeneration !== null || result.resources.resources.some((row) => !row.isDrainBlocking))) {
    throw new Error("diagnostics: invalid predecessor drain selection");
  }
  return result;
}
// parsePluginUninstallDrainDiagnostics validates the original reservation and returns its safe resource observation.
// parsePluginUninstallDrainDiagnostics 校验原预留，返回其安全资源观测。
export function parsePluginUninstallDrainDiagnostics(value: unknown): PluginUninstallDrainDiagnostics {
  // source follows the Kernel wrapper; missing original owners must fail rather than return an empty list.
  // source 遵循 Kernel 包装；缺失原所有者必须失败，不能返回空列表。
  const source = fields(value, ["requestId", "pluginId", "serviceEpoch", "pluginGeneration", "resources"]);
  // result keeps original reservation generation independent from the current catalog.
  // result 独立于当前目录保留原预留代次。
  const result = { requestId: text(source.requestId, "requestId"), pluginId: text(source.pluginId, "pluginId"), serviceEpoch: text(source.serviceEpoch, "serviceEpoch"),
    pluginGeneration: generation(source.pluginGeneration), resources: parsePluginResourceDiagnostics(source.resources) };
  verifyDrain(result.resources, result.pluginId, result.serviceEpoch);
  if (result.resources !== null && (result.resources.pluginGeneration !== null || result.resources.resources.some((row) => !row.isDrainBlocking))) {
    throw new Error("diagnostics: invalid uninstall drain selection");
  }
  return result;
}
