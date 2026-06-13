import { Bot, BrainCircuit, Calendar, Database, FolderCog, Save, Trash2 } from "lucide-react";
import { StatusPill } from "@/components/shared/status";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import type {
  ProjectSummary,
  VmmConfigDraftSummary,
  WorkbenchSnapshot,
} from "@/types";
import type {
  ProjectAgentSkillState,
  ProjectAttachmentCleanupKinds,
  ProjectAttachmentCleanupPreviewSummary,
  ProjectAttachmentCleanupReportSummary,
} from "./project-settings-types";
import {
  projectAgentSkillCurrentState,
  projectAgentSkillOptionLabel,
  projectAgentSkillStateLabel,
  projectAttachmentKindLabel,
} from "./project-settings-utils";

// ProjectOverviewSection renders static project metadata and inherited setting summaries.
// ProjectOverviewSection 渲染静态项目元信息与继承设置摘要。
export function ProjectOverviewSection(props: {
  // Project whose settings are shown.
  // 当前展示设置的项目。
  project: ProjectSummary;
  // Managed attachment root shown for the project.
  // 当前项目展示的托管附件根目录。
  attachmentRoot: string;
  // Visible conversation count under the project.
  // 当前项目下的可见会话数量。
  conversationCount: number;
  // Loaded VMM draft used to describe override state.
  // 用于描述覆盖状态的已加载 VMM 草稿。
  vmmDraft: VmmConfigDraftSummary | null;
  // Project-local Agent Skill override map.
  // 项目本地 Agent Skill 覆盖映射。
  agentOverrides: Record<string, string>;
}) {
  const overrideCount = Object.keys(props.agentOverrides).length;
  return (
    <div className="project-settings-section">
      <div className="project-settings-summary">
        <FolderCog size={22} />
        <div>
          <strong>{props.project.name}</strong>
          <span>{props.project.path}</span>
        </div>
      </div>
      <div className="project-settings-grid">
        <div className="project-settings-panel">
          <strong>项目空间</strong>
          <p>项目标识：{props.project.id}</p>
          <p>对话数量：{props.conversationCount}</p>
          <p>置顶状态：{props.project.pinned ? "已置顶" : "未置顶"}</p>
        </div>
        <div className="project-settings-panel">
          <strong>附件缓存</strong>
          <p>{props.attachmentRoot}</p>
        </div>
        <div className="project-settings-panel">
          <strong>VMM 记忆</strong>
          <p>{props.vmmDraft?.hasOverride ? "当前项目已启用独立覆盖。" : "当前项目继承全局 VMM 设置。"}</p>
        </div>
        <div className="project-settings-panel">
          <strong>Agent Skills</strong>
          <p>{overrideCount > 0 ? `当前项目有 ${overrideCount} 条覆盖。` : "未设置项目级覆盖，继承全局规则。"}</p>
        </div>
      </div>
    </div>
  );
}

