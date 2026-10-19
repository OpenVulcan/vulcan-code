import * as React from "react"
import {
  ChevronLeft,
  ChevronRight,
  GitCommitHorizontal,
  RotateCcw,
  Save,
  Trash2,
} from "lucide-react"

import { ModelPickerControl, modelPickerGroupKey } from "@/components/composer/model-picker-control"
import { Button } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Textarea } from "@/ui-kit/primitives/textarea"
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice"
import { formatMessage, pickBilingualText, useI18n } from "@/i18n"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import type { AnswerMode } from "@/service/answer-mode"
import type {
  ConversationSummary,
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types"
import type { GitCommitSummaryModelSelectionSaveRequest } from "@/components/features/settings/settings-action-types"
import "@/ui-kit/styles/features/settings-forms.css"

// DEFAULT_COMMIT_BRANCH_PREFIX mirrors the service default for empty Git commit settings.
// DEFAULT_COMMIT_BRANCH_PREFIX 对齐服务端空 Git 提交设置的默认值。
const DEFAULT_COMMIT_BRANCH_PREFIX = "vulcan/"

// DEFAULT_COMMIT_SUMMARY_MODEL follows the owning conversation session model.
// DEFAULT_COMMIT_SUMMARY_MODEL 跟随所属会话模型。
const DEFAULT_COMMIT_SUMMARY_MODEL: GitCommitSummaryModelSelectionSaveRequest = { mode: "session" }

// ARCHIVED_CONVERSATION_PAGE_SIZE keeps the archive recovery surface readable while selection spans pages.
// ARCHIVED_CONVERSATION_PAGE_SIZE 在跨分页保留选择的同时保持归档恢复页面可读。
const ARCHIVED_CONVERSATION_PAGE_SIZE = 20

// GitCommitSettingsArea edits model-scoped generation preferences without selecting a credential.
// GitCommitSettingsArea 编辑模型作用域的生成偏好，不将凭据账号写入配置。
// Parameters: actions saves the draft; workbench supplies authoritative persisted settings.
// 参数：actions 保存草稿；workbench 提供权威持久化设置。
// Returns the generation form, branch naming preview and explicit save feedback.
// 返回生成表单、分支命名预览和明确保存反馈。
export function GitCommitSettingsArea(props: {
  // Service-backed settings actions.
  // 服务端设置操作。
  actions: SettingsActionProps
  // Authoritative persisted Git settings and executable model catalog.
  // 权威持久化 Git 设置与可执行模型目录。
  workbench: WorkbenchSnapshot
}) {
  // Localized copy follows the active interface language.
  // 本地化文案跟随当前界面语言。
  const { t } = useI18n()
  // persistedDraft records only fields accepted by the save contract.
  // persistedDraft 仅记录保存契约接受的字段。
  const persistedDraft = {
    newBranchPrefix: props.workbench.gitCommit?.newBranchPrefix ?? DEFAULT_COMMIT_BRANCH_PREFIX,
    summaryModel: props.workbench.gitCommit?.summaryModel ?? DEFAULT_COMMIT_SUMMARY_MODEL,
    summaryExtraPrompt: props.workbench.gitCommit?.summaryExtraPrompt ?? "",
  }
  // persistedKey detects changes without depending on snapshot object identity.
  // persistedKey 检测变化，不依赖快照对象身份。
  const persistedKey = JSON.stringify(persistedDraft)
  // baselineRef preserves unsaved input when a background snapshot changes.
  // baselineRef 在后台快照变化时保护未保存输入。
  const baselineRef = React.useRef(persistedKey)
  // draft is the editable settings payload.
  // draft 是可编辑设置载荷。
  const [draft, setDraft] = React.useState(persistedDraft)
  // saveState and saveError expose saving, completion and failure independently.
  // saveState 与 saveError 分别展示保存过程、完成和失败。
  const [saveState, setSaveState] = React.useState<"idle" | "saving" | "saved">("idle")
  const [saveError, setSaveError] = React.useState("")
  // savingRef prevents duplicate synchronous submissions before React disables the button.
  // savingRef 在 React 禁用按钮前防止同步重复提交。
  const savingRef = React.useRef(false)
  React.useEffect(() => {
    // nextDraft is decoded from the locally serialized, typed settings object.
    // nextDraft 从本地序列化的类型化设置对象还原。
    const nextDraft = JSON.parse(persistedKey) as typeof draft
    // previousBaseline identifies whether the user has edited the previous snapshot.
    // previousBaseline 用于判断用户是否已编辑此前快照。
    const previousBaseline = baselineRef.current
    setDraft((current) => JSON.stringify(current) === previousBaseline ? nextDraft : current)
    baselineRef.current = persistedKey
  }, [persistedKey])
  // fixedModelValue intentionally contains no connection identifier.
  // fixedModelValue 明确不包含连接标识。
  const fixedModelValue = draft.summaryModel.mode === "fixed"
    ? modelPickerGroupKey(draft.summaryModel)
    : ""
  // fixedModelLabel preserves the exact unavailable selection in the trigger.
  // fixedModelLabel 在触发器中保留精确的失效选择。
  const fixedModelLabel = draft.summaryModel.mode === "fixed"
    ? `${draft.summaryModel.providerId}/${draft.summaryModel.modelId}`
    : undefined
  // dirty describes the current payload against the latest persisted snapshot.
  // dirty 描述当前载荷相对最新持久化快照的变化。
  const dirty = JSON.stringify(draft) !== persistedKey
  // saveDraft submits one immutable form snapshot and retains input on failure.
  // saveDraft 提交一个不可变表单快照，失败时保留输入。
  // Parameters: none. Returns after the explicit save attempt settles.
  // 参数：无。返回值：在明确保存尝试结束后完成。
  const saveDraft = async () => {
    if (savingRef.current) return
    savingRef.current = true
    setSaveState("saving")
    setSaveError("")
    try {
      await props.actions.onSaveGitCommitSettings(draft)
      setSaveState("saved")
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : String(error))
      setSaveState("idle")
    } finally {
      savingRef.current = false
    }
  }
  return (
    <section className="settings-detail-panel settings-preferences-page git-commit-settings-page">
      <fieldset className="settings-preferences-fields" disabled={saveState === "saving"}>
        <section className="settings-preferences-section">
          <h2><GitCommitHorizontal size={16} />{t("settings.misc.whoWritesCommitMessages")}</h2>
          <div className="settings-form-field">
            <span>{t("settings.gitCommit.summaryModel")}</span>
            <div className="settings-card-actions">
              <ModelPickerControl
                selectionScope="model"
                ariaLabel={t("settings.gitCommit.chooseSummaryModel")}
                fallbackLabel={t("settings.gitCommit.followSessionModel")}
                unavailableValueLabel={fixedModelLabel}
                filterPlaceholder={t("composer.modelPicker.filter")}
                noMatchesLabel={t("composer.modelPicker.noMatches")}
                providers={props.workbench.providers}
                value={fixedModelValue}
                disabled={saveState === "saving"}
                onValueChange={(choice) => setDraft({ ...draft, summaryModel: { mode: "fixed", providerId: choice.providerId, modelId: choice.modelId } })}
              />
              {draft.summaryModel.mode === "fixed" ? <Button
                variant="ghost"
                onClick={() => setDraft({ ...draft, summaryModel: { mode: "session" } })}
                type="button"
              ><RotateCcw size={14} />{t("settings.gitCommit.followSessionModel")}</Button> : null}
            </div>
            <small className="settings-muted-line">{draft.summaryModel.mode === "session" ? t("settings.gitCommit.followSessionModelHelp") : t("settings.gitCommit.fixedSummaryModelHelp")}</small>
            {fixedModelValue && !modelPickerChoiceExists(props.workbench.providers, fixedModelValue) ? (
              <p role="status" className="settings-muted-line">{t("settings.gitCommit.fixedSummaryModelUnavailable")}</p>
            ) : null}
          </div>
        </section>
        <section className="settings-preferences-section">
          <h2>{t("settings.misc.howCommitMessagesAre")}</h2>
          <label className="settings-form-field">
            <span>{t("settings.gitCommit.summaryExtraPrompt")}</span>
            <Textarea rows={4} value={draft.summaryExtraPrompt}
              placeholder={t("settings.gitCommit.summaryExtraPromptPlaceholder")}
              onChange={(event) => setDraft({ ...draft, summaryExtraPrompt: event.currentTarget.value })} />
            <small className="settings-muted-line">{t("settings.gitCommit.summaryExtraPromptHelp")}</small>
          </label>
        </section>
        <section className="settings-preferences-section">
          <h2>{t("settings.misc.branchNaming")}</h2>
          <label className="settings-form-field">
            <span>{t("settings.misc.newBranchNamePrefix")}</span>
            <Input value={draft.newBranchPrefix} placeholder={DEFAULT_COMMIT_BRANCH_PREFIX}
              onChange={(event) => setDraft({ ...draft, newBranchPrefix: event.currentTarget.value })} />
            <small className="settings-muted-line">{t("settings.misc.example")}：<code>{normalizeGitCommitBranchPrefix(draft.newBranchPrefix)}feature-name</code></small>
            <small className="settings-muted-line">{t("settings.misc.onlyAffectsNewBranches")}</small>
          </label>
        </section>
      </fieldset>
      <div className="settings-preferences-footer">
        <span role="status" className="settings-muted-line">{dirty ? t("settings.misc.unsavedChanges") : saveState === "saved" ? t("settings.misc.saved") : ""}</span>
        <Button disabled={saveState === "saving" || !dirty} onClick={() => void saveDraft()} type="button">
          <Save size={14} />{saveState === "saving" ? t("settings.misc.saving") : t("settings.misc.save")}
        </Button>
      </div>
      {saveError ? <SettingsErrorNotice className="settings-save-error" detail={saveError} message={t("settings.misc.couldnTSaveYour")} /> : null}
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
  return providers.some((provider) => (provider.models ?? []).some((model) => modelPickerGroupKey({
    providerId: provider.sourceProviderId ?? provider.id,
    modelId: model.id,
  }) === value))
}

// OutputStyleSettingsSection keeps the useful session default inside model basics.
// OutputStyleSettingsSection 将有用的会话默认选项保留在模型基础设置内。
// Parameters: actions persists the selected style; outputStyleCatalog supplies valid styles.
// 参数：actions 持久化所选风格；outputStyleCatalog 提供有效风格。
// Returns a compact form with explicit saving and failure feedback.
// 返回包含明确保存与失败反馈的紧凑表单。
export function OutputStyleSettingsSection(props: {
  // Service-backed output-style action.
  // 服务端输出风格动作。
  actions: SettingsActionProps
  // Catalog and persisted selection.
  // 目录与持久化选择。
  outputStyleCatalog: WorkbenchSnapshot["outputStyleCatalog"]
}) {
  // locale selects visible copy.
  // locale 选择显示文案。
  const { locale, t } = useI18n()
  // outputStyleId is the local editable selection.
  // outputStyleId 是本地可编辑选择。
  const [outputStyleId, setOutputStyleId] = React.useState(props.outputStyleCatalog.selectedId)
  // previousStyleRef prevents background refresh from overwriting an edited selection.
  // previousStyleRef 防止后台刷新覆盖已编辑选择。
  const previousStyleRef = React.useRef(props.outputStyleCatalog.selectedId)
  // saving and error distinguish a pending request from a failed save.
  // saving 与 error 区分进行中的请求和保存失败。
  const [saving, setSaving] = React.useState(false)
  const [saved, setSaved] = React.useState(false)
  const [error, setError] = React.useState("")
  // savingRef closes the synchronous duplicate-click window.
  // savingRef 关闭同步重复点击窗口。
  const savingRef = React.useRef(false)
  React.useEffect(() => {
    // previousStyle identifies the last authoritative selection.
    // previousStyle 标识上一次权威选择。
    const previousStyle = previousStyleRef.current
    setOutputStyleId((current) => current === previousStyle ? props.outputStyleCatalog.selectedId : current)
    previousStyleRef.current = props.outputStyleCatalog.selectedId
  }, [props.outputStyleCatalog.selectedId])
  // saveOutputStyle preserves the existing application boundary and retains input on errors.
  // saveOutputStyle 保留既有应用边界，错误时保留输入。
  // Parameters: none. Returns when the save settles.
  // 参数：无。返回值：保存结束后完成。
  // Choosing a style saves it at once, matching the neighbouring switches; failures keep the choice for retry.
  // 选择风格即保存，与相邻开关一致；失败时保留选择以便重试。
  const saveOutputStyle = async (nextId: string) => {
    if (savingRef.current) return
    savingRef.current = true
    setSaving(true)
    setSaved(false)
    setError("")
    try {
      await props.actions.onSavePromptOutputStyle(nextId)
      setSaved(true)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }
  // selectStyle applies a valid, changed selection and persists it immediately.
  // selectStyle 应用有效且有变化的选择并立即保存。
  const selectStyle = (nextId: string) => {
    setOutputStyleId(nextId)
    if (nextId !== props.outputStyleCatalog.selectedId && props.outputStyleCatalog.entries.some((entry) => entry.id === nextId)) void saveOutputStyle(nextId)
  }
  return <div className="settings-form-grid">
    <label className="settings-form-field">
      <span>{t("settings.misc.answerStyleForNew")}</span>
      <Select disabled={saving} onValueChange={selectStyle} value={outputStyleId}>
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>{props.outputStyleCatalog.entries.map((entry) => (
          <SelectItem key={entry.id} value={entry.id}>{pickBilingualText(locale, entry.labelZh, entry.labelEn)}</SelectItem>
        ))}</SelectContent>
      </Select>
    </label>
    <p className="settings-muted-line">{t("settings.misc.takesEffectForNew")}</p>
    {saving || saved ? <p className="settings-muted-line" role="status">{saving ? t("settings.misc.saving2") : t("settings.misc.saved")}</p> : null}
    {error ? <SettingsErrorNotice detail={error} message={t("settings.misc.couldnTSaveYour2")} /> : null}
  </div>
}

// AnswerModeSettingsSection chooses the answer mode new conversations copy when they are created.
// AnswerModeSettingsSection 选择新会话创建时复制的回答模式。
// Parameters: actions persists the mode; defaultAnswerMode is the persisted global value.
// 参数：actions 持久化模式；defaultAnswerMode 是已持久化的全局值。
export function AnswerModeSettingsSection(props: {
  // Service-backed answer-mode action.
  // 服务端回答模式动作。
  actions: SettingsActionProps
  // Persisted global answer mode.
  // 已持久化的全局回答模式。
  defaultAnswerMode: AnswerMode
}) {
  const { t } = useI18n()
  // mode follows the authoritative value unless a save is pending or failed.
  // mode 跟随权威值，保存进行中或失败时除外。
  const [mode, setMode] = React.useState(props.defaultAnswerMode)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState("")
  const savingRef = React.useRef(false)
  React.useEffect(() => { setMode(props.defaultAnswerMode) }, [props.defaultAnswerMode])
  // selectMode saves a changed choice at once and keeps it for retry on failure.
  // selectMode 立即保存有变化的选择，失败时保留以便重试。
  const selectMode = async (next: string) => {
    if (next !== "standard" && next !== "interactive") return
    setMode(next)
    if (next === props.defaultAnswerMode || savingRef.current) return
    savingRef.current = true
    setSaving(true)
    setError("")
    try {
      await props.actions.onSavePromptAnswerMode(next)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }
  return <div className="settings-form-grid">
    <label className="settings-form-field">
      <span>{t("answerMode.globalDefault")}</span>
      <Select disabled={saving} onValueChange={(next) => void selectMode(next)} value={mode}>
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="standard">{t("answerMode.standard")}</SelectItem>
          <SelectItem value="interactive">{t("answerMode.interactive")}</SelectItem>
        </SelectContent>
      </Select>
    </label>
    <p className="settings-muted-line">{t(mode === "interactive" ? "answerMode.interactiveHint" : "answerMode.standardHint")}</p>
    <p className="settings-muted-line">{t("settings.misc.takesEffectForNew")}</p>
    {saving ? <p className="settings-muted-line" role="status">{t("settings.misc.saving2")}</p> : null}
    {error ? <SettingsErrorNotice detail={error} message={t("answerMode.saveFailed")} /> : null}
  </div>
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
  const { t } = useI18n()
  const archivedConversations = props.conversations
  const [selectedConversationIds, setSelectedConversationIds] = React.useState<Set<string>>(new Set())
  const [currentPage, setCurrentPage] = React.useState(1)
  const [operation, setOperation] = React.useState<"idle" | "restoring" | "deleting">("idle")
  // Lock synchronously before React commits busy state to prevent duplicate destructive requests.
  // 在 React 提交忙碌状态前同步加锁，防止重复提交破坏性请求。
  const operationRunningRef = React.useRef(false)
  const [actionError, setActionError] = React.useState<string | null>(null)
  // Freeze the exact row or batch identities being confirmed, independently of checkbox selection.
  // 固定正在确认的单行或批量标识，与复选框选择独立。
  const [deleteTargetIds, setDeleteTargetIds] = React.useState<string[]>([])
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

  // Execute the requested operation only for its explicit identities, preserving unrelated selections.
  // 仅对明确传入标识执行指定操作，保留其他勾选项。
  // kind selects restore/delete; conversationIds names the exact targets. Resolves after completion or visible failure.
  // kind 选择恢复或删除；conversationIds 指定精确目标。完成或展示失败后返回。
  const runArchivedConversationAction = React.useCallback(async (kind: "restore" | "delete", conversationIds: string[]) => {
    if (conversationIds.length === 0 || operationRunningRef.current) {
      return
    }
    operationRunningRef.current = true
    setActionError(null)
    setOperation(kind === "restore" ? "restoring" : "deleting")
    try {
      if (kind === "restore") {
        await props.actions.onRestoreArchivedConversations(conversationIds)
      } else {
        await props.actions.onDeleteArchivedConversations(conversationIds)
      }
      setSelectedConversationIds((current) => new Set(Array.from(current).filter((id) => !conversationIds.includes(id))))
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error))
    } finally {
      operationRunningRef.current = false
      setOperation("idle")
    }
  }, [props.actions])

  return (
    <section className="settings-preferences-page settings-detail-panel settings-archive-page">
      <div className="settings-archive-toolbar">
        <span className="settings-muted-line">{formatMessage(t("settings.misc.selected"), { selectedCount })}</span>
        <div className="vk-form-inline">
          <Button
            disabled={selectedCount === 0 || operation !== "idle"}
            onClick={() => void runArchivedConversationAction("restore", Array.from(selectedConversationIds))}
            type="button"
          >
            <RotateCcw size={14} />
            <span>{operation === "restoring" ? t("settings.misc.restoring") : t("settings.misc.restoreSelected")}</span>
          </Button>
          <Button
            variant="destructive"
            disabled={selectedCount === 0 || operation !== "idle"}
            onClick={() => setDeleteTargetIds(Array.from(selectedConversationIds))}
            type="button"
          >
            <Trash2 size={14} />
            <span>{operation === "deleting" ? t("settings.misc.deleting") : t("settings.misc.deleteSelected")}</span>
          </Button>
        </div>
      </div>
      {actionError ? <SettingsErrorNotice detail={actionError} message={t("settings.misc.thatDidnTFinish")} /> : null}
      <AlertDialog onOpenChange={(open) => { if (!open) setDeleteTargetIds([]) }} open={deleteTargetIds.length > 0}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.misc.permanentlyDeleteArchivedConversations")}</AlertDialogTitle>
            <AlertDialogDescription>
              {formatMessage(t("settings.misc.permanentlyDeleteArchivedConversations2"), { length: deleteTargetIds.length })}
            </AlertDialogDescription>
            <ul className="max-h-40 overflow-auto text-sm">{deleteTargetIds.map((id) => <li key={id}>{archivedConversations.find((conversation) => conversation.id === id)?.title}</li>)}</ul>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings.misc.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={operation !== "idle" || deleteTargetIds.length === 0} onClick={() => void runArchivedConversationAction("delete", deleteTargetIds)} variant="destructive">
              {t("settings.misc.deletePermanently")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <div className="settings-archive-table-scroll">
        <table className="settings-archive-table">
          <caption className="sr-only">{t("settings.misc.archivedConversations")}</caption>
          <thead><tr>
            <th className="settings-archive-selection"><label className="inline-flex items-center gap-2 whitespace-nowrap">
            <Checkbox
              aria-label={t("settings.misc.selectAllArchivedConversations")}
              checked={allConversationsSelected ? true : selectedCount > 0 ? "indeterminate" : false}
              disabled={archivedConversations.length === 0 || operation !== "idle"}
              onCheckedChange={(checked) => toggleAllConversationSelection(checked === true)}
            />
              <span>{t("settings.misc.selectAll")}</span></label></th>
            <th>{t("settings.misc.conversation")}</th>
            <th>{t("settings.misc.project")}</th>
            <th>{t("settings.misc.lastUpdated")}</th>
            <th className="settings-archive-actions">{t("settings.misc.actions")}</th>
          </tr></thead>
          <tbody>{archivedConversations.length === 0 ? <tr><td colSpan={5} className="settings-archive-empty">{t("settings.misc.noArchivedConversations")}</td></tr>
            : visibleConversations.map((conversation) => <tr key={conversation.id} data-selected={selectedConversationIds.has(conversation.id)}>
              <td><Checkbox aria-label={formatMessage(t("settings.misc.selectConversation"), { title: conversation.title })}
                checked={selectedConversationIds.has(conversation.id)} disabled={operation !== "idle"}
                onCheckedChange={(checked) => toggleConversationSelection(conversation.id, checked === true)} /></td>
              <td className="settings-archive-title">{conversation.title}</td>
              <td>{conversation.projectId ? props.projects.find((project) => project.id === conversation.projectId)?.name ?? t("settings.misc.originalProjectUnavailable") : t("settings.misc.standaloneConversation")}</td>
              <td className="whitespace-nowrap text-muted-foreground">{conversation.lastUpdatedLabel}</td>
              <td><div className="vk-form-acts">
                <Button size="sm" variant="outline" aria-label={formatMessage(t("settings.misc.restoreConversation"), { title: conversation.title })} disabled={operation !== "idle"}
                  onClick={() => void runArchivedConversationAction("restore", [conversation.id])}><RotateCcw size={14} />{t("settings.misc.restore")}</Button>
                <Button size="sm" variant="ghost" className="vk-field-error" aria-label={formatMessage(t("settings.misc.deleteConversation"), { title: conversation.title })} disabled={operation !== "idle"}
                  onClick={() => setDeleteTargetIds([conversation.id])}><Trash2 size={14} />{t("settings.misc.delete")}</Button>
              </div></td>
            </tr>)}
          </tbody>
        </table>
      </div>
      {archivedConversations.length > 0 ? (
        <div className="settings-pagination">
          <span className="settings-muted-line">{formatMessage(t("settings.misc.pageOf"), { currentPage, pageCount })}</span>
          <div className="settings-card-actions">
            <span className="settings-muted-line">{currentPageSelected ? t("settings.misc.currentPageSelected") : t("settings.misc.selectionIsKeptAcross")}</span>
            <Button
              aria-label={t("settings.misc.previousPage")}
              variant="outline"
              size="icon-sm"
              disabled={currentPage <= 1 || operation !== "idle"}
              onClick={() => setCurrentPage((current) => Math.max(1, current - 1))}
              type="button"
            >
              <ChevronLeft size={14} />
            </Button>
            <Button
              aria-label={t("settings.misc.nextPage")}
              variant="outline"
              size="icon-sm"
              disabled={currentPage >= pageCount || operation !== "idle"}
              onClick={() => setCurrentPage((current) => Math.min(pageCount, current + 1))}
              type="button"
            >
              <ChevronRight size={14} />
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  )
}

