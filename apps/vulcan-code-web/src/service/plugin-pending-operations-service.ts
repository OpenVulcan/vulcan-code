import type { LuaSkillsCatalogUpdate } from "@/types";
import type { PluginCapacityReceipt } from "@/types/plugin-runtime";
import type { PluginPrewarmPolicyReceipt } from "@/types/plugin-prewarm-policy";
import { parsePluginPrewarmPolicyReceipt } from "./plugin-prewarm-policy-contract";
import { parseCapacityReceipt } from "./plugin-runtime-contract";
import { parsePluginUninstallStatus, type PluginUninstallStatus } from "./plugin-uninstall-contract";
import { invalidatePersistentPluginUiModules } from "./plugin-ui-module-runtime";
import { parseLocalPluginUpdateStatus, type LocalPluginUpdateStatus } from "./plugin-update-contract";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

// PluginPendingAction is one explicit continuation the service kernel may offer for a retained operation.
// PluginPendingAction 是服务内核可为保留操作提供的单个明确继续方式。
export type PluginPendingAction = "resume" | "abort" | "cancel" | "forget";

// PluginPendingActionOffer is one server-declared continuation; the browser never derives eligibility itself.
// PluginPendingActionOffer 是服务端声明的单个继续方式；浏览器绝不自行推导资格。
export interface PluginPendingActionOffer {
  // action is accepted verbatim by the continuation entry.
  // action 被继续入口原样接受。
  action: PluginPendingAction;
  // requiresAcknowledgement means the continuation must carry acknowledgeExternalEffects: true.
  // requiresAcknowledgement 表示继续请求必须携带 acknowledgeExternalEffects: true。
  requiresAcknowledgement: boolean;
}

// PluginPendingOperation is one retained operation of any kind with its stable identity and offered continuations.
// PluginPendingOperation 是任意类别的单个保留操作，含稳定身份及可用继续方式。
export type PluginPendingOperation = { operationId: string; actions: PluginPendingActionOffer[] } & (
  | { kind: "update"; pluginId: string; detail: LocalPluginUpdateStatus }
  | { kind: "uninstall"; pluginId: string; detail: PluginUninstallStatus }
  | { kind: "capacityPolicy"; pluginId: string; detail: PluginCapacityReceipt }
  | { kind: "prewarmPolicy"; pluginId: null; detail: PluginPrewarmPolicyReceipt }
);

// PluginPendingKind names one family of retained operations.
// PluginPendingKind 命名一类保留操作。
export type PluginPendingKind = PluginPendingOperation["kind"];

// PluginPendingContinuation names exactly one original operation and the continuation it accepts.
// PluginPendingContinuation 精确指名单个原操作及其接受的继续方式。
export type PluginPendingContinuation =
  | { kind: "update"; operationId: string; pluginId: string; action: "resume" | "abort"; acknowledgeExternalEffects?: boolean }
  | { kind: "uninstall"; operationId: string; pluginId: string; action: "resume"; acknowledgeExternalEffects?: boolean }
  | { kind: "capacityPolicy" | "prewarmPolicy"; operationId: string; action: "resume" | "cancel" | "forget" };

// kindActions lists the continuations each kind can ever offer; anything else is a protocol violation.
// kindActions 列出每个类别可能提供的继续方式；其他取值均为协议违规。
const kindActions: Record<PluginPendingKind, ReadonlyArray<PluginPendingAction>> = {
  update: ["resume", "abort"], uninstall: ["resume"], capacityPolicy: ["resume", "cancel", "forget"], prewarmPolicy: ["resume", "cancel", "forget"],
};

// parseOffers validates the declared continuation list of kind; returns sanitized offers in service order.
// parseOffers 校验 kind 声明的继续列表；按服务顺序返回脱敏可用动作。
function parseOffers(value: unknown, kind: PluginPendingKind): PluginPendingActionOffer[] {
  if (!Array.isArray(value)) throw new Error("Invalid plugin pending operation actions.");
  // offers keeps only declared fields so later rendering cannot read undeclared response content.
  // offers 仅保留声明字段，使后续渲染无法读取未声明响应内容。
  const offers = value.map((offer: unknown) => {
    if (typeof offer !== "object" || offer === null || Array.isArray(offer)) throw new Error("Invalid plugin pending operation action.");
    const fields = offer as Record<string, unknown>;
    if (!kindActions[kind].includes(fields.action as PluginPendingAction) || typeof fields.requiresAcknowledgement !== "boolean") {
      throw new Error("Invalid plugin pending operation action.");
    }
    return { action: fields.action as PluginPendingAction, requiresAcknowledgement: fields.requiresAcknowledgement };
  });
  if (new Set(offers.map((offer) => offer.action)).size !== offers.length) throw new Error("Duplicate plugin pending operation action.");
  return offers;
}

