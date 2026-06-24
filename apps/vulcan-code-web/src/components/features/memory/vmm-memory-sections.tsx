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
  if (props.activeSection === "project_manager") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<FolderTree size={16} />} title="项目管理" subtitle="项目路径预检、创建确认、删除确认和迁移确认。" />
        <div className="vmm-form-grid">
          <VmmTextField label="项目路径" placeholder="Team/Space/Project" value={props.projectPath} onChange={props.setProjectPath} />
          <label className="vmm-check-row"><input checked={props.projectDeleteConfirmed} onChange={(event) => props.setProjectDeleteConfirmed(event.target.checked)} type="checkbox" />确认删除</label>
          <VmmTextField label="迁移源项目" placeholder="OldTeam/OldSpace/OldProject" value={props.migrateSource} onChange={props.setMigrateSource} />
          <VmmTextField label="迁移目标项目" placeholder="NewTeam/NewSpace/NewProject" value={props.migrateTarget} onChange={props.setMigrateTarget} />
          <label className="vmm-check-row"><input checked={props.migrateConfirmed} onChange={(event) => props.setMigrateConfirmed(event.target.checked)} type="checkbox" />确认迁移</label>
        </div>
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "ensure_project"} icon={<Search size={15} />} label="预检项目" onClick={() => props.onRunAction("ensure_project", { projectPath: props.projectPath, confirmCreate: false })} />
          <VmmActionButton busy={props.busyAction === "ensure_project"} icon={<PackagePlus size={15} />} label="确认创建" onClick={() => props.onRunAction("ensure_project", { projectPath: props.projectPath, confirmCreate: true })} />
          <VmmActionButton danger busy={props.busyAction === "delete_project"} icon={<Trash2 size={15} />} label="删除项目" onClick={() => props.onRunAction("delete_project", { projectPath: props.projectPath, confirmDelete: props.projectDeleteConfirmed })} />
          <VmmActionButton danger busy={props.busyAction === "migrate_project"} icon={<GitBranch size={15} />} label="执行迁移" onClick={() => props.onRunAction("migrate_project", { sourceProjectPath: props.migrateSource, targetProjectPath: props.migrateTarget, confirmMigrate: props.migrateConfirmed })} />
        </div>
        <VmmProjectRows result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "profile_center") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<BrainCircuit size={16} />} title="画像中心" subtitle="读取 active profile nodes，并向当前目标提交手工画像指令。" />
        <div className="vmm-form-grid">
          <label className="vmm-field">
            <span>画像目标</span>
            <select value={props.profileTarget} onChange={(event) => props.setProfileTarget(event.target.value)}>
              <option value="user">用户画像</option>
              <option value="project">项目画像</option>
              <option value="team">团队画像</option>
              <option value="space">空间画像</option>
            </select>
          </label>
          <VmmTextField label="手工画像指令" placeholder="例如：降低某条偏好权重" value={props.profileInstruction} onChange={props.setProfileInstruction} />
        </div>
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "load_profile_nodes"} icon={<RefreshCw size={15} />} label="刷新节点" onClick={() => props.onRunAction("load_profile_nodes", { target: props.profileTarget })} />
          <VmmActionButton busy={props.busyAction === "apply_profile_instruction"} icon={<Hammer size={15} />} label="应用指令" onClick={() => props.onRunAction("apply_profile_instruction", { target: props.profileTarget, instruction: props.profileInstruction })} />
        </div>
        <VmmProfileRows result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "profile_bundle") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<ScrollText size={16} />} title="画像 Bundle 测试" subtitle="读取当前 user/project 的 FULL bundle，检查画像组合结果。" />
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "load_profile_bundle"} icon={<RefreshCw size={15} />} label="读取完整画像" onClick={() => props.onRunAction("load_profile_bundle")} />
        </div>
        <VmmBundlePreview result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "tools_debug") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<TerminalSquare size={16} />} title="工具调试" subtitle="模拟 VMM AI 专属工具：检索记忆、展开 turn、主动写入记忆。" />
        <div className="vmm-form-grid">
          <VmmTextField label="检索查询" placeholder="多条用 | 分隔" value={props.searchQueries} onChange={props.setSearchQueries} />
          <VmmTextField label="Top K" placeholder="5" value={props.searchTopK} onChange={props.setSearchTopK} />
          <VmmTextField label="Turn IDs" placeholder="用逗号或 | 分隔" value={props.turnIds} onChange={props.setTurnIds} />
          <VmmTextField label="删除 Memory IDs" placeholder="用逗号或 | 分隔" value={props.deleteMemoryIds} onChange={props.setDeleteMemoryIds} />
          <VmmTextField label="删除原因" placeholder="错误、过期或用户要求删除" value={props.deleteReason} onChange={props.setDeleteReason} />
          <VmmTextField label="主动写入摘要" placeholder="记忆摘要" value={props.writeAbstract} onChange={props.setWriteAbstract} />
          <VmmTextField label="主动写入详情" placeholder="完整内容" value={props.writeDetails} onChange={props.setWriteDetails} />
          <VmmTextField label="Scope" placeholder="2" value={props.writeScopeLevel} onChange={props.setWriteScopeLevel} />
          <VmmTextField label="Category" placeholder="1" value={props.writeCategory} onChange={props.setWriteCategory} />
          <VmmTextField label="Priority" placeholder="2" value={props.writePriority} onChange={props.setWritePriority} />
          <VmmTextField label="Level" placeholder="2" value={props.writeMemoryLevel} onChange={props.setWriteMemoryLevel} />
        </div>
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "search_memory_events"} icon={<Search size={15} />} label="SearchMemoryEvents" onClick={() => props.onRunAction("search_memory_events", { queries: props.searchQueries, topK: props.searchTopK })} />
          <VmmActionButton busy={props.busyAction === "get_turn_details"} icon={<FileText size={15} />} label="GetTurnDetails" onClick={() => props.onRunAction("get_turn_details", { turnIds: props.turnIds })} />
          <VmmActionButton busy={props.busyAction === "write_memories"} icon={<Database size={15} />} label="WriteMemories" onClick={() => props.onRunAction("write_memories", { sessionId: props.snapshot.selectedConversationId ?? null, abstractText: props.writeAbstract, details: props.writeDetails, scopeLevel: props.writeScopeLevel, category: props.writeCategory, priority: props.writePriority, memoryLevel: props.writeMemoryLevel })} />
          <VmmActionButton danger busy={props.busyAction === "delete_memories"} icon={<Trash2 size={15} />} label="DeleteMemories" onClick={() => props.onRunAction("delete_memories", { memoryIds: props.deleteMemoryIds, reason: props.deleteReason })} />
        </div>
        <VmmToolResult
          result={props.result}
          onDeleteMemory={(memoryId) => props.onRunAction("delete_memories", {
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
        <VmmPanelHeading icon={<SlidersHorizontal size={16} />} title="记忆设置" subtitle="配置记忆开关、AI 工具、临时 TTL、画像刷新和注入说明。" />
        <VmmSettingsForm draft={props.activeDraft} mode="memory" onChange={props.setActiveDraft} />
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === `save-${props.scope}`} icon={<Save size={15} />} label={props.scope === "global" ? "保存到全局" : "保存到项目"} onClick={() => props.onSaveDraft()} />
        </div>
      </section>
    );
  }
  if (props.activeSection === "grpc_transport") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<PlugZap size={16} />} title="gRPC 传输" subtitle="连接地址、API key、超时、最大接收字节数与 keepalive 策略。" />
        <VmmSettingsForm draft={props.globalDraft} mode="transport" onChange={(patch) => props.setGlobalDraft((draft) => ({ ...draft, ...patch }))} />
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "save-global"} icon={<Save size={15} />} label="保存到全局" onClick={() => props.onSaveDraft("global")} />
        </div>
      </section>
    );
  }
  return (
    <section className="vmm-section-panel">
      <VmmPanelHeading icon={<User size={16} />} title="用户管理" subtitle="长期用户列表、解析用户引用、创建确认和删除确认。" />
      <div className="vmm-form-grid">
        <VmmTextField label="用户引用" placeholder="ID 或用户名" value={props.userRef} onChange={props.setUserRef} />
        <VmmTextField label="删除确认码" placeholder="首次删除后返回" value={props.userConfirmationCode} onChange={props.setUserConfirmationCode} />
      </div>
      <div className="vmm-action-row">
        <VmmActionButton busy={props.busyAction === "resolve_user"} icon={<Search size={15} />} label="解析用户" onClick={() => props.onRunAction("resolve_user", { userRef: props.userRef, confirmCreate: false })} />
        <VmmActionButton busy={props.busyAction === "resolve_user"} icon={<PackagePlus size={15} />} label="确认创建" onClick={() => props.onRunAction("resolve_user", { userRef: props.userRef, confirmCreate: true })} />
        <VmmActionButton danger busy={props.busyAction === "delete_user"} icon={<Trash2 size={15} />} label="删除用户" onClick={() => props.onRunAction("delete_user", { userRef: props.userRef, confirmationCode: props.userConfirmationCode })} />
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
