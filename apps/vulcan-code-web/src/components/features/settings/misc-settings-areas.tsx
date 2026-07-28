import * as React from "react"
import {
  Archive,
  Cpu,
  FileText,
  GitCommitHorizontal,
  PackagePlus,
  RefreshCw,
  Route,
  RotateCcw,
  Save,
  ScrollText,
  SlidersHorizontal,
} from "lucide-react"

import { ModelPickerControl, modelPickerChoiceKey } from "@/components/composer/model-picker-control"
import { PanelTitle, StatusPill } from "@/components/shared/status"
import { Badge } from "@/components/ui/badge"
import { useI18n } from "@/i18n"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import type {
  ComposerAttachmentCapabilitySummary,
  ComposerSlashCommandSummary,
  ConversationSummary,
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types"
import type { GitCommitSummaryModelSelectionSaveRequest } from "@/components/features/settings/settings-action-types"

// DEFAULT_COMMIT_BRANCH_PREFIX mirrors the service default for empty Git commit settings.
// DEFAULT_COMMIT_BRANCH_PREFIX 对齐服务端空 Git 提交设置的默认值。
const DEFAULT_COMMIT_BRANCH_PREFIX = "vulcan/"

// DEFAULT_COMMIT_SUMMARY_MODEL follows the owning conversation session model.
// DEFAULT_COMMIT_SUMMARY_MODEL 跟随所属会话模型。
const DEFAULT_COMMIT_SUMMARY_MODEL: GitCommitSummaryModelSelectionSaveRequest = { mode: "session" }

// GitCommitSettingsArea renders local Git commit dialog preferences.
// GitCommitSettingsArea 渲染 Git 提交弹窗偏好。
export function GitCommitSettingsArea(props: {
  // Service-backed settings actions.
  // 服务端设置操作。
  actions: SettingsActionProps
  // Workbench snapshot that carries the shared Git commit configuration.
  // 携带共享 Git 提交配置的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { locale, t } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const branchPrefix = props.workbench.gitCommit?.newBranchPrefix ?? DEFAULT_COMMIT_BRANCH_PREFIX
  const summaryModel = props.workbench.gitCommit?.summaryModel ?? DEFAULT_COMMIT_SUMMARY_MODEL
  const summaryExtraPrompt = props.workbench.gitCommit?.summaryExtraPrompt ?? ""
  const summaryModelMode = summaryModel.mode
  const summaryModelProviderId = summaryModel.mode === "fixed" ? summaryModel.providerId : ""
  const summaryModelModelId = summaryModel.mode === "fixed" ? summaryModel.modelId : ""
  const [draftPrefix, setDraftPrefix] = React.useState(branchPrefix)
  const [draftSummaryModel, setDraftSummaryModel] = React.useState<GitCommitSummaryModelSelectionSaveRequest>(summaryModel)
  const [draftSummaryExtraPrompt, setDraftSummaryExtraPrompt] = React.useState(summaryExtraPrompt)
  const [saveState, setSaveState] = React.useState<"idle" | "saving" | "saved">("idle")

  React.useEffect(() => {
    setDraftPrefix(branchPrefix)
    setDraftSummaryModel(summaryModelMode === "fixed"
      ? { mode: "fixed", providerId: summaryModelProviderId, modelId: summaryModelModelId }
      : { mode: "session" })
    setDraftSummaryExtraPrompt(summaryExtraPrompt)
  }, [branchPrefix, summaryExtraPrompt, summaryModelMode, summaryModelProviderId, summaryModelModelId])

  const normalizedDraftPrefix = normalizeGitCommitBranchPrefix(draftPrefix)
  const fixedSummaryModelValue = draftSummaryModel.mode === "fixed"
    ? `${draftSummaryModel.providerId}/${draftSummaryModel.modelId}`
    : ""
  const saveDraft = React.useCallback(async () => {
    setSaveState("saving")
    await props.actions.onSaveGitCommitSettings({
      newBranchPrefix: draftPrefix,
      summaryModel: draftSummaryModel,
      summaryExtraPrompt: draftSummaryExtraPrompt,
    })
    setSaveState("saved")
    window.setTimeout(() => setSaveState("idle"), 1200)
  }, [draftPrefix, draftSummaryExtraPrompt, draftSummaryModel, props.actions])

  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<GitCommitHorizontal size={17} />} title={text("Git 提交", "Git commit")} />
      <div className="settings-form-grid">
        <label className="settings-form-field">
          <span>{text("新分支默认前缀", "Default new-branch prefix")}</span>
          <input
            className="settings-input"
            onChange={(event) => setDraftPrefix(event.currentTarget.value)}
            placeholder={DEFAULT_COMMIT_BRANCH_PREFIX}
            value={draftPrefix}
          />
        </label>
        <article className="settings-card">
          <StatusPill status="ready" label={t("settings.gitCommit.active")} />
          <strong>{normalizedDraftPrefix}</strong>
          <p>{text("仅当前分支没有命名空间时使用；已有命名空间的分支会沿用当前命名空间。", "Used only when the current branch has no namespace; namespaced branches keep their current namespace.")}</p>
        </article>
        <article className="settings-card">
          <StatusPill status="ready" label={t("settings.gitCommit.autoSummary")} />
          <strong>{t("settings.gitCommit.summaryModel")}</strong>
          <div className="settings-card-actions">
            {draftSummaryModel.mode === "session" ? (
              <button className="skill-action-button" type="button">
                <Cpu size={14} />
                <span>{t("settings.gitCommit.followSessionModel")}</span>
              </button>
            ) : (
              <button className="skill-action-button" type="button">
                <Cpu size={14} />
                <span>{draftSummaryModel.providerId}/{draftSummaryModel.modelId}</span>
              </button>
            )}
            <ModelPickerControl
              ariaLabel={t("settings.gitCommit.chooseSummaryModel")}
              fallbackLabel={t("composer.selectFallback")}
              filterPlaceholder={t("composer.modelPicker.filter")}
              noMatchesLabel={t("composer.modelPicker.noMatches")}
              providers={props.workbench.providers}
              value={fixedSummaryModelValue}
              onValueChange={(choice) => setDraftSummaryModel({
                mode: "fixed",
                providerId: choice.providerId,
                modelId: choice.modelId,
              })}
            />
            <button
              className="skill-action-button"
              disabled={draftSummaryModel.mode === "session"}
              onClick={() => setDraftSummaryModel({ mode: "session" })}
              type="button"
            >
              <RotateCcw size={14} />
              <span>{t("settings.gitCommit.resetSummaryModel")}</span>
            </button>
          </div>
          <p>{draftSummaryModel.mode === "session" ? t("settings.gitCommit.followSessionModelHelp") : t("settings.gitCommit.fixedSummaryModelHelp")}</p>
          {draftSummaryModel.mode === "fixed" && !modelPickerChoiceExists(props.workbench.providers, fixedSummaryModelValue) ? (
            <p className="settings-muted-line">{t("settings.gitCommit.fixedSummaryModelUnavailable")}</p>
          ) : null}
        </article>
        <label className="settings-form-field">
          <span>{t("settings.gitCommit.summaryExtraPrompt")}</span>
          <textarea
            className="settings-input"
            onChange={(event) => setDraftSummaryExtraPrompt(event.currentTarget.value)}
            placeholder={t("settings.gitCommit.summaryExtraPromptPlaceholder")}
            rows={4}
            value={draftSummaryExtraPrompt}
          />
          <small className="settings-muted-line">{t("settings.gitCommit.summaryExtraPromptHelp")}</small>
        </label>
        <div className="settings-card-actions">
          <button className="skill-action-button" disabled={saveState === "saving"} onClick={() => void saveDraft()} type="button">
            {saveState === "saving" ? text("保存中", "Saving") : saveState === "saved" ? text("已保存", "Saved") : text("保存", "Save")}
          </button>
        </div>
      </div>
    </section>
  )
}

