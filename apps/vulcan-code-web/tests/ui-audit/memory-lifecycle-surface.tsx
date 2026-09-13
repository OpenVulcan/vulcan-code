import React from "react";
import { I18nProvider } from "../../src/i18n";
import { Button } from "../../src/components/ui/button";
import { VmmNativeSessionArchive } from "../../src/components/features/memory/vmm-native-session-archive";
import { VmmNativeRemoval } from "../../src/components/features/memory/vmm-native-removal";
import { VmmNativeRecycleRecords } from "../../src/components/features/memory/vmm-native-recycle-records";
import type { VmmMemoryActionResult, VmmNativeMemorySessionSummary, VmmNativeRemovalPreview, VmmNativeRecycleRecordQuery, VmmNativeRecycleRecord } from "../../src/types";

/** Exercises native lifecycle confirmations using synthetic in-memory responses only.
 * 仅使用合成内存响应，验收原生生命周期确认。
 * Takes no parameters; returns the selected production controls and observable action log.
 * 无参数；返回选定生产控件与可观察动作日志。
 */
export function AuditMemoryLifecycleSurface() {
  // query chooses a declared scenario; project changes preserve mounted component identity.
  // query 选择已声明场景；项目变化保留组件挂载身份。
  const query = new URLSearchParams(window.location.search);
  const [project, setProject] = React.useState(7);
  const [busy, setBusy] = React.useState(false);
  const [failed, setFailed] = React.useState(false);
  const [log, setLog] = React.useState("无操作");
  // session carries the complete summary contract with deliberately long display-only titles.
  // session 携带完整摘要契约，使用刻意加长的仅供显示标题。
  const session: VmmNativeMemorySessionSummary = { sessionSourceId: "audit-session", displayTitle: query.get("content") === "long" ? "合成会话标题LongTitle".repeat(40) : "合成验收会话", status: query.get("status") === "forgotten" ? "forgotten" : query.get("status") === "archived" ? "archived" : "active", turnCount: 1, memoryCount: 1, profileNodeCount: 1, directMemoryCount: 0, turnStatuses: { pending: 0, completed: 1, processedUnknown: 0, analysisBypassed: 0, corrupted: 0 }, memoryLevels: { session: 1, phase: 0, stable: 0, persistent: 0 } };
  // target and preview use only stable synthetic identities; counts come from the declared impact.
  // target 与 preview 只使用稳定合成身份；计数来自已声明影响对象。
  const target = { kind: "sessions" as const, ids: [session.sessionSourceId] };
  const preview: VmmNativeRemovalPreview = { previewId: "audit-removal-preview", trashRetentionMs: 172800000, impact: { query: { vmmProjectId: project, selection: target }, storageVersion: 19, hotWindowSize: 5, sessionSourceIds: target.ids, turnIds: ["audit-turn"], pendingTurnIds: [], hotWindowTurnIds: ["audit-turn"], memoryIds: ["audit-memory"], vectorMemoryIds: ["audit-memory"], projectProfileNodeIds: [], userProfileNodeIds: [], contextEvidence: [] } };
  // run accepts only the exact lifecycle/read actions represented on this isolated surface.
  // run 仅接纳此隔离表面对应的准确生命周期及读取动作。
  const run = async (action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult | null> => {
    setLog(`${action} ${JSON.stringify(payload)}`);
    if (failed) return null;
    if (action === "native_session_archive_preview") {
      const value = { previewId: "audit-archive-preview", storageVersion: 19, selection: { vmmProjectId: project, sessionSourceIds: target.ids, archived: session.status === "active" } };
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_session_archive_preview", value } };
    }
    if (action === "native_session_archive" || action === "native_session_restart") {
      const value = { storageVersion: 20, selection: { vmmProjectId: project, sessionSourceIds: target.ids, archived: session.status === "active" } };
      return { action, kind: action, summary: "", request: payload, response: value, data: action === "native_session_archive" ? { kind: "native_session_archive", value } : { kind: "native_session_restart", value } };
    }
    if (action === "native_session_restart_preview") {
      const value = { previewId: "audit-restart-preview", storageVersion: 19, selection: { vmmProjectId: project, sessionSourceIds: target.ids } };
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_session_restart_preview", value } };
    }
    if (action === "native_removal_preview") return { action, kind: action, summary: "", request: payload, response: preview, data: { kind: "native_removal_preview", value: preview } };
    if (action === "native_removal_recycle") {
      const value = { storageVersion: 20, batchId: "audit-batch", impact: preview.impact };
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_removal", value } };
    }
    if (action === "native_recycle_records") {
      if (!payload) throw new Error("Missing synthetic page request");
      const pageQuery = payload as unknown as VmmNativeRecycleRecordQuery;
      const text = query.get("content") === "long" ? "SyntheticRetainedText".repeat(300) : "合成保留内容";
      const records: VmmNativeRecycleRecord[] = pageQuery.recordKind === "turn" ? Array.from({ length: pageQuery.offset === 0 ? 20 : 1 }, (_, i) => ({ kind: "turn", turnId: `audit-turn-${pageQuery.offset + i}`, sessionSourceId: "audit-session", status: "completed", analysisDetails: text, timeline: [{ role: "user", content: text, occurredAtUnixMs: 1 }] })) : pageQuery.recordKind === "memory" ? [{ kind: "memory", memoryId: "audit-memory", sessionSourceId: "audit-session", sourceTurnId: "audit-turn", summary: text, content: text, level: "stable", state: "active", sourceKind: "turn" }] : pageQuery.recordKind === "profile" ? [{ kind: "profile", profileNodeId: "audit-profile", target: "user", sourceTurnIds: ["audit-turn"], content: text }] : [{ kind: "evidence", memoryId: "audit-memory", contextKey: "topic", contextValue: text, supportCount: 1, rebuttalCount: 0 }];
      const value = { query: pageQuery, storageVersion: 19, total: pageQuery.recordKind === "turn" ? 21 : 1, nextOffset: pageQuery.recordKind === "turn" && pageQuery.offset === 0 ? 20 : null, records };
      return { action, kind: action, summary: "", request: payload, response: value, data: { kind: "native_recycle_records", value } };
    }
    throw new Error(`Unexpected synthetic lifecycle action: ${action}`);
  };
  // refresh is observable without fabricating changed persistent session state.
  // refresh 可供观察，但不虚构持久化会话状态变化。
  const refresh = async () => { setLog((previous) => `${previous}\n刷新已调用`); };
  return <I18nProvider initialLocale="zh-CN"><main className="h-dvh overflow-auto bg-background p-4 text-foreground"><div className="mx-auto max-w-3xl space-y-4">
    <h1>记忆生命周期隔离验收</h1>
    <div className="flex flex-wrap gap-2"><Button onClick={() => setFailed(!failed)}>切换请求失败</Button><Button onClick={() => setProject(project === 7 ? 8 : 7)}>切换项目</Button><Button onClick={() => setBusy(!busy)}>切换忙碌</Button></div>
    <VmmNativeSessionArchive projectId={project} sessions={[session]} busy={busy} onBusyChange={setBusy} onRunAction={run} onRefresh={refresh} />
    <VmmNativeRemoval projectId={project} selection={target} sessionNames={new Map(typeof session.displayTitle === "string" ? [[session.sessionSourceId, session.displayTitle]] : [])} busy={busy} onBusyChange={setBusy} onRunAction={run} onRefresh={refresh} />
    <VmmNativeRecycleRecords busy={busy} onRunAction={run} batch={{ manualSelection: null, batchId: "audit-batch", projectId: project, reason: "idle_session_recycle", recycledAtUnixMs: 1, expiresAtUnixMs: 100, turnCount: 21, memoryCount: 1, profileNodeCount: 1, contextEvidenceCount: 1 }} />
    <output aria-label="生命周期动作" className="block max-h-32 overflow-auto break-all text-xs">{log}</output>
  </div></main></I18nProvider>;
}
