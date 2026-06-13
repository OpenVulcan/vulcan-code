import * as React from "react"
import { ListRestart, RefreshCw } from "lucide-react"

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
    setNotice(`正在执行 VMM 动作：${action}`);
    try {
      const nextResult = await props.actions.onRunVmmAction({
        action,
        scope,
        projectId: scope === "project" ? projectId : null,
        payload,
      });
      setResult(nextResult);
      setNotice(nextResult.summary);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setNotice(`VMM 动作失败：${message}`);
    } finally {
      setBusyAction(null);
    }
  };

  const saveDraft = async (targetScope: VmmScope = scope) => {
    const draft = targetScope === "global" ? globalDraft : projectDraft;
    setBusyAction(`save-${targetScope}`);
    setNotice(targetScope === "global" ? "正在保存全局 VMM 配置。" : "正在保存项目 VMM 配置。");
    try {
      await props.actions.onSaveVmmConfig({
        scope: targetScope,
        projectId: targetScope === "project" ? projectId : null,
        draft,
      });
      setNotice(targetScope === "global" ? "已保存全局 VMM 配置。" : "已保存项目 VMM 配置。");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setNotice(`保存 VMM 配置失败：${message}`);
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
    setWriteAbstract,
    setWriteCategory,
    setWriteDetails,
    setWriteMemoryLevel,
    setWritePriority,
    setWriteScopeLevel,
    snapshot: props.snapshot,
    turnIds,
    userConfirmationCode,
    userRef,
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
              <span>作用域</span>
              <select value={scope} onChange={(event) => setScope(event.target.value as VmmScope)}>
                <option value="global">全局配置</option>
                <option value="project">项目配置</option>
              </select>
            </label>
            <label>
              <span>项目</span>
              <select disabled={scope !== "project"} value={projectId} onChange={(event) => setProjectId(event.target.value)}>
                {props.snapshot.projectScopes.map((project) => (
                  <option key={project.projectId} value={project.projectId}>{project.name}</option>
                ))}
              </select>
            </label>
            <button className="vmm-icon-button" onClick={() => runAction("healthz")} type="button">
              <RefreshCw size={15} />测试连接
            </button>
            <button className="vmm-icon-button" onClick={() => runAction("load_users_projects")} type="button">
              <ListRestart size={15} />刷新用户/项目
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