// parseOperation validates one listed operation and binds its detail to the outer identity.
// parseOperation 校验单个列出操作，并将其详情绑定至外层身份。
function parseOperation(value: unknown): PluginPendingOperation {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("Invalid plugin pending operation.");
  // fields reads the declared transport names without searching aliases.
  // fields 读取声明传输名称，不搜索别名。
  const fields = value as Record<string, unknown>;
  const { operationId, pluginId } = fields;
  if (typeof operationId !== "string" || !operationId) throw new Error("Invalid plugin pending operation identity.");
  switch (fields.kind) {
    case "update": {
      if (typeof pluginId !== "string") break;
      const detail = parseLocalPluginUpdateStatus(fields.detail, pluginId);
      if (detail.requestId !== operationId) break;
      return { kind: "update", operationId, pluginId, actions: parseOffers(fields.actions, "update"), detail };
    }
    case "uninstall": {
      if (typeof pluginId !== "string") break;
      const detail = parsePluginUninstallStatus(fields.detail, pluginId);
      if (detail.requestId !== operationId) break;
      return { kind: "uninstall", operationId, pluginId, actions: parseOffers(fields.actions, "uninstall"), detail };
    }
    case "capacityPolicy": {
      if (typeof pluginId !== "string") break;
      const detail = parseCapacityReceipt(fields.detail);
      if (detail.request.updateId !== operationId || detail.request.pluginId !== pluginId) break;
      return { kind: "capacityPolicy", operationId, pluginId, actions: parseOffers(fields.actions, "capacityPolicy"), detail };
    }
    case "prewarmPolicy": {
      if (pluginId !== null) break;
      const detail = parsePluginPrewarmPolicyReceipt(fields.detail);
      if (detail.request.updateId !== operationId) break;
      return { kind: "prewarmPolicy", operationId, pluginId, actions: parseOffers(fields.actions, "prewarmPolicy"), detail };
    }
    default:
      throw new Error("Unknown plugin pending operation kind.");
  }
  throw new Error("Plugin pending operation identity does not match its detail.");
}

// parsePluginPendingOperations validates the complete list; one malformed or repeated row rejects the whole observation.
// parsePluginPendingOperations 校验完整列表；任何损坏或重复行都会拒绝整次观测。
// value is untrusted RPC output; returns sanitized operations in service order.
// value 为不可信 RPC 输出；按服务顺序返回脱敏操作。
export function parsePluginPendingOperations(value: unknown): PluginPendingOperation[] {
  if (!Array.isArray(value)) throw new Error("Invalid plugin pending operation list.");
  const operations = value.map(parseOperation);
  if (new Set(operations.map((operation) => `${operation.kind}:${operation.operationId}`)).size !== operations.length) {
    throw new Error("Duplicate plugin pending operation identity.");
  }
  return operations;
}

// requireService rejects preview mode before any transport access; returns nothing.
// requireService 在任何传输访问前拒绝预览模式；无返回值。
function requireService(): void {
  if (!hasServiceRuntime()) throw new Error("Plugin pending operations require an attached Core service.");
}

// readPluginPendingOperations lists every retained plugin operation without starting, creating or replaying work.
// readPluginPendingOperations 列出所有保留插件操作，不启动、创建或重放工作。
export async function readPluginPendingOperations(): Promise<PluginPendingOperation[]> {
  requireService();
  return parsePluginPendingOperations(await serviceClient.request<unknown>("service_plugin_pending_operations", {}));
}

// continuePluginPendingOperation sends one explicit continuation exactly once; transport failure never implies rollback.
// continuePluginPendingOperation 精确发送一次明确继续请求；传输失败绝不表示回滚。
// Returns the republished catalog when the operation produced one; callers re-read the list for the new state.
// 操作产生重新发布目录时返回该目录；调用方重新读取列表获取新状态。
export async function continuePluginPendingOperation(continuation: PluginPendingContinuation): Promise<LuaSkillsCatalogUpdate | null> {
  requireService();
  const response = await serviceClient.request<unknown>("service_continue_plugin_pending_operation", continuation);
  if (typeof response !== "object" || response === null || Array.isArray(response) || !("catalog" in response)) {
    throw new Error("Invalid plugin pending operation continuation response.");
  }
  // catalog is authoritative only once the continuation RPC itself succeeded.
  // catalog 仅在继续 RPC 本身成功后具有权威。
  const catalog = (response as { catalog: unknown }).catalog;
  if (catalog !== null && (typeof catalog !== "object" || Array.isArray(catalog))) {
    throw new Error("Invalid plugin pending operation catalog.");
  }
  // A finished removal retires this plugin's persistent UI modules.
  // 完成的移除会使此插件持久 UI 模块退役。
  if (continuation.kind === "uninstall") invalidatePersistentPluginUiModules(continuation.pluginId);
  return catalog as LuaSkillsCatalogUpdate | null;
}

// pendingOffers returns the continuations offered for the exact receipt operationId of kind in its observed phase.
// pendingOffers 返回 kind 中精确回执 operationId 在已观测阶段下提供的继续方式。
// A missing row or a phase from another moment yields no offers, so a racing read cannot authorize an action.
// 缺失行或来自其他时刻的阶段不提供动作，使竞争读取无法授权操作。
export function pendingOffers(operations: PluginPendingOperation[], kind: "capacityPolicy" | "prewarmPolicy",
  operationId: string, phase: string): PluginPendingActionOffer[] {
  const match = operations.find((operation) => operation.kind === kind && operation.operationId === operationId);
  return match !== undefined && match.detail.phase === phase ? match.actions : [];
}
