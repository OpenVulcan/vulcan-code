import { Bot, BrainCircuit, Calendar, Database, FolderCog, LoaderCircle, Network, RefreshCw, Save, Trash2 } from "lucide-react";
import { StatusPill } from "@/components/shared/status";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { VmmSelectField, vmmPolicyModeOptions } from "@/components/features/memory/vmm-project-settings-fields";
import { agentSkillsCatalogSummary } from "@/components/features/agent-skills/agent-skills-types";
import { useI18n } from "@/i18n";
import type { TranslationKey } from "@/i18n-messages";
import { cn } from "@/lib/utils";
import type {
  ProjectSummary,
  CodebaseMemoryIndexJobSummary,
  CodebaseMemoryProjectSettingsSummary,
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

// codebaseMemoryProgressPercent calculates a bounded determinate percentage without losing integer identity.
// codebaseMemoryProgressPercent 在不丢失整数身份的情况下计算有界确定进度百分比。
function codebaseMemoryProgressPercent(job: CodebaseMemoryIndexJobSummary): number | null {
  if (!job.totalUnits) {
    return null;
  }
  try {
    const completed = BigInt(job.completedUnits);
    const total = BigInt(job.totalUnits);
    if (total <= 0n) {
      return null;
    }
    const basisPoints = completed * 10_000n / total;
    return Math.min(100, Number(basisPoints) / 100);
  } catch {
    return null;
  }
}

// codebaseMemoryAvailabilityTone maps published availability to a shared status tone.
// codebaseMemoryAvailabilityTone 将已发布可用状态映射到共享状态色调。
function codebaseMemoryAvailabilityTone(availability: string | null | undefined): string {
  if (availability === "ready") {
    return "ready";
  }
  if (availability === "degraded") {
    return "warning";
  }
  if (availability === "unavailable") {
    return "error";
  }
  return "pending";
}

// CODEBASE_MEMORY_AVAILABILITY_KEYS binds exact protocol values to localized copy.
// CODEBASE_MEMORY_AVAILABILITY_KEYS 将精确协议值绑定到本地化文案。
const CODEBASE_MEMORY_AVAILABILITY_KEYS: Record<string, TranslationKey> = {
  ready: "projectSettings.codebaseMemory.availability.ready",
  degraded: "projectSettings.codebaseMemory.availability.degraded",
  unavailable: "projectSettings.codebaseMemory.availability.unavailable",
  offline: "projectSettings.codebaseMemory.availability.offline",
  unknown: "projectSettings.codebaseMemory.availability.unknown",
};

// CODEBASE_MEMORY_JOB_MODE_KEYS binds exact managed modes to localized copy.
// CODEBASE_MEMORY_JOB_MODE_KEYS 将精确托管模式绑定到本地化文案。
const CODEBASE_MEMORY_JOB_MODE_KEYS: Record<string, TranslationKey> = {
  update: "projectSettings.codebaseMemory.job.mode.update",
  rebuild: "projectSettings.codebaseMemory.job.mode.rebuild",
};

// CODEBASE_MEMORY_JOB_PHASE_KEYS binds exact managed phases to localized copy.
// CODEBASE_MEMORY_JOB_PHASE_KEYS 将精确托管阶段绑定到本地化文案。
const CODEBASE_MEMORY_JOB_PHASE_KEYS: Record<string, TranslationKey> = {
  queued: "projectSettings.codebaseMemory.job.phase.queued",
  discovering: "projectSettings.codebaseMemory.job.phase.discovering",
  extracting: "projectSettings.codebaseMemory.job.phase.extracting",
  resolving: "projectSettings.codebaseMemory.job.phase.resolving",
  persisting: "projectSettings.codebaseMemory.job.phase.persisting",
  publishing: "projectSettings.codebaseMemory.job.phase.publishing",
  finalizing: "projectSettings.codebaseMemory.job.phase.finalizing",
};

// CODEBASE_MEMORY_JOB_STATE_KEYS binds exact managed terminal states to localized copy.
// CODEBASE_MEMORY_JOB_STATE_KEYS 将精确托管终态绑定到本地化文案。
const CODEBASE_MEMORY_JOB_STATE_KEYS: Record<string, TranslationKey> = {
  queued: "projectSettings.codebaseMemory.job.state.queued",
  running: "projectSettings.codebaseMemory.job.state.running",
  succeeded: "projectSettings.codebaseMemory.job.state.succeeded",
  failed: "projectSettings.codebaseMemory.job.state.failed",
  cancelled: "projectSettings.codebaseMemory.job.state.cancelled",
};

// ProjectCodebaseMemorySection renders project-owned managed MCP enablement and diagnostics.
// ProjectCodebaseMemorySection 渲染项目自有的托管 MCP 启用状态与诊断。
export function ProjectCodebaseMemorySection(props: {
  // Current authoritative project settings, or null while loading.
  // 当前权威项目设置；加载期间为 null。
  settings: CodebaseMemoryProjectSettingsSummary | null;
  // Whether a load, save, or reindex request is running.
  // 是否正在执行加载、保存或重建请求。
  busy: boolean;
  // Persists the selected project enablement state.
  // 持久化所选项目启用状态。
  onSetEnabled: (enabled: boolean) => void;
  // Refreshes one authoritative live Sidecar snapshot.
  // 刷新一份权威的实时 Sidecar 快照。
  onRefresh: () => void;
  // Requests the normal incremental index update.
  // 请求常规增量索引更新。
  onUpdate: () => void;
  // Requests an isolated full index rebuild after confirmation.
  // 在确认后请求隔离的完整索引重建。
  onRebuild: () => void;
}) {
  const { t } = useI18n();
  if (!props.settings) {
    return <p className="project-settings-muted">{t("projectSettings.codebaseMemory.loading")}</p>;
  }
  const activeJob = props.settings.activeJob ?? null;
  const lastJob = props.settings.lastJob ?? null;
  const progressPercent = activeJob ? codebaseMemoryProgressPercent(activeJob) : null;
  const availability = props.settings.availability ?? "unknown";
  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <Network size={18} />
        <div>
          <strong>{t("projectSettings.codebaseMemory.title")}</strong>
          <span>{t("projectSettings.codebaseMemory.description")}</span>
        </div>
        <Button
          aria-label={t("projectSettings.codebaseMemory.refresh")}
          disabled={props.busy || !props.settings.enabled}
          onClick={props.onRefresh}
          size="icon-sm"
          type="button"
          variant="outline"
        >
          <RefreshCw className={cn(props.busy && "animate-spin")} size={15} />
        </Button>
      </header>
      <div className="project-settings-toggle-row">
        <Switch
          aria-label={t("projectSettings.codebaseMemory.enable")}
          checked={props.settings.enabled}
          disabled={props.busy}
          onCheckedChange={props.onSetEnabled}
        />
        <span>
          <strong>{t("projectSettings.codebaseMemory.enable")}</strong>
          <small>{props.settings.enabled
            ? t("projectSettings.codebaseMemory.enableHint")
            : t("projectSettings.codebaseMemory.disableHint")}</small>
        </span>
      </div>
      <div className="project-settings-grid">
        <div className="project-settings-panel">
          <strong>{t("projectSettings.codebaseMemory.serviceStatus")}</strong>
          <p>{props.settings.serviceStatus}</p>
        </div>
        <div className="project-settings-panel">
          <strong>{t("projectSettings.codebaseMemory.availability")}</strong>
          <StatusPill
            label={t(CODEBASE_MEMORY_AVAILABILITY_KEYS[availability] ?? CODEBASE_MEMORY_AVAILABILITY_KEYS.unknown)}
            status={codebaseMemoryAvailabilityTone(props.settings.availability)}
          />
        </div>
        <div className="project-settings-panel">
          <strong>{t("projectSettings.codebaseMemory.lastIndexed")}</strong>
          <p>{props.settings.lastSuccessfulIndexAt ?? t("projectSettings.codebaseMemory.unavailable")}</p>
        </div>
        <div className="project-settings-panel">
          <strong>{t("projectSettings.codebaseMemory.revision")}</strong>
          <p>{props.settings.indexRevision ?? t("projectSettings.codebaseMemory.unavailable")}</p>
        </div>
        <div className="project-settings-panel">
          <strong>{t("projectSettings.codebaseMemory.watcher")}</strong>
          <p>{props.settings.watcherRegistered
            ? t("projectSettings.codebaseMemory.registered")
            : t("projectSettings.codebaseMemory.notRegistered")}</p>
        </div>
        <div className="project-settings-panel">
          <strong>{t("projectSettings.codebaseMemory.graphSize")}</strong>
          <p>{t("projectSettings.codebaseMemory.graphSizeValue")
            .replace("{nodes}", props.settings.nodeCount ?? "—")
            .replace("{edges}", props.settings.edgeCount ?? "—")}</p>
        </div>
      </div>
      {activeJob ? (
        <section className="rounded-lg border bg-muted/20 p-3" aria-live="polite">
          <div className="flex items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-2">
              <LoaderCircle className="shrink-0 animate-spin" size={16} />
              <strong>{t(CODEBASE_MEMORY_JOB_MODE_KEYS[activeJob.mode])}</strong>
              <span className="truncate text-sm text-muted-foreground">
                {t(CODEBASE_MEMORY_JOB_PHASE_KEYS[activeJob.phase])}
              </span>
            </div>
            <span className="shrink-0 text-xs text-muted-foreground">
              {activeJob.totalUnits
                ? `${activeJob.completedUnits} / ${activeJob.totalUnits} ${activeJob.unit}`
                : `${activeJob.completedUnits} ${activeJob.unit}`}
            </span>
          </div>
          {progressPercent === null ? (
            <div className="mt-3 h-1 overflow-hidden rounded-full bg-muted">
              <div className="h-full w-1/3 animate-pulse rounded-full bg-primary" />
            </div>
          ) : (
            <Progress className="mt-3" value={progressPercent} />
          )}
          <p className="mt-2 text-xs text-muted-foreground">
            {t("projectSettings.codebaseMemory.job.availableDuringUpdate")}
          </p>
        </section>
      ) : lastJob ? (
        <p className="project-settings-muted">
          {t("projectSettings.codebaseMemory.lastJob")}: {t(CODEBASE_MEMORY_JOB_MODE_KEYS[lastJob.mode])} · {t(CODEBASE_MEMORY_JOB_STATE_KEYS[lastJob.state])}
          {lastJob.completedAt ? ` · ${lastJob.completedAt}` : ""}
        </p>
      ) : null}
      {props.settings.lastError ? (
        <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
          {props.settings.lastError}
        </p>
      ) : null}
      <div className="project-settings-actions">
        <Button
          disabled={props.busy || !props.settings.enabled || Boolean(activeJob)}
          onClick={props.onUpdate}
          type="button"
        >
          <RefreshCw size={15} />
          {t("projectSettings.codebaseMemory.update")}
        </Button>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button
              disabled={props.busy || !props.settings.enabled || Boolean(activeJob)}
              type="button"
              variant="outline"
            >
              {t("projectSettings.codebaseMemory.rebuild")}
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("projectSettings.codebaseMemory.rebuildTitle")}</AlertDialogTitle>
              <AlertDialogDescription>
                {t("projectSettings.codebaseMemory.rebuildDescription")}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t("projectSettings.codebaseMemory.cancel")}</AlertDialogCancel>
              <AlertDialogAction onClick={props.onRebuild}>
                {t("projectSettings.codebaseMemory.confirmRebuild")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </div>
  );
}

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
          <strong>{t("projectSettings.agentTitle")}</strong>
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
        <StatusPill status="ready" label={`${props.workbench.agentSkills.records.filter((record) => record.effective && record.enabled).length} ${t("projectSettings.agentTitle")}`} />
        <p>{agentSkillsCatalogSummary(props.workbench.agentSkills, t)}</p>
      </div>
      {props.agentLoading ? (
        <p className="project-settings-muted">{t("projectSettings.agentLoading")}</p>
        ) : props.workbench.agentSkills.records.filter((skill) => skill.effective).length === 0 ? (
        <p className="project-settings-muted">{t("projectSettings.agentEmpty")}</p>
      ) : (
        <div className="project-settings-skill-list">
            {props.workbench.agentSkills.records.filter((skill) => skill.effective).map((skill) => {
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
                      disabled={props.agentBusySkill !== null}
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
            disabled={props.cleanupBusy !== "idle"}
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
                disabled={props.cleanupBusy !== "idle"}
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
