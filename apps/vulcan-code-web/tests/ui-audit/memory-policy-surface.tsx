import React from "react";
import { I18nProvider } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { VmmNativeHostProjectPolicy } from "../../src/components/features/memory/vmm-native-host-project-policy";
import { VmmNativeProjectRetention } from "../../src/components/features/memory/vmm-native-project-retention";
import { VmmNativeRuntimePolicy } from "../../src/components/features/memory/vmm-native-runtime-policy";
import { VmmNativeProjectProfilePolicy } from "../../src/components/features/memory/vmm-native-project-profile-policy";
import { VmmRulesPage } from "../../src/components/features/memory/vmm-rules-page";
import { VmmAdvancedPage } from "../../src/components/features/memory/vmm-advanced-page";
import { browserFallbackSnapshot } from "../../src/service/browser-fallback-workbench";
import { policy } from "./memory-fixtures";
import type { VmmMemoryActionResult, VmmNativeHostProjectPolicyOverrides, VmmNativeProjectRetentionRules, VmmNativeRuntimePolicySummary, VmmNativeProfileLifetimes, VmmConfigDraftSummary } from "../../src/types";
// The real project settings owner loads this stylesheet with the host-policy editor.
// 真实项目设置归属在加载宿主策略编辑器时加载此样式表。
import "../../src/styles/project-vmm.css";

/** Renders exact host and memory-project policy editors without real persistence.
 * 渲染准确宿主和记忆项目策略编辑器，不进行真实持久化。
 * No parameters; returns an isolated localized policy surface and action log.
 * 无参数；返回隔离本地化策略界面及动作日志。
 */
