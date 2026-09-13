import { createServiceClient } from "../../src/service/client";
import type { ServiceEventHandler, ServiceParams, ServiceRequestOptions, ServiceTransport } from "../../src/service/protocol";
import type { VmmMemoryActionResult, KnowledgeProjectBinding } from "../../src/types";
import type { LicenseCatalog, LicenseBody } from "../../src/service/licenses-service";
import { auditMcpInspections, auditMcpCapabilities } from "./mcp-fixtures";
import { auditActivity, knowledgeLibraries, publicAi, scenario, startup, workbench } from "./fixtures";
import { memoryRead } from "./memory-fixtures";
import { auditPluginModule, auditPluginRead } from "./plugin-fixtures";
import { auditGitCatalog, auditGitSnapshot } from "./git-fixtures";
export { shellClient } from "./shell";

// AuditCall records exact admitted/rejected method names without printing request data or credentials.
// AuditCall 记录准确已准入或已拒绝方法名，不打印请求数据或凭据。
export interface AuditCall { method: string; status: "read" | "navigation" | "rejected"; }
// calls is an inspectable bounded history of isolated RPC activity.
// calls 是可检查且有界的隔离 RPC 活动历史。
export const calls: AuditCall[] = [];
// listeners owns in-memory subscriptions without sockets, discovery, timers or reconnect loops.
// listeners 持有内存订阅，不使用套接字、发现、计时器或重连循环。
const listeners = new Map<string, Set<ServiceEventHandler<unknown>>>();
// record publishes a harness-only notification after appending one bounded audit entry.
// record 追加有界验收条目后发布仅属于夹具的通知。
function record(method: string, status: AuditCall["status"]) {
  calls.push({ method, status }); if (calls.length > 300) calls.shift();
  if (typeof window !== "undefined") window.dispatchEvent(new Event("ui-audit:rpc"));
}
// objectField accepts only a declared object-valued RPC parameter and rejects every other shape.
// objectField 仅接纳声明为对象的 RPC 参数，拒绝其他结构。
function objectField(params: ServiceParams | undefined, field: string): Record<string, unknown> {
  // value is a single exact protocol field, not a candidate-path fallback.
  // value 是单个准确协议字段，不是候选路径兜底。
  const value = params?.[field];
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error(`UI_AUDIT_INVALID_PARAMS: ${field}`);
  return value as Record<string, unknown>;
}
// stringField requires one exact string-valued RPC field.
// stringField 要求一个准确的字符串 RPC 字段。
function stringField(params: ServiceParams | undefined, field: string): string {
  const value = params?.[field];
  if (typeof value !== "string") throw new Error(`UI_AUDIT_INVALID_PARAMS: ${field}`);
  return value;
}

