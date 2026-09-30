import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { LuaSkillsCatalogUpdate } from "@/types";

// LocalPluginUpdateEffect exposes one durable original operation without callback data.
// LocalPluginUpdateEffect 公开单个持久原操作，不包含回调数据。
export interface LocalPluginUpdateEffect {
  // operation selects the exact candidate effect declared by the service.
  // operation 选择服务声明的精确候选副作用。
  operation: "drain_predecessor_lua" | "disable_predecessor" | "probe" | "enable" | "enable_rollback";
  // phase is historical observation, not evidence that external effects were undone.
  // phase 为历史观测，不证明外部副作用已撤销。
  phase: "started" | "succeeded" | "observation_failed";
}

// LocalPluginUpdateStatus is the sanitized original-request projection owned by the service coordinator.
// LocalPluginUpdateStatus 是服务协调器持有的脱敏原请求投影。
export interface LocalPluginUpdateStatus {
  // requestId is the original canonical UUID; recovery never allocates a replacement.
  // requestId 为原规范 UUID；恢复绝不分配替代标识。
  requestId: string;
  // pluginId is the exact queried namespace.
  // pluginId 为精确查询命名空间。
  pluginId: string;
  // phase records durable selection progress, not live readiness or resource release.
  // phase 记录持久选择进度，不表示实时就绪或资源释放。
  phase: "preparing" | "prepared" | "committing" | "published" | "completed" | "aborted";
  // candidate preserves the generation store's exact snake-case identity contract.
  // candidate 保留代次存储精确下划线命名身份契约。
  candidate: {
    // plugin_id binds the immutable package to its owning namespace.
    // plugin_id 将不可变插件包绑定至所属命名空间。
    plugin_id: string;
    // generation_id identifies the original immutable physical copy.
    // generation_id 标识原不可变物理副本。
    generation_id: string;
    // version comes from the inspected package manifest.
    // version 来自已检查插件包清单。
    version: string;
    // content_fingerprint binds the complete approved package contents.
    // content_fingerprint 绑定完整已批准插件包内容。
    content_fingerprint: string;
  };
  // activationRevision stays decimal text so JavaScript cannot round a native revision.
  // activationRevision 保持十进制文本，使 JavaScript 不会舍入原生修订。
  activationRevision: string;
  // installation is historical observation; null does not prove absence of other external effects.
  // installation 为历史观测；空值不证明其他外部副作用不存在。
  installation: LocalPluginUpdateEffect["phase"] | null;
  // effects retains every attempted candidate operation in execution order.
  // effects 按执行顺序保留每个已尝试候选操作。
  effects: LocalPluginUpdateEffect[];
}

// parseLocalPluginUpdateStatus validates the exact service projection against the explicitly queried owner.
// parseLocalPluginUpdateStatus 按明确查询所有者校验精确服务投影。
// value is untrusted RPC output; returns sanitized known fields or throws without exposing that output.
// value 为不可信 RPC 输出；返回脱敏已知字段，或报错但不暴露该输出。
export function parseLocalPluginUpdateStatus(value: unknown, pluginId: string): LocalPluginUpdateStatus {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("Invalid plugin update status.");
  // record is narrowed field by field before any action identity is accepted.
  // record 在接纳任何操作身份之前逐字段缩小类型。
  const record = value as Record<string, unknown>;
  if (typeof record.requestId !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(record.requestId)
    || record.requestId === "00000000-0000-0000-0000-000000000000" || record.pluginId !== pluginId
    || typeof record.activationRevision !== "string" || !/^[1-9][0-9]*$/.test(record.activationRevision)
    || (record.phase !== "preparing" && record.phase !== "prepared" && record.phase !== "committing" && record.phase !== "published" && record.phase !== "completed" && record.phase !== "aborted")
    || (record.phase === "preparing" && record.installation !== null)
    || (record.installation !== null && record.installation !== "started" && record.installation !== "succeeded" && record.installation !== "observation_failed")
    || typeof record.candidate !== "object" || record.candidate === null || Array.isArray(record.candidate)) {
    throw new Error("Plugin update status has an invalid identity or phase.");
  }
  // candidate uses the generation store's declared field names without aliases or inferred defaults.
  // candidate 使用代次存储声明的字段名称，不使用别名或推断默认值。
  const candidate = record.candidate as Record<string, unknown>;
  if (candidate.plugin_id !== pluginId || typeof candidate.generation_id !== "string" || !candidate.generation_id
    || typeof candidate.version !== "string" || !candidate.version
    || typeof candidate.content_fingerprint !== "string" || !candidate.content_fingerprint) {
    throw new Error("Plugin update status has an invalid immutable target.");
  }
  if (!Array.isArray(record.effects) || (record.phase === "preparing" && record.effects.length !== 0)) {
    throw new Error("Plugin update status has invalid candidate effects.");
  }
  // Validate each declared operation and discard undeclared response fields.
  // 校验每个声明操作，并丢弃未声明响应字段。
  const effects: LocalPluginUpdateEffect[] = record.effects.map((value: unknown) => {
    if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("Invalid candidate effect.");
    // Narrow exact service fields without guessing aliases or missing evidence.
    // 缩小精确服务字段类型，不猜测别名或缺失证据。
    const effect = value as Record<string, unknown>;
    if ((effect.operation !== "drain_predecessor_lua" && effect.operation !== "disable_predecessor" && effect.operation !== "probe" && effect.operation !== "enable" && effect.operation !== "enable_rollback")
      || (effect.phase !== "started" && effect.phase !== "succeeded" && effect.phase !== "observation_failed")) {
      throw new Error("Invalid candidate effect operation or observation.");
    }
    return { operation: effect.operation, phase: effect.phase };
  });
  if (new Set(effects.map((effect) => effect.operation)).size !== effects.length) throw new Error("Duplicate candidate effect.");
  return {
    requestId: record.requestId, pluginId, phase: record.phase,
    candidate: { plugin_id: pluginId, generation_id: candidate.generation_id, version: candidate.version, content_fingerprint: candidate.content_fingerprint },
    activationRevision: record.activationRevision, installation: record.installation, effects,
  };
}

