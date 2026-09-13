import type { VmmMemoryActionResponse, VmmNativeMemoryRecordsSummary, VmmNativeMemorySessionSummary, VmmNativeSessionListQuery } from "../../src/types";
import { nativeCatalog } from "./fixtures";

// sessionRecords creates two clearly synthetic sessions inside one exact catalog project.
// sessionRecords 在准确目录项目内创建两个明确合成的会话。
// Parameters: projectId is an admitted catalog ID. Returns: a coherent read-only provenance bundle.
// 参数：projectId 是已准入目录标识。返回值：一致的只读来源组合。
function sessionRecords(projectId: number): VmmNativeMemoryRecordsSummary {
  // base reserves disjoint record identities for each fixture project.
  // base 为各夹具项目保留互不重叠的记录身份。
  const base = projectId * 1000;
  // timestamp fixes deterministic ordering without referring to the user's current history.
  // timestamp 固定确定性排序，不引用用户当前历史。
  const timestamp = Date.UTC(2026, 8, 12, 8);
  // session IDs remain stable across summary, selected detail and memory provenance.
  // 会话标识在摘要、选中详情及记忆来源间保持稳定。
  const activeId = `ui-audit-project-${projectId}-active`;
  const archivedId = `ui-audit-project-${projectId}-archived`;
  // summaries intentionally cover both a pending extraction and a completed archived session.
  // summaries 有意覆盖待提炼回合及已完成的归档会话。
  const summaries: VmmNativeMemorySessionSummary[] = [
    { sessionSourceId: activeId, displayTitle: "隔离验收：数据库设计与提炼", status: "active", turnCount: 2, turnStatuses: { pending: 1, completed: 1, processedUnknown: 0, analysisBypassed: 0, corrupted: 0 }, memoryCount: 2, directMemoryCount: 0, profileNodeCount: 0, memoryLevels: { session: 0, phase: 1, stable: 1, persistent: 0 }, firstActivityAtUnixMs: timestamp, lastActivityAtUnixMs: timestamp + 60000 },
    { sessionSourceId: archivedId, displayTitle: "隔离验收：已归档接口讨论", status: "archived", turnCount: 1, turnStatuses: { pending: 0, completed: 1, processedUnknown: 0, analysisBypassed: 0, corrupted: 0 }, memoryCount: 1, directMemoryCount: 0, profileNodeCount: 0, memoryLevels: { session: 0, phase: 0, stable: 1, persistent: 0 }, firstActivityAtUnixMs: timestamp - 60000, lastActivityAtUnixMs: timestamp - 60000 },
  ];
  return { projectId, storageVersion: 7, selectedSessionSourceId: null, sessions: summaries, profileNodes: [],
    turns: [
      { turnId: base + 1, sessionSourceId: activeId, previousTurnIds: [], nextTurnIds: [base + 2], createdAtUnixMs: timestamp, status: "completed", extracted: true, userPreview: "隔离样例：数据库采用 PostgreSQL，并说明迁移原则。", assistantPreview: "采用 PostgreSQL；迁移脚本必须可回滚，并保留索引说明。", timeline: [{ role: "user", content: "隔离样例：数据库采用 PostgreSQL，并说明迁移原则。", occurredAtUnixMs: timestamp }, { role: "assistant", content: "采用 PostgreSQL；迁移脚本必须可回滚，并保留索引说明。", occurredAtUnixMs: timestamp + 1000 }], analysisDetails: "合成提炼分析：从本回合识别数据库选型与迁移原则，生成两条子记忆；未调用真实模型。", generatedMemoryIds: [base + 101, base + 102], generatedProfileNodeIds: [], memoryLevels: { session: 0, phase: 1, stable: 1, persistent: 0 }, analysisJobState: "completed", analysisAttemptCount: 1, analysisLastError: null },
      { turnId: base + 2, sessionSourceId: activeId, previousTurnIds: [base + 1], nextTurnIds: [], createdAtUnixMs: timestamp + 60000, status: "pending", extracted: false, userPreview: "隔离样例：稍后补充索引评估。", assistantPreview: "已记录需要评估索引。", timeline: [{ role: "user", content: "隔离样例：稍后补充索引评估。", occurredAtUnixMs: timestamp + 60000 }], analysisDetails: null, generatedMemoryIds: [], generatedProfileNodeIds: [], memoryLevels: { session: 0, phase: 0, stable: 0, persistent: 0 }, analysisJobState: "queued", analysisAttemptCount: 0, analysisLastError: null },
      { turnId: base + 3, sessionSourceId: archivedId, previousTurnIds: [], nextTurnIds: [], createdAtUnixMs: timestamp - 60000, status: "completed", extracted: true, userPreview: "隔离样例：接口使用明确版本号。", assistantPreview: "接口路径采用显式版本号。", timeline: [{ role: "user", content: "隔离样例：接口使用明确版本号。", occurredAtUnixMs: timestamp - 60000 }], analysisDetails: "合成提炼分析：保留接口版本约定。", generatedMemoryIds: [base + 103], generatedProfileNodeIds: [], memoryLevels: { session: 0, phase: 0, stable: 1, persistent: 0 }, analysisJobState: "completed", analysisAttemptCount: 1, analysisLastError: null },
    ], memories: [
      { memoryId: base + 101, sourceTurnId: base + 1, sessionSourceId: activeId, summary: "隔离子记忆：数据库选型", content: "这是合成子记忆：项目数据库采用 PostgreSQL，用于验证完整正文、来源回合与返回导航。", category: "architecture_decision", sourceKind: "turn", state: "active", memoryLevel: "stable", priority: 2, tags: ["隔离验收", "数据库"], createdAtUnixMs: timestamp, updatedAtUnixMs: timestamp },
      { memoryId: base + 102, sourceTurnId: base + 1, sessionSourceId: activeId, summary: "隔离子记忆：迁移可回滚", content: "这是合成子记忆：每次数据库迁移都必须提供可回滚脚本和索引变更说明。", category: "tech_spec_api", sourceKind: "turn", state: "active", memoryLevel: "phase", priority: 2, tags: ["隔离验收", "迁移"], createdAtUnixMs: timestamp, updatedAtUnixMs: timestamp },
      { memoryId: base + 103, sourceTurnId: base + 3, sessionSourceId: archivedId, summary: "隔离子记忆：接口版本", content: "这是合成子记忆：对外接口通过路径中的版本号声明协议版本。", category: "tech_spec_api", sourceKind: "turn", state: "active", memoryLevel: "stable", priority: 2, tags: ["隔离验收"], createdAtUnixMs: timestamp - 60000, updatedAtUnixMs: timestamp - 60000 },
    ] };
}