// dispatch handles only explicitly implemented read/navigation contracts and never pretends writes succeeded.
// dispatch 仅处理明确实现的读取与导航契约，绝不伪装写入成功。
// Parameters: method and params mirror ServiceTransport. Returns: a typed fixture serialized at the transport boundary.
// 参数：method 与 params 对应 ServiceTransport。返回值：在传输边界序列化的强类型夹具。
function dispatch(method: string, params?: ServiceParams): unknown {
  switch (method) {
    case "startup.snapshot": return startup;
    case "workbench.snapshot": return workbench;
    case "service_public_ai_settings_snapshot": return publicAi;
    case "service_public_ai_session_provider_tool_selections": {
      // Only this fictional session exercises the readable, bounded capability failure surface.
      // 仅此虚构会话验证清晰且有尺寸边界的能力失败界面。
      const conversationId = stringField(params, "conversationId");
      if (conversationId === "audit-capabilities-read-failure") throw new Error("Synthetic capability read failure · " + "bounded layout details ".repeat(24));
      throw new Error(`UI_AUDIT_UNIMPLEMENTED_SESSION: ${conversationId}`);
    }
    case "service_output_style_catalog": return workbench.outputStyleCatalog;
    case "service_load_plugin_ui_module": return auditPluginModule(stringField(params, "pluginId"), stringField(params, "mountId"));
    case "service_invoke_plugin_action": return auditPluginRead(stringField(params, "pluginId"), stringField(params, "actionId"), objectField(params, "arguments"), scenario);
    case "service_load_project_vmm_config": stringField(params, "projectId"); return workbench.vmmMemory.projectDraft;
    case "service_load_project_agent_skill_overrides": stringField(params, "projectId"); return {} satisfies Record<string, string>;
    case "service_get_vce_configuration_status": return auditActivity("audit").configuration;
    case "service_get_vce_project_activity": return auditActivity(stringField(objectField(params, "request"), "projectId"));
    case "service_get_vce_project_overview": {
      const activity = auditActivity(stringField(objectField(params, "request"), "projectId"));
      return { configuration: activity.configuration, settings: activity.settings, status: activity.status };
    }
    case "workspace.git.catalog": return auditGitCatalog(stringField(params, "workspacePath"), scenario);
    case "workspace.diff.snapshot": {
      const request = objectField(params, "request");
      return auditGitSnapshot(stringField(params, "workspacePath"), stringField(request, "repositoryKey"), stringField(request, "mode"));
    }
    case "service_list_knowledge_libraries": {
      // This named fictional project exercises only the composer's recoverable read-error surface.
      // 这个具名虚构项目仅验证输入区可恢复的读取错误界面。
      if (params?.projectId === "audit-knowledge-read-failure") throw new Error("Synthetic knowledge read failure · " + "bounded layout details ".repeat(12));
      return knowledgeLibraries;
    }
    case "service_list_project_knowledge_bindings": return [] satisfies KnowledgeProjectBinding[];
    case "service_list_knowledge_project_bindings": return [] satisfies KnowledgeProjectBinding[];
    case "service_list_knowledge_sources":
    case "service_list_knowledge_documents":
    case "service_list_knowledge_library_folders":
    case "service_list_knowledge_custom_words": return [];
    case "service_get_knowledge_import_job": return null;
    case "service_mcp_runtime_snapshot": return auditMcpInspections(scenario);
    case "service_mcp_server_capabilities": return auditMcpCapabilities(stringField(params, "serverId"), scenario);
    case "service_license_catalog": return { schema_version: 2, scope: "rust-desktop-dependencies", source_file: "THIRD_PARTY_NOTICES.md",
      source_sha256: "ui-audit-synthetic", licenses: [{ id: "audit-license", title: "Synthetic audit license", status: "declared", sections: [{ id: "audit-body",
        start_line: 1, end_line: 2, dependencies: [{ name: "synthetic-package", version: "0.0.0", source: "https://audit.invalid" }] }] }] } satisfies LicenseCatalog;
    case "service_license_body": return { section_id: stringField(params, "sectionId"), text: "Synthetic audit license fixture.\nThis is not a production dependency notice." } satisfies LicenseBody;
    case "service_run_vmm_memory_action": {
      const request = objectField(params, "request");
      const action = stringField(request, "action");
      // The memory request protocol explicitly declares payload optional.
      // 记忆请求协议明确声明 payload 可选。
      const payload = request.payload === undefined ? {} : objectField(request, "payload");
      const data = memoryRead(action, payload);
      if (data === undefined) throw new Error(`UI_AUDIT_UNIMPLEMENTED_MEMORY_ACTION: ${action}`);
      return { action, kind: data.kind, request, response: data, summary: "隔离只读夹具", data } satisfies VmmMemoryActionResult;
    }
    case "service_set_expanded_task_projects": {
      // expandedProjectIds is the exact project-actions-service navigation field.
      // expandedProjectIds 是 project-actions-service 的准确导航字段。
      const ids = params?.projectIds;
      if (!Array.isArray(ids) || !ids.every((id): id is string => typeof id === "string")) throw new Error("UI_AUDIT_INVALID_PARAMS: projectIds");
      workbench.expandedProjectIds = ids; return workbench;
    }
    default: throw new Error(`UI_AUDIT_UNIMPLEMENTED_RPC: ${method}`);
  }
}

// AuditTransport is the complete non-network transport injected into production service clients.
// AuditTransport 是注入生产服务客户端的完整无网络传输。
export class AuditTransport implements ServiceTransport {
  // request resolves only explicit fixture contracts, honoring caller cancellation before dispatch.
  // request 仅解析明确夹具契约，并在派发前遵循调用方取消。
  async request<T>(method: string, params?: ServiceParams, options?: ServiceRequestOptions): Promise<T> {
    if (options?.signal?.aborted) throw new DOMException("Aborted", "AbortError");
    try {
      const result = dispatch(method, params);
      record(method, method === "service_set_expanded_task_projects" ? "navigation" : "read");
      return structuredClone(result) as T;
    } catch (error) { record(method, "rejected"); throw error; }
  }
  // subscribe retains only in-memory callbacks and returns an idempotent unsubscribe function.
  // subscribe 仅保留内存回调，并返回幂等退订函数。
  async subscribe<T>(topic: string, handler: ServiceEventHandler<T>): Promise<() => void> {
    const handlers = listeners.get(topic) ?? new Set<ServiceEventHandler<unknown>>();
    const erased = handler as ServiceEventHandler<unknown>;
    handlers.add(erased); listeners.set(topic, handlers);
    return () => { handlers.delete(erased); if (handlers.size === 0) listeners.delete(topic); };
  }
  // dispose clears only this isolated page's memory subscriptions.
  // dispose 仅清除此隔离页面的内存订阅。
  dispose(): void { listeners.clear(); }
}
// serviceClient replaces the production singleton before any settings page can call it directly.
// serviceClient 在任何设置页直接调用前替换生产单例。
export const serviceClient = createServiceClient(new AuditTransport());
// The isolated transport is available, ensuring production fallback mutation simulators are never entered.
// 隔离传输可用，确保不会进入生产回退修改模拟器。
export function hasServiceRuntime(): boolean { return true; }
// Unsupported fixture contracts are terminal and never trigger bootstrap retries.
// 未支持的夹具契约为终态，绝不触发启动重试。
export function serviceBootstrapErrorIsRetryable(): boolean { return false; }
// Reuse the existing replaceable toast identity without loading production discovery code.
// 复用既有可替换提示身份，不加载生产发现代码。
export const BROWSER_SERVICE_CONNECTION_TOAST_ID = "browser-service-connection";
