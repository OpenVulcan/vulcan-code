import * as React from "react"
import { ListRestart, RefreshCw } from "lucide-react"

import { useI18n } from "@/i18n"
import type {
  MemorySummary,
  VmmConfigDraftSummary,
  VmmConfigSaveRequest,
  VmmMemoryActionRequest,
  VmmMemoryActionResult,
  VmmMemorySnapshot,
} from "@/types"

import { VmmEffectiveOverview, VmmResultPanel, VmmRuntimePanel } from "./vmm-memory-components"
import { renderVmmSectionDetail, type VmmScope } from "./vmm-memory-sections"

export { vmmSectionIcon } from "./vmm-memory-sections"

// MemoryActionProps groups service-backed actions used by the VMM memory page.
// MemoryActionProps 分组 VMM 记忆页面使用的服务端动作。
export interface MemoryActionProps {
  // Runs one VMM memory management action.
  // 执行单个 VMM 记忆管理动作。
  onRunVmmAction: (request: VmmMemoryActionRequest) => Promise<VmmMemoryActionResult>
  // Saves one VMM memory configuration draft.
  // 保存单个 VMM 记忆配置草稿。
  onSaveVmmConfig: (request: VmmConfigSaveRequest) => Promise<void>
}

// MemorySubAreaId identifies the selected VMM memory management page.
// MemorySubAreaId 标识当前选中的 VMM 记忆管理页面。
export type MemorySubAreaId = string

