import * as React from "react"
import { Pencil, Plus, Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
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
  // draft is the single source of truth for the current editor session.
  // draft 是当前编辑会话的唯一事实来源。
  const [draft, setDraft] = React.useState<AgentSkillEditorDraft>(() => createEditorDraft(props.record, props.categories, props.copy))
  // error exposes parsing, validation, or persistence failures.
  // error 暴露解析、校验或持久化失败。
  const [error, setError] = React.useState("")
  // saving prevents concurrent package submissions.
  // saving 防止并发提交技能包。
  const [saving, setSaving] = React.useState(false)

  React.useEffect(() => {
    if (props.open) {
      setDraft(createEditorDraft(props.record, props.categories, props.copy))
      setError("")
    }
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
    try {
      const request = createSkillSaveRequest(draft, props.record, props.copy, props.locale.startsWith("zh"))
      setSaving(true)
      setError("")
      await props.onSave(request)
      props.onOpenChange(false)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
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
    <Dialog onOpenChange={props.onOpenChange} open={props.open}>
      <DialogContent className="agent-skill-editor-dialog" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>
            {props.copy && props.record
              ? (chinese ? `复制 ${recordDisplayName}` : `Copy ${recordDisplayName}`)
              : props.record
                ? (chinese ? `编辑 ${recordDisplayName}` : `Edit ${recordDisplayName}`)
                : (chinese ? "新建提示词技能" : "Create Agent Skill")}
          </DialogTitle>
          <DialogDescription>
            {chinese
              ? "技能名称等于技能目录名；可直接粘贴提示词正文，SKILL.md frontmatter 与 vulcan.yaml 由系统自动生成。"
              : "The canonical name equals the package directory; paste prompt content directly and the system generates SKILL.md frontmatter and vulcan.yaml."}
          </DialogDescription>
        </DialogHeader>
        <div className="agent-skill-editor-body">
          <fieldset>
            <legend>{chinese ? "技能包字段" : "Package fields"}</legend>
            <div className="agent-skill-form-grid">
              <AgentSkillTextField label="name" onChange={(value) => updateStructuredField("name", value)} required value={draft.name} />
              <AgentSkillTextField label={chinese ? "协议" : "License"} onChange={(value) => updateStructuredField("license", value)} value={draft.license} />
              <AgentSkillTextArea className="agent-skill-form-span" label={chinese ? "模型触发说明" : "Model trigger description"} onChange={(value) => updateStructuredField("description", value)} required rows={3} value={draft.description} />
              <AgentSkillTextArea className="agent-skill-form-span" label={chinese ? "兼容环境" : "Compatibility"} onChange={(value) => updateStructuredField("compatibility", value)} rows={2} value={draft.compatibility} />
              <AgentSkillTextField label={chinese ? "标签（逗号分隔）" : "Tags (comma separated)"} onChange={(value) => updateStructuredField("tags", uniqueCommaSeparatedValues(value))} value={draft.tags.join(", ")} />
            </div>
          </fieldset>
          <fieldset>
            <legend>{chinese ? "显示与分类" : "Presentation and category"}</legend>
            <div className="agent-skill-form-grid">
              <AgentSkillTextField label={chinese ? "默认显示名称" : "Default display name"} onChange={(value) => updateStructuredField("displayName", value)} value={draft.displayName} />
              <AgentSkillTextField label={chinese ? "默认简短说明" : "Default short description"} onChange={(value) => updateStructuredField("shortDescription", value)} value={draft.shortDescription} />
              <AgentSkillTextField label={chinese ? "图标" : "Emoji"} onChange={(value) => updateStructuredField("emoji", value)} value={draft.emoji} />
              <label className="agent-skill-form-field">
                <span>{chinese ? "颜色标记" : "Color marker"}</span>
                <AgentSkillColorPicker disabled={saving} locale={props.locale} onChange={(value) => updateStructuredField("color", value)} value={draft.color} />
              </label>
              <label className="agent-skill-form-field">
                <span>{chinese ? "技能分类" : "Skill category"}</span>
                <select onChange={(event) => updateStructuredField("categoryId", event.target.value)} value={draft.categoryId}>
                  {props.categories.map((category) => <option key={category.categoryId} value={category.categoryId}>{category.emoji} {localizedCategoryName(category, props.locale)}</option>)}
                </select>
              </label>
              <label className="agent-skill-form-check">
                <input checked={draft.enabled} onChange={(event) => updateStructuredField("enabled", event.target.checked)} type="checkbox" />
                <span>{chinese ? "启用此技能" : "Enable this skill"}</span>
              </label>
              <AgentSkillTextField label="English name" onChange={(value) => updateLocaleField("en-US", "displayName", value)} value={draft.locales["en-US"]?.displayName ?? ""} />
              <AgentSkillTextField label="English description" onChange={(value) => updateLocaleField("en-US", "shortDescription", value)} value={draft.locales["en-US"]?.shortDescription ?? ""} />
              <AgentSkillTextField label="中文名称" onChange={(value) => updateLocaleField("zh-CN", "displayName", value)} value={draft.locales["zh-CN"]?.displayName ?? ""} />
              <AgentSkillTextField label="中文说明" onChange={(value) => updateLocaleField("zh-CN", "shortDescription", value)} value={draft.locales["zh-CN"]?.shortDescription ?? ""} />
            </div>
          </fieldset>
          <fieldset className="agent-skill-source-fieldset">
            <legend>{chinese ? "提示词内容" : "Prompt content"}</legend>
            <AgentSkillTextArea className="agent-skill-source-editor" label={chinese ? "SKILL.md 提示词正文" : "SKILL.md prompt body"} onChange={(value) => updateStructuredField("body", value)} required rows={22} value={draft.body} />
          </fieldset>
          {error ? <p className="agent-skill-form-error" role="alert">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button disabled={saving} onClick={() => props.onOpenChange(false)} variant="outline">{chinese ? "取消" : "Cancel"}</Button>
          <Button disabled={saving || Boolean(error)} onClick={() => void submit()}>{saving ? (chinese ? "保存中…" : "Saving…") : (chinese ? "保存技能" : "Save skill")}</Button>
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

  React.useEffect(() => {
    if (props.open) {
      setEditing(undefined)
      setError("")
    }
  }, [props.open])

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
    try {
      setSaving(true)
      setError("")
      await props.onSave(createCategorySaveRequest(draft, props.locale.startsWith("zh")))
      setEditing(undefined)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setSaving(false)
    }
  }, [draft, props])

  // deleteCategory confirms and deletes one user-owned category row.
  // deleteCategory 确认并删除一条用户所有的分类记录。
  const deleteCategory = React.useCallback(async (category: AgentSkillCategorySummary) => {
    const chinese = props.locale.startsWith("zh")
    if (!window.confirm(chinese ? `确认删除分类“${localizedCategoryName(category, props.locale)}”？` : `Delete category “${localizedCategoryName(category, props.locale)}”?`)) {
      return
    }
    try {
      setSaving(true)
      setError("")
      await props.onDelete(category.categoryId)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setSaving(false)
    }
  }, [props])

  // chinese controls the leading user-facing copy.
  // chinese 控制优先展示的用户文案。
  const chinese = props.locale.startsWith("zh")
  return (
    <Dialog onOpenChange={props.onOpenChange} open={props.open}>
      <DialogContent className="agent-skill-category-management-dialog" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{chinese ? "分类管理" : "Category management"}</DialogTitle>
          <DialogDescription>{chinese ? "系统分类不可修改或删除；附加分类与技能归属写入技能目录配置。" : "System categories are immutable; added categories and assignments are stored in the skill-root configuration."}</DialogDescription>
        </DialogHeader>
        {editing === undefined ? (
          <div className="agent-skill-category-management-body">
            <div className="agent-skill-category-management-actions">
              <Button disabled={saving} onClick={() => openForm(null)}><Plus size={15} />{chinese ? "添加分类" : "Add category"}</Button>
            </div>
            <div className="agent-skill-category-management-list">
              {props.categories.map((category) => (
                <div className="agent-skill-category-management-row" key={category.categoryId}>
                  <span className="agent-skill-category-management-marker" style={{ backgroundColor: category.color }}>{category.emoji}</span>
                  <div><strong>{localizedCategoryName(category, props.locale)}</strong><span>{category.categoryId}</span></div>
                  <span>{category.builtin ? (chinese ? "系统分类" : "System") : (chinese ? "附加分类" : "Added")}</span>
                  {!category.builtin ? (
                    <div>
                      <button aria-label={chinese ? "编辑分类" : "Edit category"} onClick={() => openForm(category)} type="button"><Pencil size={15} /></button>
                      <button aria-label={chinese ? "删除分类" : "Delete category"} onClick={() => void deleteCategory(category)} type="button"><Trash2 size={15} /></button>
                    </div>
                  ) : null}
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div className="agent-skill-form-grid">
            <AgentSkillTextField disabled={editing !== null} label="category-id" onChange={(value) => updateCategoryField("categoryId", value)} required value={draft.categoryId} />
            <AgentSkillTextField label={chinese ? "默认名称" : "Default name"} onChange={(value) => updateCategoryField("name", value)} required value={draft.name} />
            <AgentSkillTextField label={chinese ? "图标" : "Emoji"} onChange={(value) => updateCategoryField("emoji", value)} required value={draft.emoji} />
            <label className="agent-skill-form-field">
              <span>{chinese ? "颜色标记" : "Color marker"}</span>
              <AgentSkillColorPicker disabled={saving} locale={props.locale} onChange={(value) => updateCategoryField("color", value)} value={draft.color} />
            </label>
            <AgentSkillTextField label={chinese ? "排序" : "Order"} onChange={(value) => updateCategoryField("order", value)} required type="number" value={draft.order} />
            <AgentSkillTextField label={chinese ? "默认说明" : "Default description"} onChange={(value) => updateCategoryField("description", value)} value={draft.description} />
            <AgentSkillTextField label="English name" onChange={(value) => updateCategoryField("englishName", value)} required value={draft.englishName} />
            <AgentSkillTextField label="English description" onChange={(value) => updateCategoryField("englishDescription", value)} value={draft.englishDescription} />
            <AgentSkillTextField label="中文名称" onChange={(value) => updateCategoryField("chineseName", value)} required value={draft.chineseName} />
            <AgentSkillTextField label="中文说明" onChange={(value) => updateCategoryField("chineseDescription", value)} value={draft.chineseDescription} />
          </div>
        )}
        {error ? <p className="agent-skill-form-error" role="alert">{error}</p> : null}
        <DialogFooter>
          {editing !== undefined ? <Button disabled={saving} onClick={() => setEditing(undefined)} variant="outline">{chinese ? "返回" : "Back"}</Button> : null}
          <Button disabled={saving} onClick={() => props.onOpenChange(false)} variant="outline">{chinese ? "关闭" : "Close"}</Button>
          {editing !== undefined ? <Button disabled={saving} onClick={() => void saveCategory()}>{chinese ? "保存分类" : "Save category"}</Button> : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// AgentSkillTextField renders one compact controlled text field.
// AgentSkillTextField 渲染一个紧凑受控文本字段。
function AgentSkillTextField(props: { disabled?: boolean; label: string; onChange: (value: string) => void; required?: boolean; type?: React.HTMLInputTypeAttribute; value: string }) {
  return <label className="agent-skill-form-field"><span>{props.label}{props.required ? " *" : ""}</span><input disabled={props.disabled} onChange={(event) => props.onChange(event.target.value)} required={props.required} type={props.type ?? "text"} value={props.value} /></label>
}

// AgentSkillTextArea renders one controlled multiline prompt or metadata field.
// AgentSkillTextArea 渲染一个受控多行提示词或元数据字段。
function AgentSkillTextArea(props: { className?: string; label: string; onChange: (value: string) => void; required?: boolean; rows: number; value: string }) {
  return <label className={`agent-skill-form-field ${props.className ?? ""}`}><span>{props.label}{props.required ? " *" : ""}</span><textarea onChange={(event) => props.onChange(event.target.value)} required={props.required} rows={props.rows} spellCheck={false} value={props.value} /></label>
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

  const chooseColorLabel = props.locale.startsWith("zh") ? "选择颜色" : "Choose color"
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
    color: record?.color ?? "#64748B",
    locales: record?.locales ?? {},
    categoryId: record?.categoryId ?? defaultCategoryId,
    tags: record?.tags ?? [],
    enabled: record?.enabled ?? true,
  }
}

// createSkillSaveRequest validates one synchronized draft and builds the strict service payload.
// createSkillSaveRequest 校验一个同步草稿并构建严格服务载荷。
function createSkillSaveRequest(draft: AgentSkillEditorDraft, record: AgentSkillRecordSummary | null, copy: boolean, chinese: boolean): AgentSkillSaveRequest {
  if (!/^(?!-)(?!.*--)[a-z0-9-]{1,64}(?<!-)$/u.test(draft.name)) throw new Error(chinese ? "name 必须为 1–64 个小写字母、数字或单连字符，且连字符不能位于首尾。" : "name must be a canonical lowercase package directory name.")
  if (!draft.description.trim() || Array.from(draft.description).length > 1024) throw new Error(chinese ? "模型触发说明必须包含 1–1024 个字符。" : "The model trigger description must contain 1–1024 characters.")
  if (Array.from(draft.compatibility).length > 500) throw new Error(chinese ? "兼容环境说明不能超过 500 个字符。" : "Compatibility cannot exceed 500 characters.")
  if (draft.color && !/^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/iu.test(draft.color)) throw new Error(chinese ? "颜色必须为十六进制 RGB 或 RGBA。" : "Color must be hexadecimal RGB or RGBA.")
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
    color: category?.color ?? "#64748B",
    order: String(category?.order ?? 100),
    englishName: category?.locales["en-US"]?.name ?? "",
    englishDescription: category?.locales["en-US"]?.description ?? "",
    chineseName: category?.locales["zh-CN"]?.name ?? "",
    chineseDescription: category?.locales["zh-CN"]?.description ?? "",
  }
}

// createCategorySaveRequest validates and builds one strict category payload.
// createCategorySaveRequest 校验并构建一个严格分类载荷。
function createCategorySaveRequest(draft: AgentSkillCategoryDraft, chinese: boolean): AgentSkillCategorySaveRequest {
  if (!/^(?!-)(?!.*--)[a-z0-9-]{1,64}(?<!-)$/u.test(draft.categoryId)) throw new Error(chinese ? "category-id 必须为规范小写短标识。" : "category-id must be a canonical lowercase slug.")
  const order = Number(draft.order)
  if (!Number.isSafeInteger(order)) throw new Error(chinese ? "分类排序必须为安全整数。" : "Category order must be a safe integer.")
  if (!draft.name.trim() || !draft.englishName.trim() || !draft.chineseName.trim()) throw new Error(chinese ? "默认名称、English name 与中文名称均为必填项。" : "Default name, English name, and Chinese name are required.")
  if (!draft.emoji.trim() || !/^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/iu.test(draft.color.trim())) throw new Error(chinese ? "分类图标不能为空，颜色必须为十六进制 RGB 或 RGBA。" : "Emoji is required and color must be hexadecimal RGB or RGBA.")
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

// colorInputValue converts a stored color into a native color-input value.
// colorInputValue 将已存储颜色转换为原生颜色输入值。
function colorInputValue(value: string): string {
  const renderedColor = colorSwatchValue(value).trim()
  const longHex = renderedColor.match(/^#([0-9a-f]{6})$/iu)
  if (longHex) return `#${longHex[1].toLowerCase()}`
  const shortHex = renderedColor.match(/^#([0-9a-f])([0-9a-f])([0-9a-f])$/iu)
  if (shortHex) return `#${shortHex[1]}${shortHex[1]}${shortHex[2]}${shortHex[2]}${shortHex[3]}${shortHex[3]}`.toLowerCase()
  return "#6b7280"
}

// colorSwatchValue resolves supported agent color tokens into CSS colors.
// colorSwatchValue 将支持的智能体颜色标识解析为 CSS 颜色。
function colorSwatchValue(value: string): string {
  const trimmed = value.trim()
  if (trimmed.startsWith("#") || trimmed.startsWith("rgb") || trimmed.startsWith("hsl")) return trimmed
  const palette: Record<string, string> = {
    blue: "#3b82f6",
    cyan: "#06b6d4",
    gray: "#6b7280",
    green: "#22c55e",
    orange: "#f97316",
    pink: "#ec4899",
    purple: "#8b5cf6",
    red: "#ef4444",
    yellow: "#eab308",
  }
  return palette[trimmed.toLowerCase()] ?? "#6b7280"
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