// ProjectVmmSection renders project-level VMM memory override controls.
// ProjectVmmSection 渲染项目级 VMM 记忆覆盖控件。
export function ProjectVmmSection(props: {
  // Editable VMM draft loaded for the project.
  // 为当前项目加载的可编辑 VMM 草稿。
  vmmDraft: VmmConfigDraftSummary | null;
  // Current VMM load or save phase.
  // 当前 VMM 加载或保存阶段。
  vmmBusy: "idle" | "loading" | "saving";
  // Applies a partial draft edit.
  // 应用局部草稿编辑。
  onUpdateDraft: (patch: Partial<VmmConfigDraftSummary>) => void;
  // Persists the current VMM draft.
  // 持久化当前 VMM 草稿。
  onSaveDraft: () => void;
}) {
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <BrainCircuit size={18} />
        <div>
          <strong>项目级 VMM 记忆</strong>
          <span>覆盖当前项目的记忆开关、用户/项目映射与召回注入参数。</span>
        </div>
      </header>
      {props.vmmBusy === "loading" || !props.vmmDraft ? (
        <p className="project-settings-muted">正在加载项目 VMM 设置...</p>
      ) : (
        <>
          <label className="project-settings-toggle-row">
            <input
              checked={props.vmmDraft.hasOverride}
              onChange={(event) => props.onUpdateDraft({ hasOverride: event.target.checked })}
              type="checkbox"
            />
            <span>
              <strong>启用项目覆盖</strong>
              <small>{props.vmmDraft.hasOverride ? "保存后当前项目使用下方设置。" : "保存后清空项目覆盖并继承全局设置。"}</small>
            </span>
          </label>
          <div className={cn("project-settings-form-grid", !props.vmmDraft.hasOverride && "disabled")}>
            <label className="project-settings-check-row">
              <input checked={props.vmmDraft.enabled} disabled={!props.vmmDraft.hasOverride} onChange={(event) => props.onUpdateDraft({ enabled: event.target.checked })} type="checkbox" />
              启用记忆连接
            </label>
            <label className="project-settings-check-row">
              <input checked={props.vmmDraft.aiToolsEnabled} disabled={!props.vmmDraft.hasOverride} onChange={(event) => props.onUpdateDraft({ aiToolsEnabled: event.target.checked })} type="checkbox" />
              暴露 AI 工具
            </label>
            <label className="project-settings-check-row">
              <input checked={props.vmmDraft.precheckEnabled} disabled={!props.vmmDraft.hasOverride} onChange={(event) => props.onUpdateDraft({ precheckEnabled: event.target.checked })} type="checkbox" />
              自动前置召回
            </label>
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label="用户 ID" value={props.vmmDraft.userId} onChange={(userId) => props.onUpdateDraft({ userId })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label="项目 ID" value={props.vmmDraft.projectId} onChange={(projectId) => props.onUpdateDraft({ projectId })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label="临时记忆 TTL" value={props.vmmDraft.implicitMemoryTurns} onChange={(implicitMemoryTurns) => props.onUpdateDraft({ implicitMemoryTurns })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label="画像刷新间隔" value={props.vmmDraft.profileRefreshTurns} onChange={(profileRefreshTurns) => props.onUpdateDraft({ profileRefreshTurns })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label="画像节点上限" value={props.vmmDraft.profileNodeLimit} onChange={(profileNodeLimit) => props.onUpdateDraft({ profileNodeLimit })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label="注入标签" value={props.vmmDraft.injectionTag} onChange={(injectionTag) => props.onUpdateDraft({ injectionTag })} />
            <label className="project-settings-field wide">
              <span>注入说明</span>
              <Textarea disabled={!props.vmmDraft.hasOverride} value={props.vmmDraft.injectionDescription} onChange={(event) => props.onUpdateDraft({ injectionDescription: event.target.value })} />
            </label>
          </div>
          <p className="project-settings-muted">项目级设置继承全局 gRPC 连接与 API Key；连接参数请在 VMM 记忆的全局配置中调整。</p>
          <div className="project-settings-actions">
            <Button disabled={props.vmmBusy === "saving"} onClick={props.onSaveDraft} type="button">
              <Save size={15} />
              {props.vmmBusy === "saving" ? "保存中" : "保存 VMM 设置"}
            </Button>
          </div>
        </>
      )}
    </div>
  );
}

// ProjectAgentSkillsSection renders per-project Agent Skill tri-state overrides.
// ProjectAgentSkillsSection 渲染项目级 Agent Skill 三态覆盖。
export function ProjectAgentSkillsSection(props: {
  // Workbench snapshot providing the skill catalog.
  // 提供技能目录的工作台快照。
  workbench: WorkbenchSnapshot;
  // Whether project overrides are currently loading.
  // 项目覆盖是否正在加载。
  agentLoading: boolean;
  // Current project override map.
  // 当前项目覆盖映射。
  agentOverrides: Record<string, string>;
  // Skill currently being saved.
  // 当前正在保存的技能名称。
  agentBusySkill: string | null;
  // Persists one skill override.
  // 持久化单个技能覆盖。
  onSetProjectAgentSkill: (name: string, stateKey: ProjectAgentSkillState) => void;
}) {
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <Bot size={18} />
        <div>
          <strong>项目级 Agent Skills</strong>
          <span>对系统与用户提示词技能建立当前项目专属启停覆盖。</span>
        </div>
      </header>
      <div className="project-settings-panel compact">
        <StatusPill status={props.workbench.agentSkills.globalEnabled ? "ready" : "pending"} label={props.workbench.agentSkills.globalEnabled ? "全局已启用" : "全局已关闭"} />
        <p>{props.workbench.agentSkills.notice}</p>
      </div>
      {props.agentLoading ? (
        <p className="project-settings-muted">正在加载项目技能覆盖...</p>
      ) : props.workbench.agentSkills.skills.length === 0 ? (
        <p className="project-settings-muted">系统与用户目录暂未发现提示词技能。</p>
      ) : (
        <div className="project-settings-skill-list">
          {props.workbench.agentSkills.skills.map((skill) => {
            const projectState = projectAgentSkillCurrentState(props.agentOverrides[skill.name]);
            return (
              <article className="project-settings-skill-row" key={skill.name}>
                <span>
                  <strong>{skill.displayName || skill.name}</strong>
                  <small>{skill.description || skill.skillFile}</small>
                </span>
                <StatusPill status={projectState === "inherit" ? "pending" : "ready"} label={projectAgentSkillStateLabel(skill.enabled, projectState)} />
                <div className="project-settings-segmented" role="group" aria-label={`${skill.displayName || skill.name} 项目状态`}>
                  {(["inherit", "enabled", "disabled"] as ProjectAgentSkillState[]).map((stateKey) => (
                    <button
                      className={cn("project-settings-segmented-button", projectState === stateKey && "active")}
                      disabled={props.agentBusySkill === skill.name}
                      key={stateKey}
                      onClick={() => props.onSetProjectAgentSkill(skill.name, stateKey)}
                      type="button"
                    >
                      {projectAgentSkillOptionLabel(stateKey, skill.enabled)}
                    </button>
                  ))}
                </div>
              </article>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ProjectAttachmentCleanupSection renders project managed attachment cleanup controls.
// ProjectAttachmentCleanupSection 渲染项目托管附件清理控件。
export function ProjectAttachmentCleanupSection(props: {
  // Managed attachment root shown for the project.
  // 当前项目展示的托管附件根目录。
  attachmentRoot: string;
  // Selected cleanup kinds.
  // 已选择的清理类型。
  cleanupKinds: ProjectAttachmentCleanupKinds;
  // Updates selected cleanup kinds.
  // 更新已选择的清理类型。
  onChangeCleanupKinds: (kinds: ProjectAttachmentCleanupKinds) => void;
  // Cutoff date text.
  // 截止日期文本。
  cleanupCutoffDate: string;
  // Updates cutoff date text.
  // 更新截止日期文本。
  onChangeCleanupCutoffDate: (cutoffDate: string) => void;
  // Current cleanup phase.
  // 当前清理阶段。
  cleanupBusy: "idle" | "preview" | "execute";
  // Latest cleanup preview.
  // 最近一次清理预览。
  cleanupPreview: ProjectAttachmentCleanupPreviewSummary | null;
  // Latest cleanup execution report.
  // 最近一次清理执行报告。
  cleanupReport: ProjectAttachmentCleanupReportSummary | null;
  // Local cleanup status notice.
  // 本地清理状态提示。
  cleanupNotice: string;
  // Whether the latest preview matches current filters.
  // 最近一次预览是否匹配当前筛选。
  cleanupPreviewMatchesCurrent: boolean;
  // Loads cleanup preview.
  // 加载清理预览。
  onPreviewCleanup: () => void;
  // Executes cleanup.
  // 执行清理。
  onExecuteCleanup: () => void;
}) {
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <Database size={18} />
        <div>
          <strong>项目附件缓存清理</strong>
          <span>按日期和附件类型筛选当前项目的托管历史附件。</span>
        </div>
      </header>
      <div className="project-settings-panel">
        <strong>托管目录</strong>
        <p>{props.attachmentRoot}</p>
      </div>
      <div className="project-settings-cleanup-grid">
        <label className="project-settings-field">
          <span>截止日期</span>
          <Input
            value={props.cleanupCutoffDate}
            onChange={(event) => props.onChangeCleanupCutoffDate(event.target.value)}
            type="date"
          />
        </label>
        <div className="project-settings-kind-grid">
          {(["image", "audio", "video", "pdf"] as const).map((kind) => (
            <label className="project-settings-check-row" key={kind}>
              <input
                checked={props.cleanupKinds[kind]}
                onChange={(event) => props.onChangeCleanupKinds({ ...props.cleanupKinds, [kind]: event.target.checked })}
                type="checkbox"
              />
              {projectAttachmentKindLabel(kind)}
            </label>
          ))}
        </div>
      </div>
      <div className="project-settings-actions">
        <Button
          disabled={props.cleanupBusy !== "idle"}
          onClick={props.onPreviewCleanup}
          type="button"
          variant="secondary"
        >
          <Calendar size={15} />
          {props.cleanupBusy === "preview" ? "预览中" : "预览清理范围"}
        </Button>
        <Button disabled={!props.cleanupPreviewMatchesCurrent || props.cleanupBusy !== "idle"} onClick={props.onExecuteCleanup} type="button">
          <Trash2 size={15} />
          {props.cleanupBusy === "execute" ? "清理中" : "执行清理"}
        </Button>
      </div>
      <div className="project-settings-panel compact">
        <strong>最近一次预览</strong>
        <p>{props.cleanupPreview ? props.cleanupPreview.summary : props.cleanupNotice}</p>
      </div>
      <div className="project-settings-panel compact">
        <strong>最近一次执行</strong>
        <p>{props.cleanupReport ? props.cleanupReport.summary : "尚未执行历史附件缓存清理。"}</p>
      </div>
    </div>
  );
}

// ProjectSettingsField renders one labeled text input inside project settings.
// ProjectSettingsField 渲染项目设置中的单个带标签文本输入。
export function ProjectSettingsField(props: {
  // Whether the input is disabled.
  // 输入框是否禁用。
  disabled?: boolean;
  // Field label.
  // 字段标签。
  label: string;
  // Field value.
  // 字段值。
  value: string;
  // Updates the field value.
  // 更新字段值。
  onChange: (value: string) => void;
}) {
  return (
    <label className="project-settings-field">
      <span>{props.label}</span>
      <Input disabled={props.disabled} value={props.value} onChange={(event) => props.onChange(event.target.value)} />
    </label>
  );
}