// MemoryArea renders the VMM memory management workbench.
// MemoryArea 渲染 VMM 记忆管理工作台。
export function MemoryArea(props: {
  // Active VMM section selected from the shared feature sidebar.
  // 从共享功能侧栏选中的当前 VMM 分区。
  activeSection: MemorySubAreaId
  // Service-backed VMM actions.
  // 服务端支持的 VMM 动作。
  actions: MemoryActionProps
  // Memory summaries shown in the memory page.
  // 记忆页展示的记忆摘要。
  memories: MemorySummary[]
  // Detailed VMM snapshot.
  // 详细 VMM 快照。
  snapshot: VmmMemorySnapshot
}) {
  const { t } = useI18n();
  const [scope, setScope] = React.useState<VmmScope>("global");
  const [projectId, setProjectId] = React.useState(props.snapshot.selectedProjectId ?? props.snapshot.projectScopes[0]?.projectId ?? "");
  const [globalDraft, setGlobalDraft] = React.useState(props.snapshot.globalDraft);
  const [projectDraft, setProjectDraft] = React.useState(props.snapshot.projectDraft);
  const [result, setResult] = React.useState<VmmMemoryActionResult | null>(null);
  const [notice, setNotice] = React.useState(props.snapshot.notice);
  const [busyAction, setBusyAction] = React.useState<string | null>(null);
  const [userRef, setUserRef] = React.useState(props.snapshot.effective.userId?.toString() ?? "");
  const [userConfirmationCode, setUserConfirmationCode] = React.useState("");
  const [projectPath, setProjectPath] = React.useState("");
  const [projectDeleteConfirmed, setProjectDeleteConfirmed] = React.useState(false);
  const [migrateSource, setMigrateSource] = React.useState("");
  const [migrateTarget, setMigrateTarget] = React.useState("");
  const [migrateConfirmed, setMigrateConfirmed] = React.useState(false);
  const [profileTarget, setProfileTarget] = React.useState("user");
  const [profileInstruction, setProfileInstruction] = React.useState("");
  const [searchQueries, setSearchQueries] = React.useState("");
  const [searchTopK, setSearchTopK] = React.useState("5");
  const [turnIds, setTurnIds] = React.useState("");
  const [writeAbstract, setWriteAbstract] = React.useState("");
  const [writeDetails, setWriteDetails] = React.useState("");
  const [writeScopeLevel, setWriteScopeLevel] = React.useState("2");
  const [writeCategory, setWriteCategory] = React.useState("1");
  const [writePriority, setWritePriority] = React.useState("2");
  const [writeMemoryLevel, setWriteMemoryLevel] = React.useState("2");
  const [deleteMemoryIds, setDeleteMemoryIds] = React.useState("");
  const [deleteReason, setDeleteReason] = React.useState("");

  React.useEffect(() => {
    setGlobalDraft(props.snapshot.globalDraft);
    setProjectDraft(props.snapshot.projectDraft);
    setProjectId(props.snapshot.selectedProjectId ?? props.snapshot.projectScopes[0]?.projectId ?? "");
    setNotice(props.snapshot.notice);
  }, [props.snapshot]);

  const activeDraft = scope === "global" ? globalDraft : projectDraft;
  const setActiveDraft = (patch: Partial<VmmConfigDraftSummary>) => {
    if (scope === "global") {
      setGlobalDraft((draft) => ({ ...draft, ...patch }));
    } else {
      setProjectDraft((draft) => ({ ...draft, ...patch }));
    }
  };

  const runAction = async (action: string, payload: Record<string, unknown> = {}) => {
    setBusyAction(action);
    setNotice(t("memory.vmm.action.running").replace("{action}", action));
    try {
      const nextResult = await props.actions.onRunVmmAction({
        action,
        scope,
        projectId: scope === "project" ? projectId : null,
        payload,
      });
      setResult((currentResult) => vmmResultAfterAction(currentResult, nextResult, action));
      setNotice(nextResult.summary);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setNotice(t("memory.vmm.action.failed").replace("{message}", message));
    } finally {
      setBusyAction(null);
    }
  };

  const saveDraft = async (targetScope: VmmScope = scope) => {
    const draft = targetScope === "global" ? globalDraft : projectDraft;
    setBusyAction(`save-${targetScope}`);
    setNotice(targetScope === "global" ? t("memory.vmm.save.global.running") : t("memory.vmm.save.project.running"));
    try {
      await props.actions.onSaveVmmConfig({
        scope: targetScope,
        projectId: targetScope === "project" ? projectId : null,
        draft,
      });
      setNotice(targetScope === "global" ? t("memory.vmm.save.global.completed") : t("memory.vmm.save.project.completed"));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setNotice(t("memory.vmm.save.failed").replace("{message}", message));
    } finally {
      setBusyAction(null);
    }
  };

  const activeSectionDetail = renderVmmSectionDetail({
    activeDraft,
    activeSection: props.activeSection,
    busyAction,
    globalDraft,
    migrateConfirmed,
    migrateSource,
    migrateTarget,
    notice,
    profileInstruction,
    profileTarget,
    projectDeleteConfirmed,
    projectDraft,
    projectId,
    projectPath,
    result,
    scope,
    searchQueries,
    searchTopK,
    setActiveDraft,
    setGlobalDraft,
    setMigrateConfirmed,
    setMigrateSource,
    setMigrateTarget,
    setProfileInstruction,
    setProfileTarget,
    setProjectDeleteConfirmed,
    setProjectId,
    setProjectPath,
    setScope,
    setSearchQueries,
    setSearchTopK,
    setTurnIds,
    setUserConfirmationCode,
    setUserRef,
    setDeleteMemoryIds,
    setDeleteReason,
    setWriteAbstract,
    setWriteCategory,
    setWriteDetails,
    setWriteMemoryLevel,
    setWritePriority,
    setWriteScopeLevel,
    snapshot: props.snapshot,
    t,
    turnIds,
    userConfirmationCode,
    userRef,
    deleteMemoryIds,
    deleteReason,
    writeAbstract,
    writeCategory,
    writeDetails,
    writeMemoryLevel,
    writePriority,
    writeScopeLevel,
    onRunAction: runAction,
    onSaveDraft: saveDraft,
  });

  if (props.activeSection === "status") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmEffectiveOverview memories={props.memories} snapshot={props.snapshot} />
            <div className="vmm-status-grid">
              <VmmRuntimePanel snapshot={props.snapshot} />
              <VmmResultPanel result={result} />
            </div>
          </main>
        </div>
      </section>
    )
  }

  return (
    <section className="vmm-memory-page">
      <div className="vmm-memory-shell">
        <main className="vmm-memory-main">
          <div className="vmm-scope-bar">
            <label>
              <span>{t("memory.scope")}</span>
              <select value={scope} onChange={(event) => setScope(event.target.value as VmmScope)}>
                <option value="global">{t("memory.scope.global")}</option>
                <option value="project">{t("memory.scope.project")}</option>
              </select>
            </label>
            <label>
              <span>{t("memory.project")}</span>
              <select disabled={scope !== "project"} value={projectId} onChange={(event) => setProjectId(event.target.value)}>
                {props.snapshot.projectScopes.map((project) => (
                  <option key={project.projectId} value={project.projectId}>{project.name}</option>
                ))}
              </select>
            </label>
            <button className="vmm-icon-button" onClick={() => runAction("healthz")} type="button">
              <RefreshCw size={15} />{t("memory.action.testConnection")}
            </button>
            <button className="vmm-icon-button" onClick={() => runAction("load_users_projects")} type="button">
              <ListRestart size={15} />{t("memory.action.refreshUsersProjects")}
            </button>
          </div>
          {activeSectionDetail}
        </main>
        <aside className="vmm-memory-result">
          <VmmRuntimePanel snapshot={props.snapshot} />
          <VmmResultPanel result={result} />
        </aside>
      </div>
    </section>
  )
}

