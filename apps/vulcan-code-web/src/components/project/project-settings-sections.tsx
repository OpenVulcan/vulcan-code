import { Bot, BrainCircuit, Calendar, Database, FolderCog, Save, Trash2 } from "lucide-react";
import { StatusPill } from "@/components/shared/status";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { VmmSelectField, vmmPolicyModeOptions } from "@/components/features/memory/vmm-memory-components";
import { useI18n } from "@/i18n";
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
  const { t } = useI18n();
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
          <strong>{t("projectSettings.projectSpace")}</strong>
          <p>{t("projectSettings.projectId").replace("{id}", props.project.id)}</p>
          <p>{t("projectSettings.conversationCount").replace("{count}", String(props.conversationCount))}</p>
          <p>{t("projectSettings.pinStatus").replace("{status}", props.project.pinned ? t("projectSettings.pinned") : t("projectSettings.notPinned"))}</p>
        </div>
        <div className="project-settings-panel">
          <strong>{t("projectSettings.attachmentCache")}</strong>
          <p>{props.attachmentRoot}</p>
        </div>
        <div className="project-settings-panel">
          <strong>{t("projectSettings.vmmMemory")}</strong>
          <p>{props.vmmDraft?.hasOverride ? t("projectSettings.vmmHasOverride") : t("projectSettings.vmmInheritsGlobal")}</p>
        </div>
        <div className="project-settings-panel">
          <strong>Agent Skills</strong>
          <p>{overrideCount > 0 ? t("projectSettings.agentOverrideCount").replace("{count}", String(overrideCount)) : t("projectSettings.agentInheritsGlobal")}</p>
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
  const { t } = useI18n();
  const policyOptions = vmmPolicyModeOptions(t);
  const updatePolicyMode = (policy: "precheck" | "writeback", mode: string) => {
    if (policy === "precheck") {
      props.onUpdateDraft({
        precheckMode: mode,
        precheckEnabled: mode === "inherit" ? props.vmmDraft?.precheckEnabled ?? true : mode === "enabled",
      });
      return;
    }
    props.onUpdateDraft({
      memoryWritebackMode: mode,
      memoryWritebackEnabled: mode === "inherit" ? props.vmmDraft?.memoryWritebackEnabled ?? true : mode === "enabled",
    });
  };
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <BrainCircuit size={18} />
        <div>
          <strong>{t("projectSettings.vmmTitle")}</strong>
          <span>{t("projectSettings.vmmDescription")}</span>
        </div>
      </header>
      {props.vmmBusy === "loading" || !props.vmmDraft ? (
        <p className="project-settings-muted">{t("projectSettings.vmmLoading")}</p>
      ) : (
        <>
          <label className="project-settings-toggle-row">
            <input
              checked={props.vmmDraft.hasOverride}
              onChange={(event) => props.onUpdateDraft({ hasOverride: event.target.checked })}
              type="checkbox"
            />
            <span>
              <strong>{t("projectSettings.enableProjectOverride")}</strong>
              <small>{props.vmmDraft.hasOverride ? t("projectSettings.overrideEnabledHint") : t("projectSettings.overrideDisabledHint")}</small>
            </span>
          </label>
          <div className={cn("project-settings-form-grid", !props.vmmDraft.hasOverride && "disabled")}>
            <label className="project-settings-check-row">
              <input checked={props.vmmDraft.enabled} disabled={!props.vmmDraft.hasOverride} onChange={(event) => props.onUpdateDraft({ enabled: event.target.checked })} type="checkbox" />
              {t("vmm.settings.enableMemoryConnection")}
            </label>
            <label className="project-settings-check-row">
              <input checked={props.vmmDraft.aiToolsEnabled} disabled={!props.vmmDraft.hasOverride} onChange={(event) => props.onUpdateDraft({ aiToolsEnabled: event.target.checked })} type="checkbox" />
              {t("vmm.settings.exposeAiTools")}
            </label>
            <VmmSelectField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.precheckPolicy")} options={policyOptions} value={props.vmmDraft.precheckMode} onChange={(mode) => updatePolicyMode("precheck", mode)} />
            <VmmSelectField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.writebackPolicy")} options={policyOptions} value={props.vmmDraft.memoryWritebackMode} onChange={(mode) => updatePolicyMode("writeback", mode)} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.userId")} value={props.vmmDraft.userId} onChange={(userId) => props.onUpdateDraft({ userId })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.projectId")} value={props.vmmDraft.projectId} onChange={(projectId) => props.onUpdateDraft({ projectId })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.implicitMemoryTtl")} value={props.vmmDraft.implicitMemoryTurns} onChange={(implicitMemoryTurns) => props.onUpdateDraft({ implicitMemoryTurns })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.profileRefreshInterval")} value={props.vmmDraft.profileRefreshTurns} onChange={(profileRefreshTurns) => props.onUpdateDraft({ profileRefreshTurns })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.profileNodeLimit")} value={props.vmmDraft.profileNodeLimit} onChange={(profileNodeLimit) => props.onUpdateDraft({ profileNodeLimit })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.injectionTag")} value={props.vmmDraft.injectionTag} onChange={(injectionTag) => props.onUpdateDraft({ injectionTag })} />
            <label className="project-settings-field wide">
              <span>{t("vmm.settings.injectionDescription")}</span>
              <Textarea disabled={!props.vmmDraft.hasOverride} value={props.vmmDraft.injectionDescription} onChange={(event) => props.onUpdateDraft({ injectionDescription: event.target.value })} />
            </label>
          </div>
          <p className="project-settings-muted">{t("projectSettings.vmmTransportHint")}</p>
          <div className="project-settings-actions">
            <Button disabled={props.vmmBusy === "saving"} onClick={props.onSaveDraft} type="button">
              <Save size={15} />
              {props.vmmBusy === "saving" ? t("composer.capability.save.saving") : t("projectSettings.saveVmm")}
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
  const { t } = useI18n();
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <Bot size={18} />
        <div>
          <strong>{t("projectSettings.agentTitle")}</strong>
          <span>{t("projectSettings.agentDescription")}</span>
        </div>
      </header>
      <div className="project-settings-panel compact">
        <StatusPill status={props.workbench.agentSkills.globalEnabled ? "ready" : "pending"} label={props.workbench.agentSkills.globalEnabled ? t("projectSettings.agent.globalEnabled") : t("projectSettings.agent.globalDisabled")} />
        <p>{props.workbench.agentSkills.notice}</p>
      </div>
      {props.agentLoading ? (
        <p className="project-settings-muted">{t("projectSettings.agentLoading")}</p>
      ) : props.workbench.agentSkills.skills.length === 0 ? (
        <p className="project-settings-muted">{t("projectSettings.agentEmpty")}</p>
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
                <StatusPill status={projectState === "inherit" ? "pending" : "ready"} label={projectAgentSkillStateLabel(skill.enabled, projectState, t)} />
                <div className="project-settings-segmented" role="group" aria-label={t("projectSettings.agent.projectStateAria").replace("{skill}", skill.displayName || skill.name)}>
                  {(["inherit", "enabled", "disabled"] as ProjectAgentSkillState[]).map((stateKey) => (
                    <button
                      className={cn("project-settings-segmented-button", projectState === stateKey && "active")}
                      disabled={props.agentBusySkill === skill.name}
                      key={stateKey}
                      onClick={() => props.onSetProjectAgentSkill(skill.name, stateKey)}
                      type="button"
                    >
                      {projectAgentSkillOptionLabel(stateKey, skill.enabled, t)}
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
  const { t } = useI18n();
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <Database size={18} />
        <div>
          <strong>{t("projectSettings.cleanupTitle")}</strong>
          <span>{t("projectSettings.cleanupDescription")}</span>
        </div>
      </header>
      <div className="project-settings-panel">
        <strong>{t("projectSettings.managedDirectory")}</strong>
        <p>{props.attachmentRoot}</p>
      </div>
      <div className="project-settings-cleanup-grid">
        <label className="project-settings-field">
          <span>{t("projectSettings.cutoffDate")}</span>
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
              {projectAttachmentKindLabel(kind, t)}
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
          {props.cleanupBusy === "preview" ? t("projectSettings.cleanup.previewing") : t("projectSettings.cleanup.preview")}
        </Button>
        <Button disabled={!props.cleanupPreviewMatchesCurrent || props.cleanupBusy !== "idle"} onClick={props.onExecuteCleanup} type="button">
          <Trash2 size={15} />
          {props.cleanupBusy === "execute" ? t("projectSettings.cleanup.cleaning") : t("projectSettings.cleanup.execute")}
        </Button>
      </div>
      <div className="project-settings-panel compact">
        <strong>{t("projectSettings.lastPreview")}</strong>
        <p>{props.cleanupPreview ? props.cleanupPreview.summary : props.cleanupNotice}</p>
      </div>
      <div className="project-settings-panel compact">
        <strong>{t("projectSettings.lastExecution")}</strong>
        <p>{props.cleanupReport ? props.cleanupReport.summary : t("projectSettings.cleanup.notExecuted")}</p>
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
