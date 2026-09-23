import * as React from "react";
import { Bot, BrainCircuit, Calendar, Database, FolderPlus, LoaderCircle, Save, Trash2 } from "lucide-react";
import { StatusPill } from "@/components/shared/status";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DisclosureSection } from "@/components/ui/disclosure";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import "@/styles/project-vmm.css";
import { VmmSelectField, vmmPolicyModeOptions } from "@/components/features/memory/vmm-project-settings-fields";
import { canonicalProjectPath } from "@/components/features/memory/vmm-identity-project-page";
import { VmmNativeHostProjectPolicy } from "@/components/features/memory/vmm-native-host-project-policy";
import { agentSkillsCatalogSummary } from "@/components/features/agent-skills/agent-skills-types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import { runVmmMemoryAction } from "@/service/memory-actions-service";
import type {
  VmmConfigDraftSummary,
  VmmNativeProjectCatalogSummary,
  VmmMemoryActionResult,
  VmmProtocolProjectEntry,
  VmmProtocolUserEntry,
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

// Preserve the public section import while the overview owns its independent layout.
// 保留公开分区导入，同时由概览持有独立布局。
export { ProjectOverviewSection } from "./project-overview-section";

// ProjectVmmSection renders project-level VMM memory override controls.
// ProjectVmmSection 渲染项目级 VMM 记忆覆盖控件。
export function ProjectVmmSection(props: {
  // Stable VulcanCode project id whose binding is being edited.
  // 当前编辑绑定的稳定 VulcanCode 项目标识。
  projectId: string;
  // Editable VMM draft loaded for the project.
  // 为当前项目加载的可编辑 VMM 草稿。
  vmmDraft: VmmConfigDraftSummary | null;
  // Current VMM load or save phase.
  // 当前 VMM 加载或保存阶段。
  vmmBusy: "idle" | "loading" | "saving";
  // Unsaved project edits determine whether the general configuration needs persistence.
  // 尚未保存的项目编辑决定一般配置是否需要持久化。
  vmmDirty: boolean;
  // Applies a partial draft edit.
  // 应用局部草稿编辑。
  onUpdateDraft: (patch: Partial<VmmConfigDraftSummary>) => void;
  // Persists the current or supplied exact VMM draft and reports whether it committed.
  // 持久化当前或给定的准确 VMM 草稿，并报告是否提交成功。
  onSaveDraft: (draft?: VmmConfigDraftSummary) => Promise<boolean>;
}) {
  const { t } = useI18n();
  const policyOptions = vmmPolicyModeOptions(t);
  const [nativeCatalog, setNativeCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null);
  const [nativeOwnerId, setNativeOwnerId] = React.useState("");
  const [nativeLinkedIds, setNativeLinkedIds] = React.useState<number[]>([]);
  const [nativeBindingBusy, setNativeBindingBusy] = React.useState(false);
  // Bind/create share one synchronous guard, including repeated clicks within one React batch.
  // 绑定与创建共用同步锁，包括同一 React 批次内的重复点击。
  const nativeBindingBusyRef = React.useRef(false);
  const [nativeBindingNotice, setNativeBindingNotice] = React.useState("");
  // Catalog read failures remain distinct from an empty project list and can be retried explicitly.
  // 目录读取失败与空项目列表保持区分，并允许明确重试。
  const [catalogError, setCatalogError] = React.useState("");
  const [catalogLoading, setCatalogLoading] = React.useState(false);
  const [catalogAttempt, setCatalogAttempt] = React.useState(0);
  // This transport accepts only caller-policy actions; the child supplies the exact host tag in its typed payload.
  // 此传输仅接受调用方策略动作；子组件在强类型载荷中提供准确宿主标签。
  const runNativePolicyAction = React.useCallback((action: string, payload?: Record<string, unknown>): Promise<VmmMemoryActionResult> =>
    runVmmMemoryAction({ action, payload, commandId: crypto.randomUUID(), scope: "global", projectId: null }), []);
  const [externalUsers, setExternalUsers] = React.useState<VmmProtocolUserEntry[]>([]);
  const [externalProjects, setExternalProjects] = React.useState<VmmProtocolProjectEntry[]>([]);
  // createProjectName is the editable memory-project display name or external leaf name.
  // createProjectName 是可编辑的记忆项目显示名或外置项目末级名称。
  const [createProjectName, setCreateProjectName] = React.useState(props.projectId);
  // createExternalTeamName is the explicit external Team segment used only by manual creation.
  // createExternalTeamName 是仅供手动创建使用的显式外置 Team 路径段。
  const [createExternalTeamName, setCreateExternalTeamName] = React.useState("vulcan");
  // createExternalSpaceName is the explicit external Space segment used only by manual creation.
  // createExternalSpaceName 是仅供手动创建使用的显式外置 Space 路径段。
  const [createExternalSpaceName, setCreateExternalSpaceName] = React.useState("space");

  React.useEffect(() => {
    if (!props.vmmDraft) return;
    const prefixSegments = props.vmmDraft.externalProjectPrefix
      .split("/")
      .map((segment) => segment.trim())
      .filter(Boolean);
    setCreateProjectName(props.projectId);
    setCreateExternalTeamName(prefixSegments[0] ?? "vulcan");
    setCreateExternalSpaceName(prefixSegments[1] ?? "space");
  }, [props.projectId, props.vmmDraft?.backendMode, props.vmmDraft?.externalProjectPrefix]);

  // acceptNativeCatalog synchronizes the owner and recall drafts from one authoritative response.
  // acceptNativeCatalog 根据一份权威响应同步归属与召回草稿。
  const acceptNativeCatalog = React.useCallback((catalog: VmmNativeProjectCatalogSummary) => {
    setNativeCatalog(catalog);
    const binding = catalog.bindings.find((item) => item.vulcanProjectId === props.projectId);
    setNativeOwnerId(binding ? String(binding.ownerProjectId) : "");
    setNativeLinkedIds(binding?.linkedProjectIds ?? []);
  }, [props.projectId]);

  React.useEffect(() => {
    if (!props.vmmDraft) {
      setNativeCatalog(null);
      return;
    }
    let cancelled = false;
    setCatalogLoading(true);
    setCatalogError("");
    setNativeCatalog(null);
    setExternalUsers([]);
    setExternalProjects([]);
    void runVmmMemoryAction({
      action: props.vmmDraft.backendMode === "native" ? "native_project_catalog" : "load_users_projects",
      commandId: crypto.randomUUID(),
      scope: "global",
      projectId: null,
    }).then((result) => {
      if (cancelled) return;
      if (result.data.kind === "native_project_catalog") {
        acceptNativeCatalog(result.data.value);
      } else if (result.data.kind === "users_projects") {
        setExternalUsers(result.data.value.users);
        setExternalProjects(result.data.value.projects);
      } else {
        throw new Error("Memory catalog returned an unexpected result kind.");
      }
    }).catch((error) => {
      if (!cancelled) setCatalogError(error instanceof Error ? error.message : String(error));
    }).finally(() => {
      if (!cancelled) setCatalogLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [acceptNativeCatalog, props.projectId, props.vmmDraft?.backendMode, catalogAttempt]);

  // saveNativeBinding persists one owner and the separately selected recall-only projects.
  // saveNativeBinding 持久化一个归属项目与单独选择的仅召回项目。
  const saveNativeBinding = async () => {
    if (nativeBindingBusyRef.current || !nativeCatalog) return;
    nativeBindingBusyRef.current = true;
    setNativeBindingBusy(true);
    setNativeBindingNotice("");
    try {
      const ownerProjectId = Number(nativeOwnerId);
      if (!ownerProjectId) {
        const result = await runVmmMemoryAction({
          action: "native_project_unbind",
          commandId: crypto.randomUUID(),
          scope: "global",
          projectId: null,
          payload: { vulcanProjectId: props.projectId },
        });
        if (result.data.kind === "native_project_catalog") acceptNativeCatalog(result.data.value);
        setNativeBindingNotice(result.summary);
        return;
      }
      const result = await runVmmMemoryAction({
        action: "native_project_set_access",
        commandId: crypto.randomUUID(),
        scope: "global",
        projectId: null,
        payload: {
          vulcanProjectId: props.projectId,
          ownerProjectId,
          linkedProjectIds: nativeLinkedIds,
        },
      });
      if (result.data.kind === "native_project_catalog") acceptNativeCatalog(result.data.value);
      setNativeBindingNotice(result.summary);
    } catch (error) {
      setNativeBindingNotice(error instanceof Error ? error.message : String(error));
    } finally {
      nativeBindingBusyRef.current = false;
      setNativeBindingBusy(false);
    }
  };

  // createAndBindProject creates one backend-specific project and durably binds this project to its stable id.
  // createAndBindProject 创建一个后端专属项目，并将当前项目耐久绑定到其稳定标识。
  const createAndBindProject = async () => {
    if (nativeBindingBusyRef.current || catalogLoading || catalogError || !props.vmmDraft || !createProjectName.trim()) return;
    nativeBindingBusyRef.current = true;
    setNativeBindingBusy(true);
    setNativeBindingNotice("");
    try {
      if (props.vmmDraft.backendMode === "native") {
        const result = await runVmmMemoryAction({
          action: "native_project_create_and_bind",
          commandId: crypto.randomUUID(),
          scope: "global",
          projectId: null,
          payload: {
            vulcanProjectId: props.projectId,
            projectName: createProjectName.trim(),
          },
        });
        if (result.data.kind === "native_project_catalog") acceptNativeCatalog(result.data.value);
        setNativeBindingNotice(result.summary);
        return;
      }
      const projectPath = canonicalProjectPath(
        createExternalTeamName,
        createExternalSpaceName,
        createProjectName,
      );
      const result = await runVmmMemoryAction({
        action: "ensure_project",
        commandId: crypto.randomUUID(),
        scope: "global",
        projectId: null,
        payload: { projectPath, confirmCreate: true },
      });
      if (result.data.kind !== "ensure_project" || !result.data.value.project) {
        throw new Error(result.summary);
      }
      const remoteProjectId = String(result.data.value.project.project_id);
      const boundDraft = externalProjectBindingDraft(props.vmmDraft, remoteProjectId);
      if (!await props.onSaveDraft(boundDraft)) return;
      const catalogResult = await runVmmMemoryAction({
        action: "load_users_projects",
        commandId: crypto.randomUUID(),
        scope: "global",
        projectId: null,
      });
      if (catalogResult.data.kind === "users_projects") {
        setExternalUsers(catalogResult.data.value.users);
        setExternalProjects(catalogResult.data.value.projects);
      }
      setNativeBindingNotice(result.summary);
    } catch (error) {
      setNativeBindingNotice(error instanceof Error ? error.message : String(error));
    } finally {
      nativeBindingBusyRef.current = false;
      setNativeBindingBusy(false);
    }
  };
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
    <div className="project-settings-section project-vmm-section">
      <header className="project-settings-heading">
        <BrainCircuit size={18} />
        <div>
          <strong>{t("projectSettings.vmmTitle")}</strong>
          <span>{t(props.vmmDraft?.backendMode === "native"
            ? "projectSettings.vmmNativeDescription"
            : "projectSettings.vmmDescription")}</span>
        </div>
      </header>
      {props.vmmBusy === "loading" || !props.vmmDraft ? (
        <p className="project-settings-muted">{t("projectSettings.vmmLoading")}</p>
      ) : (
        <>
          <label className="project-settings-toggle-row">
            <Switch
              checked={props.vmmDraft.hasOverride}
              disabled={props.vmmBusy === "saving"}
              onCheckedChange={(hasOverride) => props.onUpdateDraft({ hasOverride })}
            />
            <span>
              <strong>{t(props.vmmDraft.backendMode === "native" ? "memory.hostPolicy.connection" : "projectSettings.enableProjectOverride")}</strong>
              <small>{props.vmmDraft.hasOverride ? t("projectSettings.overrideEnabledHint") : t("projectSettings.overrideDisabledHint")}</small>
            </span>
          </label>
          <fieldset className="project-settings-form-grid project-vmm-form" disabled={!props.vmmDraft.hasOverride || props.vmmBusy === "saving"}>
            <label className="project-settings-check-row">
              <span>{t(props.vmmDraft.backendMode === "native" ? "projectSettings.vmmNativeEnabled" : "vmm.settings.enableMemoryConnection")}</span>
              <Switch checked={props.vmmDraft.enabled} disabled={!props.vmmDraft.hasOverride || props.vmmBusy === "saving"} onCheckedChange={(enabled) => props.onUpdateDraft({ enabled })} />
            </label>
            <label className="project-settings-check-row">
              <span>{t("vmm.settings.exposeAiTools")}</span>
              <Switch checked={props.vmmDraft.aiToolsEnabled} disabled={!props.vmmDraft.hasOverride || props.vmmBusy === "saving"} onCheckedChange={(aiToolsEnabled) => props.onUpdateDraft({ aiToolsEnabled })} />
            </label>
            <VmmSelectField disabled={!props.vmmDraft.hasOverride || props.vmmBusy === "saving"} label={t("vmm.settings.precheckPolicy")} options={policyOptions} value={props.vmmDraft.precheckMode} onChange={(mode) => updatePolicyMode("precheck", mode)} />
            <VmmSelectField disabled={!props.vmmDraft.hasOverride || props.vmmBusy === "saving"} label={t("vmm.settings.writebackPolicy")} options={policyOptions} value={props.vmmDraft.memoryWritebackMode} onChange={(mode) => updatePolicyMode("writeback", mode)} />
            {props.vmmDraft.backendMode === "external_grpc" ? (
              <>
                <VmmSelectField
                  disabled={!props.vmmDraft.hasOverride}
                  label={t("vmm.settings.userId")}
                  onChange={(userId) => props.onUpdateDraft({ userId })}
                  options={externalUsers.map((user) => ({ value: String(user.user_id), label: user.user_name }))}
                  value={props.vmmDraft.userId}
                />
                <VmmSelectField
                  disabled={!props.vmmDraft.hasOverride}
                  label={t("vmm.settings.projectId")}
                  onChange={(projectId) => props.onUpdateDraft({ projectId })}
                  options={externalProjects.map((project) => ({ value: String(project.project_id), label: project.display_path }))}
                  value={props.vmmDraft.projectId}
                />
              </>
            ) : null}
            <DisclosureSection className="project-vmm-advanced" description={t("projectSettings.vmmAdvancedHint")} title={t("projectSettings.vmmAdvanced")}>
              <div className="project-settings-form-grid project-vmm-form">
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.implicitMemoryTtl")} value={props.vmmDraft.implicitMemoryTurns} onChange={(implicitMemoryTurns) => props.onUpdateDraft({ implicitMemoryTurns })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.profileRefreshInterval")} value={props.vmmDraft.profileRefreshTurns} onChange={(profileRefreshTurns) => props.onUpdateDraft({ profileRefreshTurns })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.profileNodeLimit")} value={props.vmmDraft.profileNodeLimit} onChange={(profileNodeLimit) => props.onUpdateDraft({ profileNodeLimit })} />
            <ProjectSettingsField disabled={!props.vmmDraft.hasOverride} label={t("vmm.settings.injectionTag")} value={props.vmmDraft.injectionTag} onChange={(injectionTag) => props.onUpdateDraft({ injectionTag })} />
            <label className="project-settings-field wide">
              <span>{t("vmm.settings.injectionDescription")}</span>
              <Textarea disabled={!props.vmmDraft.hasOverride} value={props.vmmDraft.injectionDescription} onChange={(event) => props.onUpdateDraft({ injectionDescription: event.target.value })} />
            </label>
              </div>
            </DisclosureSection>
          </fieldset>
          <div className="project-settings-actions">
            <Button disabled={!props.vmmDirty || props.vmmBusy === "saving"} onClick={() => void props.onSaveDraft()} type="button">
              <Save size={15} />
              {props.vmmBusy === "saving" ? t("composer.capability.save.saving") : t("projectSettings.saveVmm")}
            </Button>
          </div>
          {props.vmmDraft.backendMode === "native" ? (
            <div className="project-settings-panel">
              <strong>{t("projectSettings.vmmNativeBinding")}</strong>
              <VmmSelectField
                disabled={nativeBindingBusy || !nativeCatalog}
                label={t("projectSettings.vmmNativeOwner")}
                onChange={(value) => {
                  setNativeOwnerId(value === "unbound" ? "" : value);
                  if (value === "unbound") setNativeLinkedIds([]);
                  else setNativeLinkedIds((current) => current.filter((projectId) => String(projectId) !== value));
                }}
                options={[
                  { value: "unbound", label: t("vmm.nativeProjects.unbound") },
                  ...(nativeCatalog?.projects ?? []).map((project) => ({ value: String(project.projectId), label: project.name })),
                ]}
                value={nativeOwnerId || "unbound"}
              />
              {nativeOwnerId ? (
                <div className="project-vmm-linked">
                  <span className="project-vmm-linked-label">{t("projectSettings.vmmNativeLinked")}</span>
                  <div className="project-vmm-linked-list">
                  {(nativeCatalog?.projects ?? []).filter((project) => String(project.projectId) !== nativeOwnerId).map((project) => (
                    <label className="project-settings-check-row" key={project.projectId}>
                      <Checkbox
                        disabled={nativeBindingBusy}
                        checked={nativeLinkedIds.includes(project.projectId)}
                        onCheckedChange={() => setNativeLinkedIds((current) => current.includes(project.projectId)
                          ? current.filter((value) => value !== project.projectId)
                          : [...current, project.projectId])}
                      />
                      <span className="truncate" title={project.name}>{project.name}</span>
                    </label>
                  ))}
                  </div>
                </div>
              ) : null}
              <Button disabled={nativeBindingBusy || !nativeCatalog} onClick={() => void saveNativeBinding()} type="button" variant="outline">
                {nativeBindingBusy ? <LoaderCircle className="size-4 animate-spin" /> : <Save size={15} />}
                {t("projectSettings.vmmNativeSaveBinding")}
              </Button>
            </div>
          ) : null}
          {catalogLoading ? <p role="status" className="project-settings-muted">{t("memory.common.loading")}</p> : null}
          {catalogError ? <div role="alert" className="project-vmm-catalog-error">
            <p>{catalogError}</p>
            <Button size="sm" variant="outline" onClick={() => setCatalogAttempt((value) => value + 1)}>{t("toolPresentation.output.retryLoad")}</Button>
          </div> : null}
          {nativeBindingNotice ? <p role="status" className="project-settings-muted">{nativeBindingNotice}</p> : null}
          <DisclosureSection className="project-vmm-create" description={t("projectSettings.vmmCreateBindingHint")} title={t("projectSettings.vmmCreateBinding")}>
            <div className="project-vmm-create-body">
            {props.vmmDraft.backendMode === "external_grpc" ? (
              <div className="project-settings-form-grid">
                <label className="project-settings-field">
                  <span>{t("memory.identity.teamName")}</span>
                  <Input disabled={nativeBindingBusy} onChange={(event) => setCreateExternalTeamName(event.target.value)} value={createExternalTeamName} />
                </label>
                <label className="project-settings-field">
                  <span>{t("memory.identity.spaceName")}</span>
                  <Input disabled={nativeBindingBusy} onChange={(event) => setCreateExternalSpaceName(event.target.value)} value={createExternalSpaceName} />
                </label>
              </div>
            ) : null}
            <label className="project-settings-field wide">
              <span>{t("memory.identity.projectName")}</span>
              <Input disabled={nativeBindingBusy} onChange={(event) => setCreateProjectName(event.target.value)} value={createProjectName} />
            </label>
            {props.vmmDraft.backendMode === "external_grpc" ? (
              <p className="project-settings-muted">
                {t("memory.identity.projectPathPreview")} {canonicalProjectPath(createExternalTeamName, createExternalSpaceName, createProjectName)}
              </p>
            ) : null}
            <Button
              disabled={nativeBindingBusy
                || catalogLoading || Boolean(catalogError)
                || !createProjectName.trim()
                || (props.vmmDraft.backendMode === "external_grpc"
                  && (!createExternalTeamName.trim() || !createExternalSpaceName.trim()))}
              onClick={() => void createAndBindProject()}
              type="button"
              variant="outline"
            >
              {nativeBindingBusy ? <LoaderCircle className="size-4 animate-spin" /> : <FolderPlus size={15} />}
              {t("projectSettings.vmmCreateAndBind")}
            </Button>
            </div>
          </DisclosureSection>
          <p className="project-settings-muted">
            {t(props.vmmDraft.backendMode === "native"
              ? "projectSettings.vmmNativeHostHint"
              : "projectSettings.vmmTransportHint")}
          </p>
          {props.vmmDraft.backendMode === "native" && nativeCatalog?.bindings.some((binding) => binding.vulcanProjectId === props.projectId) ? (
            <VmmNativeHostProjectPolicy key={props.projectId} hostProjectId={props.projectId}
              busy={nativeBindingBusy || props.vmmBusy === "saving"} onRunAction={runNativePolicyAction} />
          ) : null}
        </>
      )}
    </div>
  );
}

// externalProjectBindingDraft freezes one returned external project id in a project-owned VMM layer.
// externalProjectBindingDraft 将一个返回的外置项目标识固定到项目持有的 VMM 配置层。
// Parameters: draft is the latest visible project draft and projectId is the authoritative remote id.
// 参数：draft 是最新可见项目草稿，projectId 是权威远端标识。
// Returns: a project override that preserves every unrelated field and binds by stable id.
// 返回值：保留所有无关字段并按稳定标识绑定的项目覆盖。
export function externalProjectBindingDraft(
  draft: VmmConfigDraftSummary,
  projectId: string,
): VmmConfigDraftSummary {
  const normalizedProjectId = projectId.trim();
  if (!/^[1-9]\d*$/.test(normalizedProjectId)) {
    throw new Error("External VMM project binding requires a positive project id.");
  }
  return {
    ...draft,
    hasOverride: true,
    projectId: normalizedProjectId,
  };
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
  // A successful read at the current settings revision owns the displayed override states.
  // 当前设置版本的成功读取持有所显示覆盖状态的权威。
  agentLoaded: boolean;
  // Read or write failures remain visible until a successful authoritative read or save.
  // 读取或写入失败保持可见，直到权威读取或保存成功。
  agentError: string | null;
  // Retries the existing project override read without changing any configuration.
  // 重试既有项目覆盖读取，不改变任何配置。
  onRetry: () => void;
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
  const { t, locale } = useI18n();
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
      {props.agentError ? <div role="alert" className="space-y-2 text-sm text-destructive">
        <p>{props.agentError}</p>
        <Button type="button" variant="outline" disabled={props.agentLoading || props.agentBusySkill !== null} onClick={props.onRetry}>{locale === "zh-CN" ? "重新读取项目设置" : "Reload project settings"}</Button>
      </div> : null}
      {props.agentLoading ? (
        <p className="project-settings-muted">{t("projectSettings.agentLoading")}</p>
        ) : !props.agentLoaded || props.agentError ? (
        <p className="project-settings-muted">{locale === "zh-CN" ? "项目技能设置尚未读取成功。" : "Project skill settings have not been loaded successfully."}</p>
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
            <label className="project-settings-check-row project-settings-kind-chip" key={kind}>
              <Checkbox
                checked={props.cleanupKinds[kind]}
                disabled={props.cleanupBusy !== "idle"}
                onCheckedChange={(checked) => props.onChangeCleanupKinds({ ...props.cleanupKinds, [kind]: checked === true })}
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