// vmmResultAfterAction preserves search results when deletion removes visible rows.
// vmmResultAfterAction 在删除记忆移除可见行时保留当前搜索结果。
function vmmResultAfterAction(
  currentResult: VmmMemoryActionResult | null,
  nextResult: VmmMemoryActionResult,
  action: string,
): VmmMemoryActionResult {
  if (action !== "delete_memories" || currentResult?.kind !== "search_memory_events") {
    return nextResult;
  }
  const deletedIds = vmmDeletedMemoryIdSet(nextResult.response);
  if (deletedIds.size === 0) {
    return currentResult;
  }
  return {
    ...currentResult,
    summary: nextResult.summary,
    response: vmmSearchResultWithoutMemoryIds(currentResult.response, deletedIds),
  };
}

// vmmDeletedMemoryIdSet extracts deleted ids from one delete response.
// vmmDeletedMemoryIdSet 从删除响应中提取已删除标识集合。
function vmmDeletedMemoryIdSet(response: unknown): Set<number> {
  const object = vmmObject(response);
  const values = object.deletedMemoryIds ?? object.deleted_memory_ids;
  if (!Array.isArray(values)) {
    return new Set();
  }
  return new Set(values.map(vmmNumber).filter((value) => value > 0));
}

// vmmSearchResultWithoutMemoryIds removes deleted hits from one search response.
// vmmSearchResultWithoutMemoryIds 从搜索响应中移除已删除的命中项。
function vmmSearchResultWithoutMemoryIds(response: unknown, deletedIds: Set<number>): unknown {
  const object = vmmObject(response);
  const groups = Array.isArray(object.results) ? object.results : [];
  return {
    ...object,
    results: groups.map((group) => {
      const groupObject = vmmObject(group);
      const hits = Array.isArray(groupObject.hits) ? groupObject.hits : [];
      return {
        ...groupObject,
        hits: hits.filter((hit) => !deletedIds.has(vmmNumber(vmmObject(hit).memoryId ?? vmmObject(hit).memory_id))),
      };
    }),
  };
}

// vmmObject normalizes unknown values into an object.
// vmmObject 将未知值规范化为对象。
function vmmObject(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null ? value as Record<string, unknown> : {};
}

// vmmNumber normalizes unknown values into a finite number.
// vmmNumber 将未知值规范化为有限数字。
function vmmNumber(value: unknown): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : 0;
  }
  return 0;
}

