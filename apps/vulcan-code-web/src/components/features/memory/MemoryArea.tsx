import * as React from "react"
import { ListRestart, RefreshCw } from "lucide-react"

import { useI18n } from "@/i18n"
import type {
  MemorySummary,
  VmmMemorySnapshot,
} from "@/types"

import { VmmEffectiveOverview, VmmResultPanel, VmmRuntimePanel } from "./vmm-memory-components"
import { renderVmmSectionDetail, type VmmScope } from "./vmm-memory-sections"
import {
  useMemoryAreaController,
  type MemoryActionProps,
} from "./use-memory-area-controller"
import { VmmInferenceSettings } from "./vmm-inference-settings"

export { vmmSectionIcon } from "./vmm-memory-sections"
export type { MemoryActionProps } from "./use-memory-area-controller"

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
  // controller owns exact draft routes and the exclusive asynchronous operation channel.
  // controller 持有准确草稿路由与独占异步操作通道。
  const controller = useMemoryAreaController({
    actions: props.actions,
    snapshot: props.snapshot,
  });
  // Controller fields expose the rendered scope, exact drafts, and guarded action callbacks.
  // 控制器字段暴露渲染作用域、准确草稿与受保护动作回调。
  const {
    busyAction,
    globalDraft,
    notice,
    operationBusy,
    projectDraft,
    projectDraftReady,
    projectId,
    result,
    runAction,
    saveDraft,
    scope,
    selectProject,
    selectScope,
    setActiveDraft,
    setGlobalDraft,
  } = controller;
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

  // activeDraft is the exact editable draft selected by the committed scope.
  // activeDraft 是已提交作用域选择的准确可编辑草稿。
  const activeDraft = scope === "global" ? globalDraft : projectDraft;
  // handleProjectIdChange starts an exact project draft load without leaking a floating promise to React.
  // handleProjectIdChange 启动准确项目草稿加载，并避免向 React 泄漏悬空 Promise。
  const handleProjectIdChange = React.useCallback((nextProjectId: string) => {
    void selectProject(nextProjectId);
  }, [selectProject]);

  const activeSectionDetail = renderVmmSectionDetail({
    activeDraft,
    activeSection: props.activeSection,
    busyAction,
    globalDraft,
    migrateConfirmed,
    migrateSource,
    migrateTarget,
    notice,
    operationBusy,
    profileInstruction,
    profileTarget,
    projectDeleteConfirmed,
    projectDraft,
    projectDraftReady,
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
    setProjectId: handleProjectIdChange,
    setProjectPath,
    setScope: selectScope,
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

  if (props.activeSection === "inference") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmInferenceSettings />
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
              <select disabled={operationBusy} value={scope} onChange={(event) => selectScope(event.target.value as VmmScope)}>
                <option value="global">{t("memory.scope.global")}</option>
                <option value="project">{t("memory.scope.project")}</option>
              </select>
            </label>
            <label>
              <span>{t("memory.project")}</span>
              <select disabled={scope !== "project" || operationBusy} value={projectId} onChange={(event) => handleProjectIdChange(event.target.value)}>
                {props.snapshot.projectScopes.map((project) => (
                  <option key={project.projectId} value={project.projectId}>{project.name}</option>
                ))}
              </select>
            </label>
            <button className="vmm-icon-button" disabled={operationBusy} onClick={() => runAction("healthz")} type="button">
              <RefreshCw size={15} />{t("memory.action.testConnection")}
            </button>
            <button className="vmm-icon-button" disabled={operationBusy} onClick={() => runAction("load_users_projects")} type="button">
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