// normalizeGitCommitBranchPrefix mirrors the service-side prefix normalization for preview text.
// normalizeGitCommitBranchPrefix 对齐服务端前缀归一化逻辑以显示预览文本。
function normalizeGitCommitBranchPrefix(value: string): string {
  const trimmedValue = value.trim()
  if (!trimmedValue) {
    return DEFAULT_COMMIT_BRANCH_PREFIX
  }
  return trimmedValue.endsWith("/") ? trimmedValue : `${trimmedValue}/`
}

// modelPickerChoiceExists returns whether one provider/model key is currently selectable.
// modelPickerChoiceExists 返回某个 Provider/模型键当前是否可选。
function modelPickerChoiceExists(providers: WorkbenchSnapshot["providers"], value: string): boolean {
  if (!value) {
    return false
  }
  return providers.some((provider) => (provider.models ?? []).some((model) => modelPickerChoiceKey({
    providerId: provider.sourceProviderId ?? provider.id,
    connectionId: provider.connectionId ?? "",
    providerName: provider.name,
    modelId: model.id,
    modelName: model.name,
    active: Boolean(model.active),
  }) === value))
}

// PromptComposerSettingsArea renders composer command and capability settings status.
// PromptComposerSettingsArea 渲染输入区指令与能力设置状态。
export function PromptComposerSettingsArea(props: {
  // Composer attachment capability gates.
  // 输入区附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[]
  // Backend-resolved reasoning selector view.
  // 后端解析出的推理选择器视图。
  reasoningSelection: WorkbenchSnapshot["reasoningSelection"] | null
  // Composer slash commands.
  // 输入区 slash 指令。
  slashCommands: ComposerSlashCommandSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  return (
    <div className="area-grid two-columns">
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<FileText size={17} />} title={text("Slash 指令", "Slash commands")} />
        <div className="settings-target-list">
          {props.slashCommands.map((command) => (
            <article className="settings-target-row" key={command.id}>
              <div>
                <strong>{command.command}</strong>
                <span>{command.descriptionZh || command.descriptionEn}</span>
              </div>
              <Badge variant="outline">{command.id}</Badge>
            </article>
          ))}
        </div>
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<PackagePlus size={17} />} title={text("附件能力", "Attachment capability")} />
        <div className="settings-grid compact">
          {props.attachmentCapabilities.map((capability) => (
            <article className="settings-card" key={capability.kind}>
              <StatusPill status={capability.enabled ? "ready" : "warning"} label={capability.enabled ? "enabled" : "disabled"} />
              <strong>{capability.label}</strong>
              <p>{capability.native ? text("模型原生支持", "Native model support") : capability.bridged ? text("通过宿主工具桥接", "Bridged by host tools") : text("当前不可用", "Currently unavailable")}</p>
            </article>
          ))}
        </div>
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<SlidersHorizontal size={17} />} title={text("推理选择", "Reasoning selection")} />
        <div className="settings-target-list">
          {(props.reasoningSelection?.options ?? []).map((option) => (
            <article className="settings-target-row" key={option.value}>
              <div>
                <strong>{option.label}</strong>
                <span>{option.value}</span>
              </div>
              {option.value === props.reasoningSelection?.selectedEffort ? <Badge className="skill-card-enabled-badge">Selected</Badge> : <Badge variant="outline">Available</Badge>}
            </article>
          ))}
        </div>
      </section>
    </div>
  )
}

