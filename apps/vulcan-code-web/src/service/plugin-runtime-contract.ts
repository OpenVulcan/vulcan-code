import {
  PLUGIN_CAPACITY_PURPOSES,
  type PluginCapacityCatalog, type PluginCapacityConfig, type PluginCapacityPurpose,
  type PluginCapacityReceipt, type PluginCapacityRegistration, type PluginCapacityStatus,
  type PluginCapacityUpdate, type PluginRuntimeView, type PluginVmUsage,
} from "@/types/plugin-runtime";

// record validates an object at path and returns its fields without inventing aliases.
// record 校验 path 处的对象，返回原字段，不猜测别名。
export function record(value: unknown, path: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${path}: expected object`);
  return value as Record<string, unknown>;
}

// exact rejects fields a full replacement would otherwise silently drop; returns nothing.
// exact 拒绝完整替换时可能被静默丢弃的字段；无返回值。
function exact(value: Record<string, unknown>, keys: string[], path: string): void {
  if (Object.keys(value).some((key) => !keys.includes(key))) throw new Error(`${path}: unknown configuration field`);
}

// text validates a required nonempty identity at path and returns the original opaque string.
// text 校验 path 处的必填非空身份，返回原不透明字符串。
export function text(value: unknown, path: string): string {
  if (typeof value !== "string" || !value.length || value.includes("\0")) throw new Error(`${path}: expected identity`);
  return value;
}

// optionalDiagnostic validates required nullable diagnostics at path and returns them unchanged.
// optionalDiagnostic 校验 path 处必填可空诊断，并原样返回。
export function optionalDiagnostic(value: unknown, path: string): string | null {
  if (value === null || typeof value === "string") return value;
  throw new Error(`${path}: expected string or null`);
}

// capacityCount rejects unsafe JSON numbers at path before any rounded value can enter an editable draft.
// capacityCount 在任何舍入值进入可编辑草稿前拒绝 path 处不安全的 JSON 数字。
export function capacityCount(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) throw new Error(`${path}: expected nonnegative safe integer`);
  return value;
}

// flag validates a required boolean at path and returns its exact value.
// flag 校验 path 处必填布尔值并返回精确值。
export function flag(value: unknown, path: string): boolean {
  if (typeof value !== "boolean") throw new Error(`${path}: expected boolean`);
  return value;
}

// choice validates a declared wire variant at path and returns its narrowed type.
// choice 校验 path 处声明的线协议变体并返回收窄类型。
function choice<T extends string>(value: unknown, choices: readonly T[], path: string): T {
  if (typeof value !== "string" || !choices.includes(value as T)) throw new Error(`${path}: unknown variant`);
  return value as T;
}

// purpose validates one host route and returns its exact serialized variant.
// purpose 校验单个宿主路由并返回精确序列化变体。
function purpose(value: unknown): PluginCapacityPurpose {
  return choice(value, PLUGIN_CAPACITY_PURPOSES, "purpose");
}

// parseCapacityConfig validates a complete native configuration and returns a lossless editable projection.
// parseCapacityConfig 校验完整原生配置，返回无损可编辑投影。
export function parseCapacityConfig(value: unknown): PluginCapacityConfig {
  // config and resources retain native field spelling; unknown fields require an explicit consumer update.
  // config 和 resources 保留原生字段拼写；未知字段要求明确更新消费端。
  const config = record(value, "config");
  const resources = record(config.resources, "config.resources");
  exact(config, ["resources", "max_queued_calls", "max_queued_bytes"], "config");
  exact(resources, ["kind", "min_resident_vms", "max_resident_vms", "max_running_calls"], "config.resources");
  const result: PluginCapacityConfig = {
    resources: {
      kind: choice(resources.kind, ["shared", "dedicated"], "resources.kind"),
      min_resident_vms: capacityCount(resources.min_resident_vms, "min_resident_vms"),
      max_resident_vms: capacityCount(resources.max_resident_vms, "max_resident_vms"),
      max_running_calls: capacityCount(resources.max_running_calls, "max_running_calls"),
    },
    max_queued_calls: capacityCount(config.max_queued_calls, "max_queued_calls"),
    max_queued_bytes: capacityCount(config.max_queued_bytes, "max_queued_bytes"),
  };
  if (result.resources.max_resident_vms === 0 || result.resources.max_running_calls === 0
    || result.resources.max_running_calls > result.resources.max_resident_vms
    || result.resources.min_resident_vms > result.resources.max_resident_vms
    || (result.resources.kind === "shared" && result.resources.min_resident_vms !== 0)) {
    throw new Error("config: invalid VM reservation or execution bounds");
  }
  return result;
}

// registration validates a retained registration intent and returns exact original fields.
// registration 校验保留登记意图并返回精确原字段。
function registration(value: unknown): PluginCapacityRegistration {
  // request owns the registration token and mapping predecessor.
  // request 持有登记令牌及映射前驱。
  const request = record(value, "registration");
  return {
    registrationId: text(request.registrationId, "registrationId"),
    runtimeId: text(request.runtimeId, "runtimeId"),
    expectedRegistryRevision: text(request.expectedRegistryRevision, "expectedRegistryRevision"),
    pluginId: text(request.pluginId, "pluginId"), purpose: purpose(request.purpose),
  };
}

// update validates a full policy intent, including required nullable config, and returns its projection.
// update 校验完整策略意图及必填可空 config，返回其投影。
function update(value: unknown): PluginCapacityUpdate {
  // request contains one immutable predecessor and one explicit desired configuration.
  // request 包含单个不可变前驱及一个明确目标配置。
  const request = record(value, "request");
  return {
    updateId: text(request.updateId, "updateId"), runtimeId: text(request.runtimeId, "runtimeId"),
    capacityId: text(request.capacityId, "capacityId"), expectedRevision: text(request.expectedRevision, "expectedRevision"),
    pluginId: text(request.pluginId, "pluginId"), purpose: purpose(request.purpose),
    config: request.config === null ? null : parseCapacityConfig(request.config),
  };
}

// parseCapacityReceipt validates one retained transaction and returns diagnostics without predicting its outcome.
// parseCapacityReceipt 校验单个保留事务并返回诊断，不预测结果。
export function parseCapacityReceipt(value: unknown): PluginCapacityReceipt {
  // receipt remains valid even when its parent belongs to a previous host generation.
  // 即使父级属于旧宿主代际，receipt 仍是有效保留证据。
  const receipt = record(value, "receipt");
  return {
    request: update(receipt.request),
    phase: choice(receipt.phase, ["prepared", "applied", "committed", "rejected", "interrupted"], "phase"),
    appliedRevision: receipt.appliedRevision === null ? null : text(receipt.appliedRevision, "appliedRevision"),
    error: optionalDiagnostic(receipt.error, "error"),
    ...(receipt.persistenceError === undefined ? {} : { persistenceError: text(receipt.persistenceError, "persistenceError") }),
  };
}

// usage validates physical counters and returns the independently sampled native values.
// usage 校验物理计数并返回独立采样的原生值。
export function usage(value: unknown): PluginVmUsage {
  // resources is observation only; these counts never become policy defaults.
  // resources 仅为观测；这些计数绝不成为策略默认值。
  const resources = record(value, "resources");
  return {
    resident: capacityCount(resources.resident, "resident"), creating: capacityCount(resources.creating, "creating"),
    idle: capacityCount(resources.idle, "idle"), running: capacityCount(resources.running, "running"),
    retiring: capacityCount(resources.retiring, "retiring"),
  };
}

// parseCapacityStatus validates one retained capacity and returns a safe browser projection.
// parseCapacityStatus 校验单个保留容量并返回安全浏览器投影。
export function parseCapacityStatus(value: unknown): PluginCapacityStatus {
  // status and snapshot preserve their separate host and core naming conventions.
  // status 和 snapshot 保留各自宿主及核心命名约定。
  const status = record(value, "capacity");
  const snapshot = record(status.snapshot, "snapshot");
  const result: PluginCapacityStatus = {
    managementRequest: status.managementRequest === null ? null : registration(status.managementRequest),
    runtimeId: text(status.runtimeId, "runtimeId"), policyRevision: text(status.policyRevision, "policyRevision"),
    pendingConvergence: flag(status.pendingConvergence, "pendingConvergence"),
    purpose: purpose(status.purpose),
    budgetSource: choice(status.budgetSource, ["plugin_budget", "host_override"], "budgetSource"),
    preparingMembers: capacityCount(status.preparingMembers, "preparingMembers"),
    retirementRequested: flag(status.retirementRequested, "retirementRequested"),
    snapshot: {
      capacity_id: text(snapshot.capacity_id, "capacity_id"), plugin_id: text(snapshot.plugin_id, "plugin_id"),
      config: parseCapacityConfig(snapshot.config), resources: usage(snapshot.resources),
      committed_resident_vms: capacityCount(snapshot.committed_resident_vms, "committed_resident_vms"),
      active_operations: capacityCount(snapshot.active_operations, "active_operations"),
      queued_calls: capacityCount(snapshot.queued_calls, "queued_calls"),
      queued_bytes: capacityCount(snapshot.queued_bytes, "queued_bytes"),
      retained_pools: capacityCount(snapshot.retained_pools, "retained_pools"), closing: flag(snapshot.closing, "closing"),
    },
  };
  if (result.managementRequest && (result.managementRequest.runtimeId !== result.runtimeId
    || result.managementRequest.pluginId !== result.snapshot.plugin_id || result.managementRequest.purpose !== result.purpose)) {
    throw new Error("capacity: registration owner mismatch");
  }
  return result;
}

// parseCapacityCatalog validates retained mappings and returns null only for an explicitly absent parent.
// parseCapacityCatalog 校验保留映射，仅在父级明确不存在时返回空值。
export function parseCapacityCatalog(value: unknown): PluginCapacityCatalog | null {
  if (value === null) return null;
  // catalog is a coherent mapping; counters within its rows are not a simultaneous global sample.
  // catalog 是一致映射；各行计数并非同时采集的全局样本。
  const catalog = record(value, "catalog");
  const runtimeId = text(catalog.runtimeId, "runtimeId");
  if (!Array.isArray(catalog.capacities)) throw new Error("capacities: expected array");
  const capacities = catalog.capacities.map(parseCapacityStatus);
  const owners = new Set<string>();
  const identities = new Set<string>();
  for (const capacity of capacities) {
    // ownerKey cannot collide because validated strings cannot contain a zero byte.
    // 经校验的字符串不含零字节，因此 ownerKey 不会碰撞。
    const ownerKey = capacity.snapshot.plugin_id + "\0" + capacity.purpose;
    if (capacity.runtimeId !== runtimeId || owners.has(ownerKey) || identities.has(capacity.snapshot.capacity_id)) {
      throw new Error("catalog: duplicate or mismatched capacity owner");
    }
    owners.add(ownerKey); identities.add(capacity.snapshot.capacity_id);
  }
  return { runtimeId, registryRevision: text(catalog.registryRevision, "registryRevision"), capacities };
}

// parsePluginRuntime validates the read-only parent projection; settings remain exclusively host-owned.
// parsePluginRuntime 校验只读父级投影；设置仍由宿主独占持有。
export function parsePluginRuntime(value: unknown): PluginRuntimeView | null {
  if (value === null) return null;
  // runtime contains both present resources and a possibly historical durable receipt.
  // runtime 包含当前资源及可能属于历史代际的持久回执。
  const runtime = record(value, "runtime");
  return {
    runtimeId: text(runtime.runtimeId, "runtimeId"), resources: usage(runtime.resources),
    policyUpdate: runtime.policyUpdate === null ? null : parseCapacityReceipt(runtime.policyUpdate),
  };
}
