import * as React from "react"
import {
  BrainCircuit,
  Database,
  FileText,
  FolderTree,
  GitBranch,
  Hammer,
  PackagePlus,
  PlugZap,
  RefreshCw,
  Save,
  ScrollText,
  Search,
  SlidersHorizontal,
  TerminalSquare,
  Trash2,
  User,
} from "lucide-react"

import type { useI18n } from "@/i18n"
import type { VmmConfigDraftSummary, VmmMemoryActionResult, VmmMemorySnapshot } from "@/types"

import {
  VmmActionButton,
  VmmBundlePreview,
  VmmPanelHeading,
  VmmProfileRows,
  VmmProjectRows,
  VmmSettingsForm,
  VmmTextField,
  VmmToolResult,
  VmmUserRows,
} from "./vmm-memory-components"

// VmmScope identifies whether the VMM page edits global or project configuration.
// VmmScope 标识 VMM 页面编辑全局配置还是项目配置。
export type VmmScope = "global" | "project"

// VmmSectionRenderProps groups state and callbacks used by VMM section renderers.
// VmmSectionRenderProps 分组 VMM 分区渲染器使用的状态与回调。
export interface VmmSectionRenderProps {
  // Active configuration draft for the selected scope.
  // 当前作用域的激活配置草稿。
  activeDraft: VmmConfigDraftSummary
  // Active VMM section id.
  // 当前 VMM 分区标识。
  activeSection: string
  // Action id currently running.
  // 当前正在运行的动作标识。
  busyAction: string | null
  // Whether one exclusive memory-page operation prevents new submissions.
  // 是否有一个独占记忆页面操作阻止新提交。
  operationBusy: boolean
  // Global configuration draft.
  // 全局配置草稿。
  globalDraft: VmmConfigDraftSummary
  // Whether project migration has been confirmed.
  // 是否确认项目迁移。
  migrateConfirmed: boolean
  // Source project path for migration.
  // 迁移源项目路径。
  migrateSource: string
  // Target project path for migration.
  // 迁移目标项目路径。
  migrateTarget: string
  // Latest notice text.
  // 最新通知文本。
  notice: string
  // Profile instruction text.
  // 画像指令文本。
  profileInstruction: string
  // Selected profile target.
  // 选中的画像目标。
  profileTarget: string
  // Whether project deletion has been confirmed.
  // 是否确认删除项目。
  projectDeleteConfirmed: boolean
  // Project configuration draft.
  // 项目配置草稿。
  projectDraft: VmmConfigDraftSummary
  // Whether projectDraft belongs to the currently selected project id.
  // projectDraft 是否属于当前选中项目标识。
  projectDraftReady: boolean
  // Selected project id.
  // 选中的项目标识。
  projectId: string
  // Project path input.
  // 项目路径输入。
  projectPath: string
  // Latest action result.
  // 最新动作结果。
  result: VmmMemoryActionResult | null
  // Current VMM scope.
  // 当前 VMM 作用域。
  scope: VmmScope
  // Search queries input.
  // 检索查询输入。
  searchQueries: string
  // Search top-k input.
  // 检索 top-k 输入。
  searchTopK: string
  // Detailed VMM snapshot.
  // 详细 VMM 快照。
  snapshot: VmmMemorySnapshot
  // Translation helper for user-facing VMM section labels.
  // 用户可见 VMM 分区标签的翻译辅助函数。
  t: ReturnType<typeof useI18n>["t"]
  // Turn ids input.
  // 回合标识输入。
  turnIds: string
  // Memory ids selected for deletion.
  // 选中用于删除的记忆标识输入。
  deleteMemoryIds: string
  // Audit reason used by memory delete actions.
  // 记忆删除动作使用的审计原因。
  deleteReason: string
  // User deletion confirmation code.
  // 用户删除确认码。
  userConfirmationCode: string
  // User reference input.
  // 用户引用输入。
  userRef: string
  // Manual memory abstract input.
  // 手工记忆摘要输入。
  writeAbstract: string
  // Manual memory category input.
  // 手工记忆分类输入。
  writeCategory: string
  // Manual memory details input.
  // 手工记忆详情输入。
  writeDetails: string
  // Manual memory level input.
  // 手工记忆级别输入。
  writeMemoryLevel: string
  // Manual memory priority input.
  // 手工记忆优先级输入。
  writePriority: string
  // Manual memory scope input.
  // 手工记忆作用域输入。
  writeScopeLevel: string
  // Updates the active draft.
  // 更新当前激活草稿。
  setActiveDraft: (patch: Partial<VmmConfigDraftSummary>) => void
  // Updates the global draft.
  // 更新全局草稿。
  setGlobalDraft: React.Dispatch<React.SetStateAction<VmmConfigDraftSummary>>
  setMigrateConfirmed: (value: boolean) => void
  setMigrateSource: (value: string) => void
  setMigrateTarget: (value: string) => void
  setProfileInstruction: (value: string) => void
  setProfileTarget: (value: string) => void
  setProjectDeleteConfirmed: (value: boolean) => void
  setProjectId: (value: string) => void
  setProjectPath: (value: string) => void
  setScope: (value: VmmScope) => void
  setSearchQueries: (value: string) => void
  setSearchTopK: (value: string) => void
  setTurnIds: (value: string) => void
  setDeleteMemoryIds: (value: string) => void
  setDeleteReason: (value: string) => void
  setUserConfirmationCode: (value: string) => void
  setUserRef: (value: string) => void
  setWriteAbstract: (value: string) => void
  setWriteCategory: (value: string) => void
  setWriteDetails: (value: string) => void
  setWriteMemoryLevel: (value: string) => void
  setWritePriority: (value: string) => void
  setWriteScopeLevel: (value: string) => void
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<void>
  onSaveDraft: (scope?: VmmScope) => Promise<void>
}