// AboutLicensesSettingsArea renders application version and license entry information.
// AboutLicensesSettingsArea 渲染应用版本与许可证入口信息。
export function AboutLicensesSettingsArea(props: {
  // Current application version.
  // 当前应用版本。
  appVersion: string
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<ScrollText size={17} />} title={text("关于与许可证", "About and licenses")} />
      <div className="settings-grid">
        <article className="settings-card">
          <StatusPill status="ready" label="version" />
          <strong>Vulcan Code</strong>
          <p>{text(`当前版本：${props.appVersion}`, `Current version: ${props.appVersion}`)}</p>
        </article>
        <article className="settings-card">
          <StatusPill status="pending" label="licenses" />
          <strong>{text("许可证目录", "License directory")}</strong>
          <p>{text("WebUI 当前展示许可证入口状态，后续接入目录命令后可直接打开。", "The WebUI shows the license entry state until the directory command is connected.")}</p>
        </article>
      </div>
    </section>
  )
}

// ArchivedRestoreSettingsArea renders restorable archived conversations from service snapshots.
// ArchivedRestoreSettingsArea 根据服务快照渲染可恢复的归档会话。
export function ArchivedRestoreSettingsArea(props: {
  // Archived conversation summaries.
  // 归档会话摘要。
  conversations: ConversationSummary[]
  // Active project summaries.
  // 活动项目摘要。
  projects: ProjectSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const archivedConversations = props.conversations
  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<Archive size={17} />} title={text("归档恢复", "Archived restore")} />
      <div className="settings-summary-strip">
        <StatusPill status={archivedConversations.length > 0 ? "warning" : "ready"} label={`${archivedConversations.length}`} />
        <span>{text(`当前 WebUI snapshot 暴露 ${props.projects.length} 个活动项目与 ${archivedConversations.length} 个可恢复归档会话。`, `The current WebUI snapshot exposes ${props.projects.length} active projects and ${archivedConversations.length} restorable archived conversations.`)}</span>
      </div>
      <div className="settings-target-list">
        {archivedConversations.length === 0 ? (
          <p className="settings-muted-line">{text("当前快照中没有可恢复的归档会话。", "No restorable archived conversations are exposed in the current snapshot.")}</p>
        ) : archivedConversations.map((conversation) => (
          <article className="settings-target-row" key={conversation.id}>
            <div>
              <strong>{conversation.title}</strong>
              <span>{conversation.projectId ?? "standalone"} · {conversation.lastUpdatedLabel ?? ""}</span>
            </div>
            <StatusPill status="warning" label={conversation.status} />
          </article>
        ))}
      </div>
    </section>
  )
}

// localizedLiteral picks the display literal that matches the active locale.
// localizedLiteral 按当前语言选择展示文本。
function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}
