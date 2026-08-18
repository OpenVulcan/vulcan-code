import * as React from "react"
import {
  Archive,
  ChevronLeft,
  ChevronRight,
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
  Trash2,
} from "lucide-react"

import { ModelPickerControl, modelPickerChoiceKey } from "@/components/composer/model-picker-control"
import { PanelTitle, StatusPill } from "@/components/shared/status"
import { Badge } from "@/components/ui/badge"
import { Checkbox } from "@/components/ui/checkbox"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
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

// ARCHIVED_CONVERSATION_PAGE_SIZE keeps the archive recovery surface readable while selection spans pages.
// ARCHIVED_CONVERSATION_PAGE_SIZE 在跨分页保留选择的同时保持归档恢复页面可读。
const ARCHIVED_CONVERSATION_PAGE_SIZE = 20

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
          <Input
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
          <Textarea
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
  // Service-backed settings actions.
  // 服务端设置动作。
  actions: SettingsActionProps
  // Composer attachment capability gates.
  // 输入区附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[]
  // Global output-style catalog and persisted selection.
  // 全局输出风格目录与已持久化选择。
  outputStyleCatalog: WorkbenchSnapshot["outputStyleCatalog"]
  // Backend-resolved reasoning selector view.
  // 后端解析出的推理选择器视图。
  reasoningSelection: WorkbenchSnapshot["reasoningSelection"] | null
  // Composer slash commands.
  // 输入区 slash 指令。
  slashCommands: ComposerSlashCommandSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [outputStyleId, setOutputStyleId] = React.useState(props.outputStyleCatalog.selectedId)
  const [outputStyleSaving, setOutputStyleSaving] = React.useState(false)
  React.useEffect(() => {
    setOutputStyleId(props.outputStyleCatalog.selectedId)
  }, [props.outputStyleCatalog.selectedId])
  const saveOutputStyle = React.useCallback(async () => {
    setOutputStyleSaving(true)
    try {
      await props.actions.onSavePromptOutputStyle(outputStyleId)
    } finally {
      setOutputStyleSaving(false)
    }
  }, [outputStyleId, props.actions])
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
        <PanelTitle icon={<SlidersHorizontal size={17} />} title={text("输出风格", "Output style")} />
        <div className="settings-form-grid">
          <div className="settings-field">
            <span>{text("新会话默认风格", "Default for new sessions")}</span>
            <Select onValueChange={setOutputStyleId} value={outputStyleId}>
              <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
              {props.outputStyleCatalog.entries.map((entry) => (
                <SelectItem key={entry.id} value={entry.id}>
                  {`${locale === "zh-CN" ? entry.labelZh : entry.labelEn} · ${entry.promptCharCount} chars`}
                </SelectItem>
              ))}
              </SelectContent>
            </Select>
          </div>
          <p className="settings-muted-line">
            {text("全局设置仅在新会话或上下文压缩后的会话应用，避免替换当前系统提示词。", "The global setting applies only at a new session or compaction boundary, so the active system prompt is never replaced mid-context.")}
          </p>
          <button className="button button-primary" disabled={outputStyleSaving} onClick={() => void saveOutputStyle()} type="button">
            {outputStyleSaving ? text("保存中…", "Saving…") : text("保存输出风格", "Save output style")}
          </button>
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
  // Service-backed archive lifecycle actions.
  // 服务端支持的归档生命周期操作。
  actions: SettingsActionProps
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
  const [selectedConversationIds, setSelectedConversationIds] = React.useState<Set<string>>(new Set())
  const [currentPage, setCurrentPage] = React.useState(1)
  const [operation, setOperation] = React.useState<"idle" | "restoring" | "deleting">("idle")
  const [actionError, setActionError] = React.useState<string | null>(null)
  // deleteConfirmationOpen controls the component-based permanent-delete confirmation.
  // deleteConfirmationOpen 控制基于组件的彻底删除确认框。
  const [deleteConfirmationOpen, setDeleteConfirmationOpen] = React.useState(false)
  const pageCount = Math.max(1, Math.ceil(archivedConversations.length / ARCHIVED_CONVERSATION_PAGE_SIZE))
  const visibleConversations = archivedConversations.slice(
    (currentPage - 1) * ARCHIVED_CONVERSATION_PAGE_SIZE,
    currentPage * ARCHIVED_CONVERSATION_PAGE_SIZE,
  )
  const selectedCount = selectedConversationIds.size
  const allConversationsSelected = archivedConversations.length > 0 && selectedCount === archivedConversations.length
  const currentPageSelected = visibleConversations.length > 0 && visibleConversations.every((conversation) => selectedConversationIds.has(conversation.id))

  // Reconciles selection and pagination with the latest authoritative workbench snapshot.
  // 将选择与分页状态和最新权威工作台快照重新对齐。
  React.useEffect(() => {
    const availableIds = new Set(archivedConversations.map((conversation) => conversation.id))
    setSelectedConversationIds((current) => {
      const retained = new Set(Array.from(current).filter((conversationId) => availableIds.has(conversationId)))
      return retained.size === current.size ? current : retained
    })
    setCurrentPage((current) => Math.min(current, pageCount))
  }, [archivedConversations, pageCount])

  // toggleConversationSelection changes one stable archived conversation identity.
  // toggleConversationSelection 切换一个稳定的归档会话标识。
  const toggleConversationSelection = React.useCallback((conversationId: string, checked: boolean) => {
    setSelectedConversationIds((current) => {
      const next = new Set(current)
      if (checked) {
        next.add(conversationId)
      } else {
        next.delete(conversationId)
      }
      return next
    })
  }, [])

  // toggleAllConversationSelection selects or clears every archived conversation across all pages.
  // toggleAllConversationSelection 跨所有分页全选或清空归档会话。
  const toggleAllConversationSelection = React.useCallback((checked: boolean) => {
    setSelectedConversationIds(checked
      ? new Set(archivedConversations.map((conversation) => conversation.id))
      : new Set())
  }, [archivedConversations])

  // runArchivedConversationAction executes one batch lifecycle operation after UI confirmation.
  // runArchivedConversationAction 在界面确认后执行一次批量生命周期操作。
  const runArchivedConversationAction = React.useCallback(async (kind: "restore" | "delete") => {
    const conversationIds = Array.from(selectedConversationIds)
    if (conversationIds.length === 0 || operation !== "idle") {
      return
    }
    setActionError(null)
    setOperation(kind === "restore" ? "restoring" : "deleting")
    try {
      if (kind === "restore") {
        await props.actions.onRestoreArchivedConversations(conversationIds)
      } else {
        await props.actions.onDeleteArchivedConversations(conversationIds)
      }
      setSelectedConversationIds(new Set())
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error))
    } finally {
      setOperation("idle")
    }
  }, [operation, props.actions, selectedConversationIds])

  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<Archive size={17} />} title={text("归档恢复", "Archived restore")} />
      <div className="settings-summary-strip">
        <StatusPill status={archivedConversations.length > 0 ? "warning" : "ready"} label={`${archivedConversations.length}`} />
        <span>{text(`当前 WebUI snapshot 暴露 ${props.projects.length} 个活动项目与 ${archivedConversations.length} 个可恢复归档会话。`, `The current WebUI snapshot exposes ${props.projects.length} active projects and ${archivedConversations.length} restorable archived conversations.`)}</span>
      </div>
      <div className="settings-card-actions">
        <label className="settings-target-row-main">
          <Checkbox
            aria-label={text("全选归档会话", "Select all archived conversations")}
            checked={allConversationsSelected ? true : selectedCount > 0 ? "indeterminate" : false}
            disabled={archivedConversations.length === 0 || operation !== "idle"}
            onCheckedChange={(checked) => toggleAllConversationSelection(checked === true)}
          />
          <span>{text("全选", "Select all")}</span>
        </label>
        <span className="settings-muted-line">{text(`已选择 ${selectedCount} 项`, `${selectedCount} selected`)}</span>
        <button
          className="skill-action-button"
          disabled={selectedCount === 0 || operation !== "idle"}
          onClick={() => void runArchivedConversationAction("restore")}
          type="button"
        >
          <RotateCcw size={14} />
          <span>{operation === "restoring" ? text("恢复中", "Restoring") : text("批量恢复", "Restore selected")}</span>
        </button>
        <button
          className="skill-action-button secondary danger"
          disabled={selectedCount === 0 || operation !== "idle"}
          onClick={() => setDeleteConfirmationOpen(true)}
          type="button"
        >
          <Trash2 size={14} />
          <span>{operation === "deleting" ? text("删除中", "Deleting") : text("彻底删除", "Delete permanently")}</span>
        </button>
      </div>
      {actionError ? <p className="settings-validation-error" role="alert">{actionError}</p> : null}
      <AlertDialog onOpenChange={setDeleteConfirmationOpen} open={deleteConfirmationOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{text("彻底删除归档会话", "Permanently delete archived conversations")}</AlertDialogTitle>
            <AlertDialogDescription>
              {text(
                `确定彻底删除已选择的 ${selectedCount} 个归档会话吗？删除后无法恢复。`,
                `Permanently delete ${selectedCount} selected archived conversations? This cannot be undone.`,
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{text("取消", "Cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void runArchivedConversationAction("delete")} variant="destructive">
              {text("彻底删除", "Delete permanently")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <div className="settings-target-list">
        {archivedConversations.length === 0 ? (
          <p className="settings-muted-line">{text("当前快照中没有可恢复的归档会话。", "No restorable archived conversations are exposed in the current snapshot.")}</p>
        ) : visibleConversations.map((conversation) => (
          <article className="settings-target-row" key={conversation.id}>
            <div className="settings-target-row-main">
              <Checkbox
                aria-label={text(`选择会话 ${conversation.title}`, `Select conversation ${conversation.title}`)}
                checked={selectedConversationIds.has(conversation.id)}
                disabled={operation !== "idle"}
                onCheckedChange={(checked) => toggleConversationSelection(conversation.id, checked === true)}
              />
              <div>
              <strong>{conversation.title}</strong>
              <span>{conversation.projectId ?? "standalone"} · {conversation.lastUpdatedLabel ?? ""}</span>
              </div>
            </div>
            <StatusPill status="warning" label={conversation.status} />
          </article>
        ))}
      </div>
      {archivedConversations.length > 0 ? (
        <div className="settings-pagination">
          <span className="settings-muted-line">{text(`第 ${currentPage} / ${pageCount} 页`, `Page ${currentPage} of ${pageCount}`)}</span>
          <div className="settings-card-actions">
            <span className="settings-muted-line">{currentPageSelected ? text("本页已全选", "Current page selected") : text("可跨页保留选择", "Selection is kept across pages")}</span>
            <button
              aria-label={text("上一页", "Previous page")}
              className="skill-action-button secondary"
              disabled={currentPage <= 1 || operation !== "idle"}
              onClick={() => setCurrentPage((current) => Math.max(1, current - 1))}
              type="button"
            >
              <ChevronLeft size={14} />
            </button>
            <button
              aria-label={text("下一页", "Next page")}
              className="skill-action-button secondary"
              disabled={currentPage >= pageCount || operation !== "idle"}
              onClick={() => setCurrentPage((current) => Math.min(pageCount, current + 1))}
              type="button"
            >
              <ChevronRight size={14} />
            </button>
          </div>
        </div>
      ) : null}
    </section>
  )
}

// localizedLiteral picks the display literal that matches the active locale.
// localizedLiteral 按当前语言选择展示文本。
function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}