// memorySessionRead admits only the existing two read actions and rejects foreign owners or stale versions.
// memorySessionRead 仅准入既有两个读取动作，并拒绝错误归属或过期版本。
// Parameters: action/payload are original RPC fields. Returns: typed read data or undefined for unsupported actions.
// 参数：action/payload 为原始 RPC 字段。返回值：强类型读取数据；不支持动作返回 undefined。
export function memorySessionRead(action: string, payload: Record<string, unknown>): VmmMemoryActionResponse | undefined {
  if (action !== "native_session_list" && action !== "native_memory_records") return undefined;
  if (typeof payload.vmmProjectId !== "number" || !nativeCatalog.projects.some((project) => project.projectId === payload.vmmProjectId)) throw new Error("UI_AUDIT_UNKNOWN_MEMORY_PROJECT");
  // records are freshly allocated so visual consumers cannot mutate the next response.
  // records 每次重新分配，避免视觉消费者修改下一次响应。
  const records = sessionRecords(payload.vmmProjectId);
  if (action === "native_memory_records") {
    if (payload.sessionSourceId === undefined || payload.sessionSourceId === null) return { kind: "native_memory_records", value: records };
    if (typeof payload.sessionSourceId !== "string" || !records.sessions.some((session) => session.sessionSourceId === payload.sessionSourceId)) throw new Error("UI_AUDIT_UNKNOWN_MEMORY_SESSION");
    return { kind: "native_memory_records", value: { ...records, selectedSessionSourceId: payload.sessionSourceId, turns: records.turns.filter((turn) => turn.sessionSourceId === payload.sessionSourceId), memories: records.memories.filter((memory) => memory.sessionSourceId === payload.sessionSourceId) } };
  }
  if (!Number.isInteger(payload.offset) || Number(payload.offset) < 0 || !Number.isInteger(payload.limit) || Number(payload.limit) < 1 || Number(payload.limit) > 100) throw new Error("UI_AUDIT_INVALID_MEMORY_PAGE");
  if (payload.expectedStorageVersion !== null && payload.expectedStorageVersion !== records.storageVersion) throw new Error("UI_AUDIT_STALE_MEMORY_VERSION");
  if (typeof payload.search !== "string" || !["all", "active", "archived", "recycled", "forgotten", "unregistered"].includes(String(payload.status)) || !["all", "pending", "completed", "processed_unknown", "analysis_bypassed", "corrupted"].includes(String(payload.extractionStatus)) || !["last_activity_desc", "last_activity_asc", "title_asc"].includes(String(payload.sort))) throw new Error("UI_AUDIT_INVALID_MEMORY_FILTER");
  if ((payload.lastActivityFromUnixMs !== null && typeof payload.lastActivityFromUnixMs !== "number") || (payload.lastActivityBeforeUnixMs !== null && typeof payload.lastActivityBeforeUnixMs !== "number")) throw new Error("UI_AUDIT_INVALID_MEMORY_DATE");
  // query is narrowed only after checking every field consumed by the production list contract.
  // query 仅在检查生产列表契约消费的全部字段后收窄。
  const query = payload as unknown as VmmNativeSessionListQuery;
  // sessions apply exact owner-local filters before calculating the bounded page total.
  // sessions 先应用准确归属内筛选，再计算有界分页总数。
  const sessions = records.sessions.filter((session) => (query.status === "all" || (query.status === "unregistered" ? session.status === null : session.status === query.status))
    && (query.extractionStatus === "all" || records.turns.some((turn) => turn.sessionSourceId === session.sessionSourceId && turn.status === query.extractionStatus))
    && `${session.displayTitle} ${session.sessionSourceId}`.toLocaleLowerCase().includes(query.search.trim().toLocaleLowerCase())
    && (query.lastActivityFromUnixMs === null || session.lastActivityAtUnixMs! >= query.lastActivityFromUnixMs)
    && (query.lastActivityBeforeUnixMs === null || session.lastActivityAtUnixMs! < query.lastActivityBeforeUnixMs));
  sessions.sort((left, right) => query.sort === "title_asc" ? left.displayTitle!.localeCompare(right.displayTitle!) : query.sort === "last_activity_asc" ? left.lastActivityAtUnixMs! - right.lastActivityAtUnixMs! : right.lastActivityAtUnixMs! - left.lastActivityAtUnixMs!);
  return { kind: "native_session_list", value: { projectId: records.projectId, storageVersion: records.storageVersion, total: sessions.length, offset: query.offset, limit: query.limit, sessions: sessions.slice(query.offset, query.offset + query.limit) } };
}
