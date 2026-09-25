import * as React from "react"
import { Pencil, Plus, Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type {
  AgentSkillCategorySaveRequest,
  AgentSkillCategorySummary,
  AgentSkillLocaleSummary,
  AgentSkillRecordSummary,
  AgentSkillSaveRequest,
} from "@/types"
import { useI18n, formatMessage, type Translate } from "@/i18n"
import { AGENT_SKILL_DEFAULT_COLOR, colorInputValue } from "@/lib/agent-color"

// AgentSkillEditorDraft stores structured package fields and the directly editable prompt body.
// AgentSkillEditorDraft 保存结构化技能包字段与可直接编辑的提示词正文。
interface AgentSkillEditorDraft {
  name: string
  description: string
  license: string
  compatibility: string
  metadata: Record<string, string>
  body: string
  displayName: string
  shortDescription: string
  emoji: string
  color: string
  locales: Record<string, AgentSkillLocaleSummary>
  categoryId: string
  tags: string[]
  enabled: boolean
}

// AgentSkillCategoryDraft stores one user-defined logical category form.
// AgentSkillCategoryDraft 保存一个用户定义逻辑分类表单。
interface AgentSkillCategoryDraft {
  categoryId: string
  name: string
  description: string
  emoji: string
  color: string
  order: string
  englishName: string
  englishDescription: string
  chineseName: string
  chineseDescription: string
}

// AgentSkillEditorDialog edits a package through synchronized structured fields and source documents.
// AgentSkillEditorDialog 通过同步的结构字段与源码文档编辑技能包。
export function AgentSkillEditorDialog(props: {
  categories: AgentSkillCategorySummary[]
  locale: string
  open: boolean
  copy: boolean
  record: AgentSkillRecordSummary | null
  onOpenChange: (open: boolean) => void
  onSave: (request: AgentSkillSaveRequest) => Promise<void>
}) {
  const { t } = useI18n();
  // draft is the single source of truth for the current editor session.
  // draft 是当前编辑会话的唯一事实来源。
  const [draft, setDraft] = React.useState<AgentSkillEditorDraft>(() => createEditorDraft(props.record, props.categories, props.copy))
  // error exposes parsing, validation, or persistence failures.
  // error 暴露解析、校验或持久化失败。
  const [error, setError] = React.useState("")
  // saving prevents concurrent package submissions.
  // saving 防止并发提交技能包。
  const [saving, setSaving] = React.useState(false)
  // savingRef excludes duplicate submissions and dismissals before React commits busy state.
  // savingRef 在 React 提交忙碌状态前排除重复提交和关闭。
  const savingRef = React.useRef(false)
  // editorSession preserves an unsaved draft when a background catalog refresh replaces props.
  // editorSession 在后台目录刷新替换属性时保留未保存草稿。
  const editorSession = React.useRef<string | null>(null)

  React.useEffect(() => {
    const session = props.open ? `${props.copy}:${props.record?.baseDir ?? "new"}` : null
    if (session !== null && editorSession.current !== session) {
      setDraft(createEditorDraft(props.record, props.categories, props.copy))
      setError("")
    }
    editorSession.current = session
  }, [props.categories, props.copy, props.open, props.record])

  // updateStructuredField changes one host-managed package node.
  // updateStructuredField 修改一个由宿主管理的技能包节点。
  const updateStructuredField = React.useCallback(<Key extends keyof AgentSkillEditorDraft>(key: Key, value: AgentSkillEditorDraft[Key]) => {
    setDraft((current) => ({ ...current, [key]: value }))
    setError("")
  }, [])

  // updateLocaleField changes one exact BCP 47 display node.
  // updateLocaleField 修改一个精确 BCP 47 展示节点。
  const updateLocaleField = React.useCallback((locale: string, key: keyof AgentSkillLocaleSummary, value: string) => {
    setDraft((current) => {
      const locales = { ...current.locales }
      const metadata = { ...locales[locale], [key]: value || null }
      if (!metadata.displayName && !metadata.shortDescription) {
        delete locales[locale]
      } else {
        locales[locale] = metadata
      }
      return { ...current, locales }
    })
    setError("")
  }, [])

  // submit builds the strict service payload; the host generates SKILL.md frontmatter and vulcan.yaml.
  // submit 构建严格服务载荷；SKILL.md frontmatter 与 vulcan.yaml 由宿主生成。
  const submit = React.useCallback(async () => {
    if (savingRef.current) return
    try {
      const request = createSkillSaveRequest(draft, props.record, props.copy, t)
      savingRef.current = true
      setSaving(true)
      setError("")
      await props.onSave(request)
      props.onOpenChange(false)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }, [draft, props])

  // chinese controls the leading user-facing copy.
  // chinese 控制优先展示的用户文案。
  const chinese = props.locale.startsWith("zh")
  // recordDisplayName is the localized dialog title baseline.
  // recordDisplayName 是本地化弹窗标题基线。
  const recordDisplayName = props.record ? localizedRecordDisplayName(props.record, props.locale) : ""
  return (
    <Dialog onOpenChange={(open) => { if (!savingRef.current) props.onOpenChange(open) }} open={props.open}>
      <DialogContent className="agent-skill-editor-dialog" size="xl" scrollMode="fixed-footer" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>
            {props.copy && props.record
              ? (formatMessage(t("agentSkills.copy2"), { recordDisplayName }))
              : props.record
                ? (formatMessage(t("agentSkills.edit2"), { recordDisplayName }))
                : t("agentSkills.createSkill2")}
          </DialogTitle>
          <DialogDescription>
            {t("agentSkills.theCanonicalNameEquals")}
          </DialogDescription>
        </DialogHeader>
        <div className="agent-skill-editor-body">
          <fieldset disabled={saving}>
            <legend>{t("agentSkills.packageFields")}</legend>
            <div className="agent-skill-form-grid">
              <AgentSkillTextField label="name" onChange={(value) => updateStructuredField("name", value)} required value={draft.name} />
              <AgentSkillTextField label={t("agentSkills.license2")} onChange={(value) => updateStructuredField("license", value)} value={draft.license} />
              <AgentSkillTextArea className="agent-skill-form-span" label={t("agentSkills.modelTriggerDescription")} onChange={(value) => updateStructuredField("description", value)} required rows={3} value={draft.description} />
              <AgentSkillTextArea className="agent-skill-form-span" label={t("agentSkills.compatibility2")} onChange={(value) => updateStructuredField("compatibility", value)} rows={2} value={draft.compatibility} />
              <AgentSkillTextField label={t("agentSkills.tagsCommaSeparated")} onChange={(value) => updateStructuredField("tags", uniqueCommaSeparatedValues(value))} value={draft.tags.join(", ")} />
            </div>
          </fieldset>
          <fieldset disabled={saving}>
            <legend>{t("agentSkills.presentationAndCategory")}</legend>
            <div className="agent-skill-form-grid">
              <AgentSkillTextField label={t("agentSkills.defaultDisplayName")} onChange={(value) => updateStructuredField("displayName", value)} value={draft.displayName} />
              <AgentSkillTextField label={t("agentSkills.defaultShortDescription")} onChange={(value) => updateStructuredField("shortDescription", value)} value={draft.shortDescription} />
              <AgentSkillTextField label={t("agentSkills.emoji")} onChange={(value) => updateStructuredField("emoji", value)} value={draft.emoji} />
              <label className="agent-skill-form-field">
                <span>{t("agentSkills.colorMarker")}</span>
                <AgentSkillColorPicker disabled={saving} locale={props.locale} onChange={(value) => updateStructuredField("color", value)} value={draft.color} />
              </label>
              <label className="agent-skill-form-field">
                <span>{t("agentSkills.skillCategory")}</span>
                <Select disabled={saving} onValueChange={(value) => updateStructuredField("categoryId", value)} value={draft.categoryId}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {props.categories.map((category) => <SelectItem key={category.categoryId} value={category.categoryId}>{category.emoji} {localizedCategoryName(category, props.locale)}</SelectItem>)}
                  </SelectContent>
                </Select>
              </label>
              <label className="agent-skill-form-check">
                <Switch disabled={saving} checked={draft.enabled} onCheckedChange={(checked) => updateStructuredField("enabled", checked)} />
                <span>{t("agentSkills.enableThisSkill")}</span>
              </label>
              <AgentSkillTextField label="English name" onChange={(value) => updateLocaleField("en-US", "displayName", value)} value={draft.locales["en-US"]?.displayName ?? ""} />
              <AgentSkillTextField label="English description" onChange={(value) => updateLocaleField("en-US", "shortDescription", value)} value={draft.locales["en-US"]?.shortDescription ?? ""} />
              <AgentSkillTextField label="中文名称" onChange={(value) => updateLocaleField("zh-CN", "displayName", value)} value={draft.locales["zh-CN"]?.displayName ?? ""} />
              <AgentSkillTextField label="中文说明" onChange={(value) => updateLocaleField("zh-CN", "shortDescription", value)} value={draft.locales["zh-CN"]?.shortDescription ?? ""} />
            </div>
          </fieldset>
          <fieldset disabled={saving} className="agent-skill-source-fieldset">
            <legend>{t("agentSkills.promptContent")}</legend>
            <AgentSkillTextArea className="agent-skill-source-editor" label={t("agentSkills.skillMdPromptBody")} onChange={(value) => updateStructuredField("body", value)} required rows={22} value={draft.body} />
          </fieldset>
          {error ? <p className="agent-skill-form-error" role="alert">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button disabled={saving} onClick={() => props.onOpenChange(false)} variant="outline">{t("agentSkills.cancel")}</Button>
          <Button disabled={saving} onClick={() => void submit()}>{saving ? t("agentSkills.saving") : t("agentSkills.saveSkill")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// AgentSkillCategoryManagementDialog manages the logical category table in one top-level panel.
// AgentSkillCategoryManagementDialog 在一个顶层面板中管理逻辑分类表。
export function AgentSkillCategoryManagementDialog(props: {
  categories: AgentSkillCategorySummary[]
  locale: string
  open: boolean
  onOpenChange: (open: boolean) => void
  onSave: (request: AgentSkillCategorySaveRequest) => Promise<void>
  onDelete: (categoryId: string) => Promise<void>
}) {
  const { t } = useI18n();
  // editing selects the user category form; undefined shows only the category table.
  // editing 选择用户分类表单；undefined 时仅显示分类表。
  const [editing, setEditing] = React.useState<AgentSkillCategorySummary | null | undefined>(undefined)
  // draft stores the active user category form.
  // draft 保存当前用户分类表单。
  const [draft, setDraft] = React.useState<AgentSkillCategoryDraft>(() => createCategoryDraft(null))
  // error stores category validation or service failures.
  // error 保存分类校验或服务失败。
  const [error, setError] = React.useState("")
  // saving serializes category writes and deletes.
  // saving 串行化分类写入与删除。
  const [saving, setSaving] = React.useState(false)
  // savingRef synchronously serializes category writes and protects the active dialog session.
  // savingRef 同步串行化分类写入，并保护活动弹窗会话。
  const savingRef = React.useRef(false)

  React.useEffect(() => {
    if (props.open) {
      setPendingDelete(null)
      setEditing(undefined)
      setError("")
    }
  }, [props.open])

  // pendingDelete holds the exact category whose removal is awaiting the shared confirmation dialog.
  // pendingDelete 保存等待共享确认弹窗确认删除的精确分类。
  const [pendingDelete, setPendingDelete] = React.useState<AgentSkillCategorySummary | null>(null)

  // openForm initializes the exact selected category or a new user category.
  // openForm 初始化精确选中的分类或新用户分类。
  const openForm = React.useCallback((category: AgentSkillCategorySummary | null) => {
    setEditing(category)
    setDraft(createCategoryDraft(category))
    setError("")
  }, [])

  // updateCategoryField replaces one category form field.
  // updateCategoryField 替换一个分类表单字段。
  const updateCategoryField = React.useCallback(<Key extends keyof AgentSkillCategoryDraft>(key: Key, value: AgentSkillCategoryDraft[Key]) => {
    setDraft((current) => ({ ...current, [key]: value }))
  }, [])

  // saveCategory validates and persists one user-owned category row.
  // saveCategory 校验并持久化一条用户所有的分类记录。
  const saveCategory = React.useCallback(async () => {
    if (savingRef.current) return
    savingRef.current = true
    try {
      setSaving(true)
      setError("")
      await props.onSave(createCategorySaveRequest(draft, t))
      setEditing(undefined)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }, [draft, props])

  // deleteCategory confirms and deletes one user-owned category row.
  // deleteCategory 确认并删除一条用户所有的分类记录。
  const deleteCategory = React.useCallback(async (category: AgentSkillCategorySummary) => {
    if (savingRef.current) return
    try {
      savingRef.current = true
      setSaving(true)
      setError("")
      await props.onDelete(category.categoryId)
      setPendingDelete(null)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }, [props])

  // chinese controls the leading user-facing copy.
  // chinese 控制优先展示的用户文案。
  const chinese = props.locale.startsWith("zh")
  return (
    <Dialog onOpenChange={(open) => { if (!savingRef.current) props.onOpenChange(open) }} open={props.open}>
      <DialogContent className="agent-skill-category-management-dialog" size="lg" scrollMode="fixed-footer" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{t("agentSkills.categoryManagement")}</DialogTitle>
          <DialogDescription>{t("agentSkills.systemCategoriesAreImmutable")}</DialogDescription>
        </DialogHeader>
        <div className="min-h-0 overflow-y-auto">
        {editing === undefined ? (
          <div className="grid gap-2">
            <div className="agent-skill-category-management-actions">
              <Button disabled={saving} onClick={() => openForm(null)}><Plus size={15} />{t("agentSkills.addCategory")}</Button>
            </div>
            <div className="grid gap-2">
              {props.categories.map((category) => (
                <div className="agent-skill-category-management-row" key={category.categoryId}>
                  <span className="agent-skill-category-management-marker" style={{ backgroundColor: category.color }}>{category.emoji}</span>
                  <div><strong>{localizedCategoryName(category, props.locale)}</strong><span>{category.categoryId}</span></div>
                  <span>{category.builtin ? t("agentSkills.system2") : t("agentSkills.added")}</span>
                  {!category.builtin ? (
                    <div>
                      <Button variant="ghost" size="icon-sm" disabled={saving} aria-label={t("agentSkills.editCategory")} onClick={() => openForm(category)} type="button"><Pencil size={15} /></Button>
                      <Button variant="ghost" size="icon-sm" disabled={saving} aria-label={t("agentSkills.deleteCategory")} onClick={() => { setError(""); setPendingDelete(category) }} type="button"><Trash2 size={15} /></Button>
                    </div>
                  ) : null}
                </div>
              ))}
            </div>
          </div>
        ) : (
          <fieldset disabled={saving} className="agent-skill-form-grid min-w-0 border-0 p-0">
            <AgentSkillTextField disabled={editing !== null} label="category-id" onChange={(value) => updateCategoryField("categoryId", value)} required value={draft.categoryId} />
            <AgentSkillTextField label={t("agentSkills.defaultName")} onChange={(value) => updateCategoryField("name", value)} required value={draft.name} />
            <AgentSkillTextField label={t("agentSkills.emoji")} onChange={(value) => updateCategoryField("emoji", value)} required value={draft.emoji} />
            <label className="agent-skill-form-field">
              <span>{t("agentSkills.colorMarker")}</span>
              <AgentSkillColorPicker disabled={saving} locale={props.locale} onChange={(value) => updateCategoryField("color", value)} value={draft.color} />
            </label>
            <AgentSkillTextField label={t("agentSkills.order")} onChange={(value) => updateCategoryField("order", value)} required type="number" value={draft.order} />
            <AgentSkillTextField label={t("agentSkills.defaultDescription")} onChange={(value) => updateCategoryField("description", value)} value={draft.description} />
            <AgentSkillTextField label="English name" onChange={(value) => updateCategoryField("englishName", value)} required value={draft.englishName} />
            <AgentSkillTextField label="English description" onChange={(value) => updateCategoryField("englishDescription", value)} value={draft.englishDescription} />
            <AgentSkillTextField label="中文名称" onChange={(value) => updateCategoryField("chineseName", value)} required value={draft.chineseName} />
            <AgentSkillTextField label="中文说明" onChange={(value) => updateCategoryField("chineseDescription", value)} value={draft.chineseDescription} />
          </fieldset>
        )}
        {error ? <p className="agent-skill-form-error" role="alert">{error}</p> : null}
        </div>
        <DialogFooter>
          {editing !== undefined ? <Button disabled={saving} onClick={() => setEditing(undefined)} variant="outline">{t("agentSkills.back")}</Button> : null}
          <Button disabled={saving} onClick={() => props.onOpenChange(false)} variant="outline">{t("agentSkills.close")}</Button>
          {editing !== undefined ? <Button disabled={saving} onClick={() => void saveCategory()}>{t("agentSkills.saveCategory")}</Button> : null}
        </DialogFooter>
        <AlertDialog open={pendingDelete !== null} onOpenChange={(open) => { if (!open && !savingRef.current) setPendingDelete(null) }}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("agentSkills.deleteCategory")}</AlertDialogTitle>
              <AlertDialogDescription>{pendingDelete ? (formatMessage(t("agentSkills.deleteCategory2"), { value: localizedCategoryName(pendingDelete, props.locale) })) : ""}</AlertDialogDescription>
            </AlertDialogHeader>
            {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
            <AlertDialogFooter>
              <AlertDialogCancel disabled={saving}>{t("agentSkills.cancel")}</AlertDialogCancel>
              <AlertDialogAction disabled={saving} onClick={(event) => { event.preventDefault(); if (pendingDelete) void deleteCategory(pendingDelete) }}>{saving ? t("agentSkills.deleting") : t("agentSkills.delete")}</AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  )
}

// AgentSkillTextField renders one compact controlled text field.
// AgentSkillTextField 渲染一个紧凑受控文本字段。
function AgentSkillTextField(props: { disabled?: boolean; label: string; onChange: (value: string) => void; required?: boolean; type?: React.HTMLInputTypeAttribute; value: string }) {
  return <label className="agent-skill-form-field"><span>{props.label}{props.required ? " *" : ""}</span><Input disabled={props.disabled} onChange={(event) => props.onChange(event.target.value)} required={props.required} type={props.type ?? "text"} value={props.value} /></label>
}

// AgentSkillTextArea renders one controlled multiline prompt or metadata field.
// AgentSkillTextArea 渲染一个受控多行提示词或元数据字段。
function AgentSkillTextArea(props: { className?: string; label: string; onChange: (value: string) => void; required?: boolean; rows: number; value: string }) {
  return <label className={`agent-skill-form-field ${props.className ?? ""}`}><span>{props.label}{props.required ? " *" : ""}</span><Textarea density={props.rows >= 8 ? "large" : "default"} onChange={(event) => props.onChange(event.target.value)} required={props.required} rows={props.rows} spellCheck={false} value={props.value} /></label>
}

// AgentSkillColorPicker renders the same native color selector used by the agent editor.
// AgentSkillColorPicker 渲染与智能体编辑器相同的原生颜色选择器。
function AgentSkillColorPicker(props: {
  // Whether the picker is disabled.
  // 选择器是否禁用。
  disabled: boolean
  // Current UI locale.
  // 当前界面语言区域。
  locale: string
  // Commits the selected hexadecimal color.
  // 提交选中的十六进制颜色。
  onChange: (value: string) => void
  // Current color value.
  // 当前颜色值。
  value: string
}) {
  const { t } = useI18n();
  const inputRef = React.useRef<HTMLInputElement | null>(null)
  const dirtyRef = React.useRef(false)
  const onChange = props.onChange
  const normalizedPropColor = colorInputValue(props.value)
  // draftColor keeps palette dragging local until the browser confirms the selection.
  // draftColor 在浏览器确认选择前将调色板拖动保持在本地。
  const [draftColor, setDraftColor] = React.useState(normalizedPropColor)

  React.useEffect(() => {
    setDraftColor(normalizedPropColor)
  }, [normalizedPropColor])

  React.useEffect(() => {
    const input = inputRef.current
    if (!input) return
    // handleNativeChange commits only the browser's final native change event.
    // handleNativeChange 仅提交浏览器最终的原生 change 事件。
    const handleNativeChange = () => {
      if (!dirtyRef.current) return
      dirtyRef.current = false
      onChange(input.value)
    }
    input.addEventListener("change", handleNativeChange)
    return () => input.removeEventListener("change", handleNativeChange)
  }, [onChange])

  // commitDraftColor commits a pending selection when focus leaves the native picker.
  // commitDraftColor 在原生选择器失去焦点时提交待处理选择。
  const commitDraftColor = React.useCallback(() => {
    if (!dirtyRef.current) return
    dirtyRef.current = false
    onChange(inputRef.current?.value ?? draftColor)
  }, [draftColor, onChange])

  const chooseColorLabel = t("agentSkills.chooseColor")
  return (
    <div className="agent-editor-color-picker">
      <label className={`agent-editor-color-trigger ${props.disabled ? "disabled" : ""}`}>
        <span className="agent-editor-color-swatch" style={{ backgroundColor: draftColor }} />
        <span>{draftColor || chooseColorLabel}</span>
        <input
          aria-label={chooseColorLabel}
          className="agent-editor-color-input"
          disabled={props.disabled}
          onBlur={commitDraftColor}
          onInput={(event) => {
            dirtyRef.current = true
            setDraftColor(event.currentTarget.value)
          }}
          ref={inputRef}
          type="color"
          value={draftColor}
        />
      </label>
    </div>
  )
}

// createEditorDraft builds one exact structured baseline and prompt body.
// createEditorDraft 构建一个精确的结构化基线与提示词正文。
function createEditorDraft(record: AgentSkillRecordSummary | null, categories: AgentSkillCategorySummary[], copy: boolean): AgentSkillEditorDraft {
  const defaultCategoryId = categories.find((category) => category.categoryId === "uncategorized")?.categoryId ?? categories[0]?.categoryId ?? "uncategorized"
  return {
    name: record ? (copy ? `${record.name}-copy` : record.name) : "new-skill",
    description: record?.standard.description ?? "Describe when this skill should be used.",
    license: record?.standard.license ?? "",
    compatibility: record?.standard.compatibility ?? "",
    metadata: record?.standard.metadata ?? {},
    body: record?.body ?? "# Instructions\n\nDescribe the workflow, constraints, and expected result.\n",
    displayName: record?.displayName ?? "New Skill",
    shortDescription: record?.description ?? "Reusable prompt skill",
    emoji: record?.emoji ?? "✨",
    color: record?.color ?? AGENT_SKILL_DEFAULT_COLOR,
    locales: record?.locales ?? {},
    categoryId: record?.categoryId ?? defaultCategoryId,
    tags: record?.tags ?? [],
    enabled: record?.enabled ?? true,
  }
}

// createSkillSaveRequest validates one synchronized draft and builds the strict service payload.
// createSkillSaveRequest 校验一个同步草稿并构建严格服务载荷。
function createSkillSaveRequest(draft: AgentSkillEditorDraft, record: AgentSkillRecordSummary | null, copy: boolean, t: Translate): AgentSkillSaveRequest {
  if (!/^(?!-)(?!.*--)[a-z0-9-]{1,64}(?<!-)$/u.test(draft.name)) throw new Error(t("agentSkills.nameMustBeA"))
  if (!draft.description.trim() || Array.from(draft.description).length > 1024) throw new Error(t("agentSkills.theModelTriggerDescription"))
  if (Array.from(draft.compatibility).length > 500) throw new Error(t("agentSkills.compatibilityCannotExceed500"))
  if (draft.color && !/^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/iu.test(draft.color)) throw new Error(t("agentSkills.colorMustBeHexadecimal"))
  return {
    originalName: !copy && record?.scopeKey === "user" ? record.name : null,
    sourceSkillKey: record?.skillKey ?? null,
    name: draft.name,
    description: draft.description.trim(),
    license: optionalText(draft.license),
    compatibility: optionalText(draft.compatibility),
    metadata: draft.metadata,
    body: draft.body,
    interface: {
      displayName: optionalText(draft.displayName),
      shortDescription: optionalText(draft.shortDescription),
      emoji: optionalText(draft.emoji),
      color: optionalText(draft.color),
      locales: draft.locales,
    },
    categoryId: draft.categoryId,
    tags: draft.tags,
    enabled: draft.enabled,
  }
}

// createCategoryDraft builds one exact category form baseline.
// createCategoryDraft 构建一个精确分类表单基线。
function createCategoryDraft(category: AgentSkillCategorySummary | null): AgentSkillCategoryDraft {
  return {
    categoryId: category?.categoryId ?? "",
    name: category?.name ?? "",
    description: category?.description ?? "",
    emoji: category?.emoji ?? "📁",
    color: category?.color ?? AGENT_SKILL_DEFAULT_COLOR,
    order: String(category?.order ?? 100),
    englishName: category?.locales["en-US"]?.name ?? "",
    englishDescription: category?.locales["en-US"]?.description ?? "",
    chineseName: category?.locales["zh-CN"]?.name ?? "",
    chineseDescription: category?.locales["zh-CN"]?.description ?? "",
  }
}

// createCategorySaveRequest validates and builds one strict category payload.
// createCategorySaveRequest 校验并构建一个严格分类载荷。
function createCategorySaveRequest(draft: AgentSkillCategoryDraft, t: Translate): AgentSkillCategorySaveRequest {
  if (!/^(?!-)(?!.*--)[a-z0-9-]{1,64}(?<!-)$/u.test(draft.categoryId)) throw new Error(t("agentSkills.categoryIdMustBe"))
  const order = Number(draft.order)
  if (!Number.isSafeInteger(order)) throw new Error(t("agentSkills.categoryOrderMustBe"))
  if (!draft.name.trim() || !draft.englishName.trim() || !draft.chineseName.trim()) throw new Error(t("agentSkills.defaultNameEnglishName"))
  if (!draft.emoji.trim() || !/^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/iu.test(draft.color.trim())) throw new Error(t("agentSkills.emojiIsRequiredAnd"))
  return {
    categoryId: draft.categoryId,
    name: draft.name.trim(),
    description: optionalText(draft.description),
    emoji: draft.emoji.trim(),
    color: draft.color.trim(),
    order,
    locales: {
      "en-US": { name: draft.englishName.trim(), description: optionalText(draft.englishDescription) },
      "zh-CN": { name: draft.chineseName.trim(), description: optionalText(draft.chineseDescription) },
    },
  }
}

// localizedRecordDisplayName resolves an Agent Skill title for the exact interface locale.
// localizedRecordDisplayName 为精确界面语言解析 Agent Skill 标题。
function localizedRecordDisplayName(record: AgentSkillRecordSummary, locale: string): string {
  const localized = record.locales[locale] ?? Object.entries(record.locales).find(([candidate]) => candidate.split("-")[0] === locale.split("-")[0])?.[1]
  return localized?.displayName?.trim() || record.displayName || record.name
}

// localizedCategoryName resolves exact then language-prefix category metadata.
// localizedCategoryName 按精确语言再语言前缀解析分类元数据。
function localizedCategoryName(category: AgentSkillCategorySummary, locale: string): string {
  return category.locales[locale]?.name ?? Object.entries(category.locales).find(([candidate]) => candidate.split("-")[0] === locale.split("-")[0])?.[1].name ?? category.name
}

// uniqueCommaSeparatedValues returns stable trimmed unique tags.
// uniqueCommaSeparatedValues 返回稳定裁剪去重标签。
function uniqueCommaSeparatedValues(raw: string): string[] {
  return [...new Set(raw.split(",").map((value) => value.trim()).filter(Boolean))]
}

// optionalText converts blank editor text into an absent protocol field.
// optionalText 将空白编辑器文本转换为缺失协议字段。
function optionalText(value: string): string | null {
  return value.trim() || null
}