// renderVmmSectionDetail renders the active VMM management section.
// renderVmmSectionDetail 渲染当前激活的 VMM 管理分区。
export function renderVmmSectionDetail(props: VmmSectionRenderProps) {
  const { t } = props;
  // projectDraftUnavailable prevents one project's form from rendering under another project's id.
  // projectDraftUnavailable 防止一个项目的表单渲染到另一个项目标识下。
  const projectDraftUnavailable = props.scope === "project" && !props.projectDraftReady;
  if (props.activeSection === "project_manager") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<FolderTree size={16} />} title={t("vmm.section.project.title")} subtitle={t("vmm.section.project.subtitle")} />
        <div className="vmm-form-grid">
          <VmmTextField label={t("vmm.field.projectPath")} placeholder="Team/Space/Project" value={props.projectPath} onChange={props.setProjectPath} />
          <label className="vmm-check-row"><input checked={props.projectDeleteConfirmed} onChange={(event) => props.setProjectDeleteConfirmed(event.target.checked)} type="checkbox" />{t("vmm.field.confirmDelete")}</label>
          <VmmTextField label={t("vmm.field.migrateSource")} placeholder="OldTeam/OldSpace/OldProject" value={props.migrateSource} onChange={props.setMigrateSource} />
          <VmmTextField label={t("vmm.field.migrateTarget")} placeholder="NewTeam/NewSpace/NewProject" value={props.migrateTarget} onChange={props.setMigrateTarget} />
          <label className="vmm-check-row"><input checked={props.migrateConfirmed} onChange={(event) => props.setMigrateConfirmed(event.target.checked)} type="checkbox" />{t("vmm.field.confirmMigrate")}</label>
        </div>
        <div className="vmm-action-row">
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "ensure_project"} icon={<Search size={15} />} label={t("vmm.action.precheckProject")} onClick={() => props.onRunAction("ensure_project", { projectPath: props.projectPath, confirmCreate: false })} />
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "ensure_project"} icon={<PackagePlus size={15} />} label={t("vmm.action.confirmCreate")} onClick={() => props.onRunAction("ensure_project", { projectPath: props.projectPath, confirmCreate: true })} />
          <VmmActionButton disabled={props.operationBusy} danger busy={props.busyAction === "delete_project"} icon={<Trash2 size={15} />} label={t("vmm.action.deleteProject")} onClick={() => props.onRunAction("delete_project", { projectPath: props.projectPath, confirmDelete: props.projectDeleteConfirmed })} />
          <VmmActionButton disabled={props.operationBusy} danger busy={props.busyAction === "migrate_project"} icon={<GitBranch size={15} />} label={t("vmm.action.runMigrate")} onClick={() => props.onRunAction("migrate_project", { sourceProjectPath: props.migrateSource, targetProjectPath: props.migrateTarget, confirmMigrate: props.migrateConfirmed })} />
        </div>
        <VmmProjectRows result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "profile_center") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<BrainCircuit size={16} />} title={t("vmm.section.profile.title")} subtitle={t("vmm.section.profile.subtitle")} />
        <div className="vmm-form-grid">
          <label className="vmm-field">
            <span>{t("vmm.field.profileTarget")}</span>
            <select value={props.profileTarget} onChange={(event) => props.setProfileTarget(event.target.value)}>
              <option value="user">{t("vmm.profile.user")}</option>
              <option value="project">{t("vmm.profile.project")}</option>
              <option value="team">{t("vmm.profile.team")}</option>
              <option value="space">{t("vmm.profile.space")}</option>
            </select>
          </label>
          <VmmTextField label={t("vmm.field.profileInstruction")} placeholder={t("vmm.placeholder.profileInstruction")} value={props.profileInstruction} onChange={props.setProfileInstruction} />
        </div>
        <div className="vmm-action-row">
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "load_profile_nodes"} icon={<RefreshCw size={15} />} label={t("vmm.action.refreshNodes")} onClick={() => props.onRunAction("load_profile_nodes", { target: props.profileTarget })} />
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "apply_profile_instruction"} icon={<Hammer size={15} />} label={t("vmm.action.applyInstruction")} onClick={() => props.onRunAction("apply_profile_instruction", { target: props.profileTarget, instruction: props.profileInstruction })} />
        </div>
        <VmmProfileRows result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "profile_bundle") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<ScrollText size={16} />} title={t("vmm.section.bundle.title")} subtitle={t("vmm.section.bundle.subtitle")} />
        <div className="vmm-action-row">
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "load_profile_bundle"} icon={<RefreshCw size={15} />} label={t("vmm.action.loadProfileBundle")} onClick={() => props.onRunAction("load_profile_bundle")} />
        </div>
        <VmmBundlePreview result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "tools_debug") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<TerminalSquare size={16} />} title={t("vmm.section.tool.title")} subtitle={t("vmm.section.tool.subtitle")} />
        <div className="vmm-form-grid">
          <VmmTextField label={t("vmm.field.searchQueries")} placeholder={t("vmm.placeholder.multiPipe")} value={props.searchQueries} onChange={props.setSearchQueries} />
          <VmmTextField label="Top K" placeholder="5" value={props.searchTopK} onChange={props.setSearchTopK} />
          <VmmTextField label="Turn IDs" placeholder={t("vmm.placeholder.commaOrPipe")} value={props.turnIds} onChange={props.setTurnIds} />
          <VmmTextField label="Memory IDs" placeholder={t("vmm.placeholder.commaOrPipe")} value={props.deleteMemoryIds} onChange={props.setDeleteMemoryIds} />
          <VmmTextField label={t("vmm.field.deleteReason")} placeholder={t("vmm.placeholder.deleteReason")} value={props.deleteReason} onChange={props.setDeleteReason} />
          <VmmTextField label={t("vmm.field.writeAbstract")} placeholder={t("vmm.placeholder.writeAbstract")} value={props.writeAbstract} onChange={props.setWriteAbstract} />
          <VmmTextField label={t("vmm.field.writeDetails")} placeholder={t("vmm.placeholder.writeDetails")} value={props.writeDetails} onChange={props.setWriteDetails} />
          <VmmTextField label="Scope" placeholder="2" value={props.writeScopeLevel} onChange={props.setWriteScopeLevel} />
          <VmmTextField label="Category" placeholder="1" value={props.writeCategory} onChange={props.setWriteCategory} />
          <VmmTextField label="Priority" placeholder="2" value={props.writePriority} onChange={props.setWritePriority} />
          <VmmTextField label="Level" placeholder="2" value={props.writeMemoryLevel} onChange={props.setWriteMemoryLevel} />
        </div>
        <div className="vmm-action-row">
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "search_memory_events"} icon={<Search size={15} />} label="SearchMemoryEvents" onClick={() => props.onRunAction("search_memory_events", { queries: props.searchQueries, topK: props.searchTopK })} />
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "get_turn_details"} icon={<FileText size={15} />} label="GetTurnDetails" onClick={() => props.onRunAction("get_turn_details", { turnIds: props.turnIds })} />
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "write_memories"} icon={<Database size={15} />} label="WriteMemories" onClick={() => props.onRunAction("write_memories", { sessionId: props.snapshot.selectedConversationId ?? null, abstractText: props.writeAbstract, details: props.writeDetails, scopeLevel: props.writeScopeLevel, category: props.writeCategory, priority: props.writePriority, memoryLevel: props.writeMemoryLevel })} />
          <VmmActionButton disabled={props.operationBusy} danger busy={props.busyAction === "delete_memories"} icon={<Trash2 size={15} />} label="DeleteMemories" onClick={() => props.onRunAction("delete_memories", { memoryIds: props.deleteMemoryIds, reason: props.deleteReason })} />
        </div>
        <VmmToolResult
          result={props.result}
          onDeleteMemory={props.operationBusy ? undefined : (memoryId) => props.onRunAction("delete_memories", {
            memoryIds: [memoryId],
            reason: props.deleteReason || "manual memory delete from memory search result",
          })}
        />
      </section>
    );
  }
  if (props.activeSection === "memory_settings") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<SlidersHorizontal size={16} />} title={t("vmm.section.memory.title")} subtitle={t("vmm.section.memory.subtitle")} />
        {projectDraftUnavailable ? (
          <p className="vmm-muted">{props.notice}</p>
        ) : (
          <>
            <VmmSettingsForm draft={props.activeDraft} mode="memory" onChange={props.setActiveDraft} />
            <div className="vmm-action-row">
              <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === `save-${props.scope}`} icon={<Save size={15} />} label={props.scope === "global" ? t("vmm.action.saveGlobal") : t("vmm.action.saveProject")} onClick={() => props.onSaveDraft()} />
            </div>
          </>
        )}
      </section>
    );
  }
  if (props.activeSection === "grpc_transport") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<PlugZap size={16} />} title={t("vmm.section.transport.title")} subtitle={t("vmm.section.transport.subtitle")} />
        <VmmSettingsForm draft={props.globalDraft} mode="transport" onChange={(patch) => props.setGlobalDraft((draft) => ({ ...draft, ...patch }))} />
        <div className="vmm-action-row">
          <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "save-global"} icon={<Save size={15} />} label={t("vmm.action.saveGlobal")} onClick={() => props.onSaveDraft("global")} />
        </div>
      </section>
    );
  }
  return (
    <section className="vmm-section-panel">
      <VmmPanelHeading icon={<User size={16} />} title={t("vmm.section.user.title")} subtitle={t("vmm.section.user.subtitle")} />
      <div className="vmm-form-grid">
        <VmmTextField label={t("vmm.field.userRef")} placeholder={t("vmm.placeholder.userRef")} value={props.userRef} onChange={props.setUserRef} />
        <VmmTextField label={t("vmm.field.deleteConfirmationCode")} placeholder={t("vmm.placeholder.deleteConfirmationCode")} value={props.userConfirmationCode} onChange={props.setUserConfirmationCode} />
      </div>
      <div className="vmm-action-row">
        <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "resolve_user"} icon={<Search size={15} />} label={t("vmm.action.resolveUser")} onClick={() => props.onRunAction("resolve_user", { userRef: props.userRef, confirmCreate: false })} />
        <VmmActionButton disabled={props.operationBusy} busy={props.busyAction === "resolve_user"} icon={<PackagePlus size={15} />} label={t("vmm.action.confirmCreate")} onClick={() => props.onRunAction("resolve_user", { userRef: props.userRef, confirmCreate: true })} />
        <VmmActionButton disabled={props.operationBusy} danger busy={props.busyAction === "delete_user"} icon={<Trash2 size={15} />} label={t("vmm.action.deleteUser")} onClick={() => props.onRunAction("delete_user", { userRef: props.userRef, confirmationCode: props.userConfirmationCode })} />
      </div>
      <VmmUserRows result={props.result} />
    </section>
  );
}

// vmmSectionIcon maps VMM section ids to sidebar icons.
// vmmSectionIcon 将 VMM 分区标识映射到侧栏图标。
export function vmmSectionIcon(sectionId: string): React.ReactNode {
  if (sectionId === "user_manager") {
    return <User size={16} />
  }
  if (sectionId === "project_manager") {
    return <FolderTree size={16} />
  }
  if (sectionId === "profile_center") {
    return <BrainCircuit size={16} />
  }
  if (sectionId === "profile_bundle") {
    return <ScrollText size={16} />
  }
  if (sectionId === "tools_debug") {
    return <TerminalSquare size={16} />
  }
  if (sectionId === "memory_settings") {
    return <SlidersHorizontal size={16} />
  }
  if (sectionId === "grpc_transport") {
    return <PlugZap size={16} />
  }
  return <BrainCircuit size={16} />
}