export function AuditMemoryPolicySurface() {
  // query selects the declared global, host, or memory-project policy owner.
  // query 选择已声明的全局、宿主或记忆项目策略归属。
  const query = new URLSearchParams(window.location.search);
  // isHost chooses the caller override editor rather than memory retention.
  // isHost 选择调用方覆盖编辑器而非记忆保留策略。
  const isHost = query.get("entry") === "host";
  // isGlobal exposes the already-declared five global groups in the same isolated transport.
  // isGlobal 在同一隔离传输中展示已声明的五个全局配置组。
  const isGlobal = query.get("entry") === "global";
  // snapshot and draft supply declared native/external forms without real settings access.
  // snapshot 与 draft 提供已声明原生/外链表单，不访问真实设置。
  const snapshot = React.useMemo(() => browserFallbackSnapshot().vmmMemory, []);
  const [draft, setDraft] = React.useState<VmmConfigDraftSummary>({ ...snapshot.globalDraft, backendMode: query.get("backend") === "external" ? "external_grpc" : "native", nativeProjectBindingMode: "fixed" });
  const [scope, setScope] = React.useState<"global" | "project">("global");
  const [scopeProject, setScopeProject] = React.useState("");
  // profileOverrides separates each synthetic project's accepted profile lifetime policy.
  // profileOverrides 分开持有每个合成项目接纳的画像期限策略。
  const profileOverrides = React.useRef(new Map<number, VmmNativeProfileLifetimes>());
  // project fixes a keyed owner and permits explicit identity-change testing.
  // project 固定按键归属，并允许明确身份切换测试。
  const [project, setProject] = React.useState(1);
  // rejected and busy model the declared failure and exclusive action states.
  // rejected 与 busy 模拟已声明失败及独占动作状态。
  const [rejected, setRejected] = React.useState(query.get("result") === "failure");
  const [busy, setBusy] = React.useState(false);
  // log records only exact synthetic action payloads.
  // log 仅记录准确的合成动作负载。
  const [log, setLog] = React.useState("无操作");
  // hostOverrides and retentionOverrides retain each synthetic owner independently.
  // hostOverrides 与 retentionOverrides 独立保留每个合成归属方。
  const hostOverrides = React.useRef(new Map<string, VmmNativeHostProjectPolicyOverrides>());
  const retentionOverrides = React.useRef(new Map<number, VmmNativeProjectRetentionRules | null>());
  // runtimePolicy owns only synthetic global acknowledgement state for this mounted fixture.
  // runtimePolicy 仅持有此挂载夹具的合成全局确认状态。
  const runtimePolicy = React.useRef(structuredClone(policy));
  // defaults contains only the six project-owned fields in the real contract.
  // defaults 仅包含真实契约中六个项目归属字段。
  const defaults: VmmNativeProjectRetentionRules = { turnHotWindowSize: 8, sessionIdleMs: 259200000, trashRetentionMs: 604800000, protectedPriorityFloor: 80, protectedMemoryLevelFloor: 3, skipProtectedSharedMemories: true };
  // run accepts the four known actions and rejects every unimplemented fixture route.
  // run 接受四个已知动作，并拒绝全部未实现夹具路由。
  const run = React.useCallback(async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult | null> => {
    setLog(`${action} ${JSON.stringify(payload)}`);
    if (rejected) return null;
    if (action === "native_project_catalog") return { action, kind: action, summary: "", request: payload, response: {}, data: { kind: "native_project_catalog", value: { projects: [1, 2].map((projectId) => ({ projectId, name: "同名项目", tag: `memory-${projectId}`, protected: false, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] })), bindings: [] } } };
    if ((action === "native_project_profile_policy" || action === "native_project_profile_policy_update") && typeof payload?.projectId === "number") {
      if (action === "native_project_profile_policy_update") {
        if (payload.mode === "inherit") profileOverrides.current.delete(payload.projectId);
        else profileOverrides.current.set(payload.projectId, payload.rules as VmmNativeProfileLifetimes);
      }
      const overrides = profileOverrides.current.get(payload.projectId) ?? null;
      const value = { projectId: payload.projectId, overrides, effective: overrides ?? policy.profiles.project, global: policy.profiles.project };
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_project_profile_policy", value: structuredClone(value) } };
    }
    // groups maps only the five update contracts declared by the production editors.
    // groups 仅映射生产编辑器声明的五个更新契约。
    const groups = new Map<string, "recall" | "text" | "profiles" | "retrieval" | "retention">([
      ["native_runtime_policy_update", "recall"], ["native_text_policy_update", "text"],
      ["native_profile_policy_update", "profiles"], ["native_retrieval_policy_update", "retrieval"],
      ["native_retention_settings_update", "retention"],
    ]);
    // group identifies one explicit whole-group update, while reads require no payload.
    // group 标识一个明确完整配置组更新；读取不要求负载。
    const group = groups.get(action);
    if (action === "native_runtime_policy" || group) {
      if (group) {
        if (!payload) throw new Error("Missing synthetic global policy payload");
        runtimePolicy.current = { ...runtimePolicy.current, [group]: structuredClone(payload) } as VmmNativeRuntimePolicySummary;
      }
      return { action, kind: "native_runtime_policy", summary: "", request: payload, response: runtimePolicy.current, data: { kind: "native_runtime_policy", value: structuredClone(runtimePolicy.current) } };
    }
    if (!payload) throw new Error("Missing synthetic policy payload");
    if ((action === "native_host_project_policy" || action === "native_host_project_policy_update") && typeof payload.hostProjectId === "string") {
      if (action === "native_host_project_policy_update") hostOverrides.current.set(payload.hostProjectId, payload.overrides as VmmNativeHostProjectPolicyOverrides);
      // overrides follows the protocol's explicit nullable inheritance contract.
      // overrides 遵循协议明确的可空继承契约。
      const overrides = hostOverrides.current.get(payload.hostProjectId) ?? { recall: null, retrieval: null, text: null };
      // value is a fresh acknowledged envelope, preventing later edits from mutating prior responses.
      // value 是新的已确认信封，避免后续编辑修改先前响应。
      const value = { hostProjectId: payload.hostProjectId, overrides, effective: { ...policy, recall: overrides.recall ?? policy.recall, retrieval: overrides.retrieval ?? policy.retrieval, text: overrides.text ?? policy.text } };
      return { action, kind: "native_host_project_policy", summary: "", request: payload, response: value, data: { kind: "native_host_project_policy", value: structuredClone(value) } };
    }
    if ((action === "native_project_retention" || action === "native_project_retention_update") && typeof payload.projectId === "number") {
      if (action === "native_project_retention_update") retentionOverrides.current.set(payload.projectId, payload.mode === "inherit" ? null : payload.rules as VmmNativeProjectRetentionRules);
      // overrides remains null until an explicit synthetic save.
      // overrides 在明确合成保存之前保持空值。
      const overrides = retentionOverrides.current.get(payload.projectId) ?? null;
      // value carries the exact selected numeric project identity.
      // value 携带准确选中数值项目身份。
      const value = { projectId: payload.projectId, overrides, effective: overrides ?? defaults, global: defaults };
      return { action, kind: "native_project_retention", summary: "", request: payload, response: value, data: { kind: "native_project_retention", value: structuredClone(value) } };
    }
    throw new Error(`Unsupported synthetic policy action: ${action}`);
  }, [rejected]);
  return <I18nProvider initialLocale="zh-CN"><main className="h-dvh overflow-auto bg-background p-4 text-foreground"><div className="mx-auto max-w-3xl space-y-4">
    <h1>记忆策略隔离验收</h1>
    <div className="flex flex-wrap gap-2"><Button onClick={() => setRejected(!rejected)}>切换请求失败</Button><Button onClick={() => setBusy(!busy)}>切换忙碌</Button><Button onClick={() => setProject(project === 1 ? 2 : 1)}>切换宿主</Button></div>
    {query.get("entry") === "rules" ? <VmmRulesPage busy={busy} scope={scope} projectId={scopeProject} draftReady draft={draft} projects={snapshot.projectScopes} notice="" onSelectScope={(next, projectId) => { setScope(next); setScopeProject(projectId ?? ""); setLog(`选择范围 ${next} ${projectId}`); }} onChange={(patch) => setDraft({ ...draft, ...patch })} onApply={async (selectedScope, projectId, savedDraft) => { setLog(`保存规则 ${selectedScope} ${projectId} ${JSON.stringify(savedDraft)}`); return !rejected; }} /> : query.get("entry") === "backend" ? <VmmAdvancedPage busy={busy} draft={draft} snapshot={snapshot} notice="" onChange={(patch) => setDraft({ ...draft, ...patch })} onSave={async () => { setLog(`保存后端 ${draft.backendMode}`); return !rejected; }} onTestManagement={async () => { setLog("合成保存并测试管理连接"); return !rejected; }} onRunAction={run} /> : query.get("entry") === "project-profile" ? <VmmNativeProjectProfilePolicy busy={busy} onRunAction={run} selectedProjectId={query.get("owner") === "parent" ? project : undefined} projects={[1, 2].map((projectId) => ({ projectId, name: "同名项目", tag: `memory-${projectId}`, protected: false, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] }))} /> : isGlobal ? <VmmNativeRuntimePolicy busy={busy} onRunAction={run} /> : isHost ? <VmmNativeHostProjectPolicy key={project} hostProjectId={`audit-host-${project}`} busy={busy} onRunAction={run} /> : <VmmNativeProjectRetention busy={busy} onRunAction={run} projects={[1, 2].map((projectId) => ({ projectId, name: query.get("content") === "long" ? "SyntheticMemoryProject".repeat(20) : "同名项目", tag: `memory-${projectId}`, protected: projectId === 1, ownerVulcanProjectIds: [], linkedVulcanProjectIds: [] }))} />}
    <output aria-label="策略动作" className="block max-h-32 overflow-auto break-all text-xs">{log}</output>
  </div></main></I18nProvider>;
}
