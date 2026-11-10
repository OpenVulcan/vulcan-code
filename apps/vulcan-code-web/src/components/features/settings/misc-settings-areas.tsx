import * as React from "react"
import {
  Archive,
  ChevronLeft,
  ChevronRight,
  LoaderCircle,
  RotateCcw,
  Trash2,
} from "lucide-react"

import { ModelPickerControl, modelPickerGroupKey } from "@/components/composer/model-picker-control"
import { PageActionBar, PageBody, PageEmptyState, PageRow, PageSection, PageTable, PageToolbar } from "@/components/page"
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
import { formatConversationRelativeTime, formatConversationUpdatedAt } from "@/components/project/project-sidebar-time"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import type { AnswerMode } from "@/service/answer-mode"
import { SettingsPreviewChoices } from "./settings-preview-choices"
import type {
  ConversationSummary,
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types"
import {
  DEFAULT_COMMIT_BRANCH_PREFIX,
  gitCommitSettingsDraft,
  gitCommitSettingsDraftKey,
} from "@/components/features/settings/git-commit-settings-draft"
import "@/ui-kit/styles/features/settings-forms.css"

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
  // persistedDraft records only fields accepted by the save contract, in canonical field order.
  // persistedDraft 仅记录保存契约接受的字段，字段顺序规范化。
  const persistedDraft = gitCommitSettingsDraft(props.workbench.gitCommit)
  // persistedKey detects changes by value, independent of snapshot identity and key order.
  // persistedKey 按值检测变化，不依赖快照对象身份与键顺序。
  const persistedKey = gitCommitSettingsDraftKey(persistedDraft)
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
    setDraft((current) => gitCommitSettingsDraftKey(current) === previousBaseline ? nextDraft : current)
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
  const dirty = gitCommitSettingsDraftKey(draft) !== persistedKey
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
  // saving locks the form while one immutable payload is persisted.
  // saving 在持久化一个不可变载荷期间锁定表单。
  const saving = saveState === "saving"
  return (
    <PageBody>
      <fieldset className="vk-fieldset" disabled={saving}>
        <PageSection layout="rows" title={t("settings.misc.whoWritesCommitMessages")}>
          <PageRow label={t("settings.gitCommit.summaryModel")} description={<>
            {draft.summaryModel.mode === "session" ? t("settings.gitCommit.followSessionModelHelp") : t("settings.gitCommit.fixedSummaryModelHelp")}
            {fixedModelValue && !modelPickerChoiceExists(props.workbench.providers, fixedModelValue) ? <><br /><span className="vk-field-error" role="status">{t("settings.gitCommit.fixedSummaryModelUnavailable")}</span></> : null}
          </>}>
            {draft.summaryModel.mode === "fixed" ? <Button
              onClick={() => setDraft({ ...draft, summaryModel: { mode: "session" } })}
              size="sm"
              type="button"
              variant="ghost"
            ><RotateCcw aria-hidden="true" />{t("settings.gitCommit.followSessionModel")}</Button> : null}
            <ModelPickerControl
              selectionScope="model"
              ariaLabel={t("settings.gitCommit.chooseSummaryModel")}
              fallbackLabel={t("settings.gitCommit.followSessionModel")}
              unavailableValueLabel={fixedModelLabel}
              filterPlaceholder={t("composer.modelPicker.filter")}
              noMatchesLabel={t("composer.modelPicker.noMatches")}
              providers={props.workbench.providers}
              value={fixedModelValue}
              disabled={saving}
              onValueChange={(choice) => setDraft({ ...draft, summaryModel: { mode: "fixed", providerId: choice.providerId, modelId: choice.modelId } })}
            />
          </PageRow>
        </PageSection>
        <PageSection layout="rows" title={t("settings.misc.howCommitMessagesAre")}>
          <PageRow htmlFor="git-commit-extra-prompt" label={t("settings.gitCommit.summaryExtraPrompt")} description={t("settings.gitCommit.summaryExtraPromptHelp")} stacked>
            <Textarea id="git-commit-extra-prompt" rows={4} value={draft.summaryExtraPrompt}
              placeholder={t("settings.gitCommit.summaryExtraPromptPlaceholder")}
              onChange={(event) => setDraft({ ...draft, summaryExtraPrompt: event.currentTarget.value })} />
          </PageRow>
        </PageSection>
        <PageSection layout="rows" title={t("settings.misc.branchNaming")}>
          <PageRow htmlFor="git-commit-branch-prefix" label={t("settings.misc.newBranchNamePrefix")} description={<>
            {t("settings.misc.onlyAffectsNewBranches")}
            <br />{t("settings.misc.example")}：<code>{normalizeGitCommitBranchPrefix(draft.newBranchPrefix)}feature-name</code>
          </>}>
            <Input className="vk-form-w-lg" id="git-commit-branch-prefix" value={draft.newBranchPrefix} placeholder={DEFAULT_COMMIT_BRANCH_PREFIX}
              onChange={(event) => setDraft({ ...draft, newBranchPrefix: event.currentTarget.value })} />
          </PageRow>
        </PageSection>
      </fieldset>
      {saveError ? <SettingsErrorNotice detail={saveError} message={t("settings.misc.couldnTSaveYour")} /> : null}
      {dirty || saving ? (
        <PageActionBar pending={dirty} actions={
          <Button disabled={saving || !dirty} onClick={() => void saveDraft()} size="sm" type="button">
            {saving ? <LoaderCircle className="vk-spin" aria-hidden="true" /> : null}
            {t(saving ? "settings.misc.saving" : "settings.misc.save")}
          </Button>
        }>{t("settings.misc.unsavedChanges")}</PageActionBar>
      ) : null}
    </PageBody>
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
  return <div className="vk-form-pref">
    <div className="vk-form-pref-text">
      <strong>{t("settings.misc.answerStyleForNew")}</strong>
      <p className="vk-form-note">{t("settings.misc.takesEffectForNew")}</p>
    </div>
      <Select disabled={saving} onValueChange={selectStyle} value={outputStyleId}>
        <SelectTrigger aria-label={t("settings.misc.answerStyleForNew")} className="vk-form-w-md"><SelectValue /></SelectTrigger>
        <SelectContent>{props.outputStyleCatalog.entries.map((entry) => (
          <SelectItem key={entry.id} value={entry.id}>{pickBilingualText(locale, entry.labelZh, entry.labelEn)}</SelectItem>
        ))}</SelectContent>
      </Select>
    {saving || saved ? <p className="vk-form-note" role="status">{saving ? t("settings.misc.saving2") : t("settings.misc.saved")}</p> : null}
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
  return <SettingsPreviewChoices
    label={t("answerMode.globalDefault")}
    description={t("settings.misc.takesEffectForNew")}
    value={mode}
    disabled={saving}
    onSelect={(next) => void selectMode(next)}
    options={[
      { value: "standard", label: t("answerMode.standard"), hint: t("answerMode.standardHint"), preview: <AnswerLayoutExample mode="standard" /> },
      { value: "interactive", label: t("answerMode.interactive"), hint: t("answerMode.interactiveHint"), preview: <AnswerLayoutExample mode="interactive" /> },
    ]}
  >
    {saving ? <p className="vk-form-note" role="status">{t("settings.misc.saving2")}</p> : null}
    {error ? <SettingsErrorNotice detail={error} message={t("answerMode.saveFailed")} /> : null}
  </SettingsPreviewChoices>
}

// Illustrate the two answer modes without rendering a live answer or any interactive component.
// 用静态图示对比两种回答模式，无需渲染实时回答或实际互动组件。
function AnswerLayoutExample({ mode }: { mode: AnswerMode }) {
  return <span className="session-defaults-scene">
    <span className="settings-preview-lines" data-mode={mode}><i /><i /><i /></span>
    {mode === "interactive" ? <span className="settings-preview-chart"><i /><i /><i /><i /></span> : null}
  </span>
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
  const { locale, t } = useI18n()
  const nowMs = Date.now()
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

  // Rows address their cells by column key; the selection column header carries the cross-page select-all box.
  // 行按列键寻址单元格；选择列表头放置跨页全选复选框。
  const projectName = (conversation: ConversationSummary) => conversation.projectId
    ? props.projects.find((project) => project.id === conversation.projectId)?.name ?? t("settings.misc.originalProjectUnavailable")
    : t("settings.misc.standaloneConversation")
  const rows = visibleConversations.map((conversation) => ({
    key: conversation.id,
    selected: selectedConversationIds.has(conversation.id),
    cells: {
      select: <Checkbox aria-label={formatMessage(t("settings.misc.selectConversation"), { title: conversation.title })}
        checked={selectedConversationIds.has(conversation.id)} disabled={operation !== "idle"}
        onCheckedChange={(checked) => toggleConversationSelection(conversation.id, checked === true)} />,
      title: conversation.title,
      project: <span className="vk-faint">{projectName(conversation)}</span>,
      updated: <span className="vk-faint" title={formatConversationUpdatedAt(conversation, locale) ?? undefined}>{formatConversationRelativeTime(conversation, nowMs, locale) || "—"}</span>,
      actions: <div className="vk-form-acts">
        <Button aria-label={formatMessage(t("settings.misc.restoreConversation"), { title: conversation.title })} disabled={operation !== "idle"}
          onClick={() => void runArchivedConversationAction("restore", [conversation.id])} size="icon-sm" title={t("settings.misc.restore")} variant="ghost"><RotateCcw /></Button>
        <Button aria-label={formatMessage(t("settings.misc.deleteConversation"), { title: conversation.title })} disabled={operation !== "idle"}
          onClick={() => setDeleteTargetIds([conversation.id])} size="icon-sm" title={t("settings.misc.delete")} variant="ghost"><Trash2 /></Button>
      </div>,
    },
  }))

  return (
    <PageBody>
      {archivedConversations.length === 0 ? (
        <PageEmptyState description={t("settings.misc.noArchivedConversationsHint")} icon={<Archive />} title={t("settings.misc.noArchivedConversations")} />
      ) : (
        <>
          <PageToolbar actions={<>
            <Button
              disabled={selectedCount === 0 || operation !== "idle"}
              onClick={() => void runArchivedConversationAction("restore", Array.from(selectedConversationIds))}
              size="sm"
              type="button"
              variant="outline"
            >
              {operation === "restoring" ? <LoaderCircle className="vk-spin" /> : <RotateCcw />}
              <span>{operation === "restoring" ? t("settings.misc.restoring") : t("settings.misc.restoreSelected")}</span>
            </Button>
            <Button
              disabled={selectedCount === 0 || operation !== "idle"}
              onClick={() => setDeleteTargetIds(Array.from(selectedConversationIds))}
              size="sm"
              type="button"
              variant="destructive"
            >
              <Trash2 />
              <span>{operation === "deleting" ? t("settings.misc.deleting") : t("settings.misc.deleteSelected")}</span>
            </Button>
          </>}>
            <span className="vk-form-note" role="status">{formatMessage(t("settings.misc.selected"), { selectedCount })}</span>
          </PageToolbar>
          {actionError ? <SettingsErrorNotice detail={actionError} message={t("settings.misc.thatDidnTFinish")} /> : null}
          <PageTable
            columns={[
              { key: "select", label: <Checkbox
                aria-label={t("settings.misc.selectAllArchivedConversations")}
                checked={allConversationsSelected ? true : selectedCount > 0 ? "indeterminate" : false}
                disabled={operation !== "idle"}
                onCheckedChange={(checked) => toggleAllConversationSelection(checked === true)}
              />, width: "40px" },
              { key: "title", label: t("settings.misc.conversation"), wrap: true },
              { key: "project", label: t("settings.misc.project"), width: "22%" },
              { key: "updated", label: t("settings.misc.lastUpdated"), width: "18%" },
              { key: "actions", label: <span className="vk-sr-only">{t("settings.misc.actions")}</span>, width: "84px" },
            ]}
            label={t("settings.misc.archivedConversations")}
            layout="fixed"
            rows={rows}
          />
          <div className="settings-pagination">
            <span className="vk-form-note">{formatMessage(t("settings.misc.pageOf"), { currentPage, pageCount })}</span>
            <div className="vk-form-inline">
              <span className="vk-form-note">{currentPageSelected ? t("settings.misc.currentPageSelected") : t("settings.misc.selectionIsKeptAcross")}</span>
              <Button
                aria-label={t("settings.misc.previousPage")}
                variant="ghost"
                size="icon-sm"
                disabled={currentPage <= 1 || operation !== "idle"}
                onClick={() => setCurrentPage((current) => Math.max(1, current - 1))}
                type="button"
              >
                <ChevronLeft />
              </Button>
              <Button
                aria-label={t("settings.misc.nextPage")}
                variant="ghost"
                size="icon-sm"
                disabled={currentPage >= pageCount || operation !== "idle"}
                onClick={() => setCurrentPage((current) => Math.min(pageCount, current + 1))}
                type="button"
              >
                <ChevronRight />
              </Button>
            </div>
          </div>
        </>
      )}
      <AlertDialog onOpenChange={(open) => { if (!open) setDeleteTargetIds([]) }} open={deleteTargetIds.length > 0}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.misc.permanentlyDeleteArchivedConversations")}</AlertDialogTitle>
            <AlertDialogDescription>
              {formatMessage(t("settings.misc.permanentlyDeleteArchivedConversations2"), { length: deleteTargetIds.length })}
            </AlertDialogDescription>
            <ul className="vk-form-bullets vk-form-scroll-list">{deleteTargetIds.map((id) => <li key={id}>{archivedConversations.find((conversation) => conversation.id === id)?.title}</li>)}</ul>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("settings.misc.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={operation !== "idle" || deleteTargetIds.length === 0} onClick={() => void runArchivedConversationAction("delete", deleteTargetIds)} variant="destructive">
              {t("settings.misc.deletePermanently")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageBody>
  )
}