// readLocalPluginUpdates reads every original request for pluginId without creating or recovering work.
// readLocalPluginUpdates 读取 pluginId 的全部原请求，不创建或恢复工作。
// Returns validated records; a failed or mixed-owner response must not authorize UI actions.
// 返回已校验记录；失败或混合所有者响应不得授权界面操作。
export async function readLocalPluginUpdates(pluginId: string): Promise<LocalPluginUpdateStatus[]> {
  if (!hasServiceRuntime()) throw new Error("Plugin update recovery requires an attached Core service.");
  // response belongs to this single explicit namespace, independent from the current public package version.
  // response 属于此单个明确命名空间，独立于当前公开插件包版本。
  const response = await serviceClient.request<unknown>("service_local_plugin_update_requests", { pluginId });
  if (!Array.isArray(response)) throw new Error("Invalid plugin update request list.");
  // Duplicate identities cannot represent two independent original requests or two independently actionable rows.
  // 重复身份不能表示两个独立原请求或两行可独立操作条目。
  const records = response.map((record) => parseLocalPluginUpdateStatus(record, pluginId));
  if (new Set(records.map((record) => record.requestId)).size !== records.length) throw new Error("Duplicate plugin update request identity.");
  return records;
}

// resumeLocalPluginUpdate completes an already-committed original request; no callback or installation is replayed.
// resumeLocalPluginUpdate 完成已提交的原请求；不重放回调或安装。
// Exact pluginId/requestId identify the request; returns the service's revisioned catalog update.
// 精确 pluginId/requestId 标识请求；返回服务带修订的目录更新。
export async function resumeLocalPluginUpdate(pluginId: string, requestId: string): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) throw new Error("Plugin update recovery requires an attached Core service.");
  return serviceClient.request<LuaSkillsCatalogUpdate>("service_resume_local_plugin_update", { pluginId, requestId });
}

// abortLocalPluginUpdate abandons an exact unpublished request after explicit acknowledgement of retained external effects.
// abortLocalPluginUpdate 在明确确认外部副作用仍保留后，放弃精确尚未发布请求。
// Parameters preserve the reviewed request and acknowledgement; returns validated durable status, not a rollback claim.
// 参数保留已审阅请求及确认；返回已校验持久状态，不声称回滚。
export async function abortLocalPluginUpdate(pluginId: string, requestId: string, acknowledgeExternalEffects: boolean): Promise<LocalPluginUpdateStatus> {
  if (!hasServiceRuntime()) throw new Error("Plugin update recovery requires an attached Core service.");
  // The acknowledgement authorizes this original request only and is never inferred from phase.
  // 确认仅授权此原请求，绝不从阶段推断。
  const response = parseLocalPluginUpdateStatus(await serviceClient.request<unknown>("service_abort_local_plugin_update", {
    pluginId, requestId, acknowledgeExternalEffects,
  }), pluginId);
  if (response.requestId !== requestId) throw new Error("Plugin update acknowledgement belongs to another request.");
  if (response.phase !== "aborted") throw new Error("Plugin update abandonment was not confirmed.");
  return response;
}
