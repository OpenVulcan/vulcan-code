import * as React from "react"
import { Bot, ExternalLink, FolderOpen, Github, Maximize2, Minimize2, Pencil, Plus, RefreshCw, RotateCcw, Save, Trash2, X } from "lucide-react"
import { toast } from "sonner"

import { ModelPickerControl, modelPickerChoiceKey } from "@/components/composer/model-picker-control"
import { formatSkillText } from "@/components/features/featureText"
import type { FeatureHeaderSummary } from "@/components/features/feature-area-types"
import { Badge } from "@/components/ui/badge"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { StatusPill } from "@/components/shared/status"
import { useI18n } from "@/i18n"
import { openExternalUrl } from "@/service/project-actions-service"
import type { CapabilityPolicyMode, SubagentCatalogSummary, SubagentCategorySummary, SubagentDefinitionSaveRequest, SubagentDefinitionSummary, SubagentOriginSummary, WorkbenchSnapshot } from "@/types"
import type { AgentActionProps } from "./agent-area-types"

export type { AgentActionProps } from "./agent-area-types"

// AgentEditorDraft stores the editable form state before it becomes a save request.
// AgentEditorDraft 保存可编辑表单在转换为保存请求前的状态。
interface AgentEditorDraft {
  // Stable agent id, normally the Markdown file stem.
  // 稳定智能体标识，通常为 Markdown 文件 stem。
  agentId: string
  // Original agent id used when saving package transfers.
  // 保存包转移时使用的原始智能体标识。
  originalAgentId: string
  // Optional user package id.
  // 可选用户包标识。
  packageId: string
  // Human-facing display name.
  // 面向用户的显示名称。
  name: string
  // Optional human-facing description.
  // 可选的人类可读说明。
  description: string
  // Currently selected metadata language.
  // 当前选中的元数据语言。
  metadataLocale: AgentMetadataLocale
  // Editable localized names keyed by locale.
  // 按语言索引的可编辑本地化名称。
  localizedNames: Record<string, string>
  // Editable localized descriptions keyed by locale.
  // 按语言索引的可编辑本地化说明。
  localizedDescriptions: Record<string, string>
  // Human-facing emoji.
  // 面向用户的图标。
  emoji: string
  // Human-facing color token or value.
  // 面向用户的颜色标识或颜色值。
  color: string
  // Optional short tone or persona hint.
  // 可选简短语气或人格提示。
  vibe: string
  // Whether this definition is exposed to the model.
  // 该定义是否暴露给模型。
  enabled: boolean
  // Base system prompt text.
  // 基础系统提示词文本。
  systemPrompt: string
  // Appended system prompt text.
  // 追加系统提示词文本。
  systemPromptAppend: string
  // Fixed provider id; empty means follow the parent session model.
  // 固定供应商标识；为空表示跟随父会话模型。
  fixedModelProviderId: string
  // Fixed model id; empty means follow the parent session model.
  // 固定模型标识；为空表示跟随父会话模型。
  fixedModelId: string
  // Current backend availability diagnostic for the fixed model.
  // 当前固定模型的后端可用性诊断。
  fixedModelAvailable: boolean | null
  // Current backend unavailable reason for the fixed model.
  // 当前固定模型的后端不可用原因。
  fixedModelUnavailableReason: string
  // Unified tool policy mode.
  // 统一工具策略模式。
  toolPolicyMode: CapabilityPolicyMode
  // Newline-separated tool selectors used by the policy mode.
  // 当前策略模式使用的按换行分隔工具选择器。
  toolPolicyTools: string
  // Unified prompt skill policy mode.
  // 统一提示词技能策略模式。
  promptSkillPolicyMode: CapabilityPolicyMode
  // Newline-separated prompt skill selectors used by the policy mode.
  // 当前策略模式使用的按换行分隔提示词技能选择器。
  promptSkillPolicySkills: string
  // Whether this draft edits an existing id.
  // 当前草稿是否编辑已有标识。
  existing: boolean
  // Source scope label of the definition being edited.
  // 当前编辑定义的来源作用域标签。
  sourceScope: string
  // Stable source scope key of the definition being edited.
  // 当前编辑定义的稳定来源作用域键。
  sourceScopeKey: string
  // Optional system definition id overridden by this draft.
  // 当前草稿覆盖的可选系统定义标识。
  overrideOf: string
  // Current definition file path shown for audit context.
  // 用于审计上下文展示的当前定义文件路径。
  definitionFile: string
}

// AgentMetadataLocale is the editable display metadata language supported by the form.
// AgentMetadataLocale 是表单支持编辑的展示元数据语言。
type AgentMetadataLocale = "zh-CN" | "en-US"

// AGENT_METADATA_LOCALES lists selectable metadata languages in stable order.
// AGENT_METADATA_LOCALES 按稳定顺序列出可选择的元数据语言。
const AGENT_METADATA_LOCALES: Array<{ locale: AgentMetadataLocale; zh: string; en: string }> = [
  { locale: "zh-CN", zh: "中文", en: "Chinese" },
  { locale: "en-US", zh: "英文", en: "English" },
]

// AgentStatusFilter represents the top-level enabled-state filter.
// AgentStatusFilter 表示顶部启停状态筛选。
type AgentStatusFilter = "all" | "enabled" | "disabled"

// AgentCapabilityPolicyOption describes one selectable policy row.
// AgentCapabilityPolicyOption 描述策略表中的单个可选能力行。
interface AgentCapabilityPolicyOption {
  // Stable capability identifier persisted in the agent definition.
  // 持久化到智能体定义中的稳定能力标识。
  id: string
  // Human-facing capability label.
  // 面向用户的能力名称。
  label: string
  // Capability family or source label.
  // 能力族或来源标签。
  family: string
  // Optional capability description.
  // 可选能力说明。
  description: string
  // Additional source detail.
  // 额外来源详情。
  detail: string
}

// activeAgentsHeaderSummary returns the title and metrics for the independent agents page.
// activeAgentsHeaderSummary 返回独立智能体页面的标题与统计。
export function activeAgentsHeaderSummary(workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): FeatureHeaderSummary {
  const enabledCount = workbench.subagentCatalog.definitions.filter((definition) => definition.enabled).length
  return {
    title: t("area.agents"),
    detail: formatSkillText(t("agents.header.detail"), {
      total: workbench.subagentCatalog.definitions.length,
      enabled: enabledCount,
      warnings: workbench.subagentCatalog.errors.length,
    }),
  }
}

// AgentsArea renders the independent agent and expert management page.
// AgentsArea 渲染独立智能体与专家管理页面。
export function AgentsArea(props: {
  // Service-backed agent page actions.
  // 服务端支持的智能体页面动作。
  actions: AgentActionProps
  // Currently selected category id, or empty for all categories.
  // 当前选中的分类标识；空字符串表示全部分类。
  selectedCategoryId?: string
  // Updates the selected category id.
  // 更新当前选中的分类标识。
  onSelectCategory?: (categoryId: string) => void
  // Workbench snapshot containing the agent catalog.
  // 包含智能体目录的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { locale, t } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedAgentLiteral(locale, zh, en), [locale])
  // busyAction stores the catalog action currently in flight.
  // busyAction 保存当前正在执行的目录动作。
  const [busyAction, setBusyAction] = React.useState<string | null>(null)
  // togglingAgentIds stores row-level enabled-state updates currently in flight.
  // togglingAgentIds 保存当前正在执行启停更新的行级智能体标识。
  const [togglingAgentIds, setTogglingAgentIds] = React.useState<ReadonlySet<string>>(() => new Set())
  // editorDraft stores the currently edited or newly created definition.
  // editorDraft 保存当前正在编辑或新建的定义。
  const [editorDraft, setEditorDraft] = React.useState<AgentEditorDraft | null>(null)
  // detailDefinition stores the agent selected for the details dialog.
  // detailDefinition 保存当前打开详情弹窗的智能体定义。
  const [detailDefinition, setDetailDefinition] = React.useState<SubagentDefinitionSummary | null>(null)
  // searchQuery stores the cross-locale keyword filter text.
  // searchQuery 保存跨语言关键词筛选文本。
  const [searchQuery, setSearchQuery] = React.useState("")
  // statusFilter stores the enabled-state filter.
  // statusFilter 保存启停状态筛选。
  const [statusFilter, setStatusFilter] = React.useState<AgentStatusFilter>("all")
  // localCategoryId stores standalone category filtering outside the adaptive shell.
  // localCategoryId 保存自适应外壳之外的独立分类筛选。
  const [localCategoryId, setLocalCategoryId] = React.useState("")
  // selectedCategoryId resolves controlled or standalone category filtering.
  // selectedCategoryId 解析受控或独立的分类筛选。
  const selectedCategoryId = props.selectedCategoryId ?? localCategoryId
  // onSelectCategory resolves controlled or standalone category updates.
  // onSelectCategory 解析受控或独立的分类更新。
  const onSelectCategory = props.onSelectCategory ?? setLocalCategoryId
  // editorToolOptions merges live tool rows with the draft's persisted policy ids.
  // editorToolOptions 合并实时工具行与草稿中已持久化的策略标识。
  const editorToolOptions = React.useMemo(() => buildAgentToolPolicyOptions(props.workbench, editorDraft, locale), [editorDraft, locale, props.workbench])
  // editorPromptSkillOptions merges live Agent Skill rows with the draft's persisted policy ids.
  // editorPromptSkillOptions 合并实时 Agent Skill 行与草稿中已持久化的策略标识。
  const editorPromptSkillOptions = React.useMemo(() => buildAgentPromptSkillPolicyOptions(props.workbench, editorDraft, locale), [editorDraft, locale, props.workbench])
  // categoryRows sorts catalog categories for the filter menu.
  // categoryRows 对目录分类排序以供筛选菜单使用。
  const categoryRows = React.useMemo(() => sortedAgentCategories(props.workbench.subagentCatalog.categories, locale), [locale, props.workbench.subagentCatalog.categories])
  // categoryById resolves localized category metadata for definition cards.
  // categoryById 为定义卡片解析本地化分类元数据。
  const categoryById = React.useMemo(() => new Map(categoryRows.map((category) => [category.categoryId, category])), [categoryRows])
  // visibleDefinitions applies category, status, and cross-locale keyword filters.
  // visibleDefinitions 应用分类、启停状态与跨语言关键词筛选。
  const visibleDefinitions = React.useMemo(() => {
    return props.workbench.subagentCatalog.definitions.filter((definition) => (
      (!selectedCategoryId || definition.categoryId === selectedCategoryId)
        && agentDefinitionMatchesStatusFilter(definition, statusFilter)
        && agentDefinitionMatchesSearch(definition, searchQuery)
    ))
  }, [props.workbench.subagentCatalog.definitions, searchQuery, selectedCategoryId, statusFilter])

  React.useEffect(() => {
    if (selectedCategoryId && !props.workbench.subagentCatalog.categories.some((category) => category.categoryId === selectedCategoryId)) {
      onSelectCategory("")
    }
  }, [onSelectCategory, props.workbench.subagentCatalog.categories, selectedCategoryId])

  const runAction = React.useCallback(async (label: string, action: () => Promise<void>) => {
    setBusyAction(label)
    try {
      await action()
    } finally {
      setBusyAction(null)
    }
  }, [])

  // runToggleAction executes one enabled-state update without blocking the whole page.
  // runToggleAction 执行单个启停更新且不阻塞整个页面。
  const runToggleAction = React.useCallback(async (agentId: string, action: () => Promise<void>) => {
    setTogglingAgentIds((current) => {
      const next = new Set(current)
      next.add(agentId)
      return next
    })
    try {
      await action()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    } finally {
      setTogglingAgentIds((current) => {
        const next = new Set(current)
        next.delete(agentId)
        return next
      })
    }
  }, [])

  // submitEditorDraft persists the current draft through the user catalog.
  // submitEditorDraft 通过用户目录持久化当前草稿。
  const submitEditorDraft = React.useCallback(async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const normalizedDraft = editorDraft ? normalizedAgentEditorDraftFileIdentity(editorDraft) : null
    if (!normalizedDraft || normalizedDraft.agentId.trim().length === 0) {
      return
    }
    const conflictsWithExistingDefinition = !normalizedDraft.existing
      && props.workbench.subagentCatalog.definitions.some((definition) => definition.agentId === targetAgentIdFromDraft(normalizedDraft))
    if (conflictsWithExistingDefinition) {
      setEditorDraft(normalizedDraft)
      toast.error(text("技能文件名已存在，请换一个文件名。", "Skill file name already exists. Choose another file name."))
      return
    }
    let request: SubagentDefinitionSaveRequest
    try {
      request = saveRequestFromDraft(normalizedDraft)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
      return
    }
    await runAction("save", () => props.actions.onSaveAgentDefinition(request))
    setEditorDraft(null)
  }, [editorDraft, props.actions, props.workbench.subagentCatalog.definitions, runAction, text])

  return (
    <section className="skill-management-surface agent-management-surface">
      <div className="skill-page-body agent-skill-page-body">
        <section className="skill-toolbar agent-catalog-toolbar" aria-label={text("智能体目录操作", "Agent catalog actions")}>
          <div className="agent-catalog-toolbar-heading">
            <p>{text("管理 Vulcan Code Agent 运行时消费的用户级子智能体 Markdown 文件。", "Manage user-level subagent Markdown files consumed by the Vulcan Code Agent runtime.")}</p>
            <div className="agent-catalog-toolbar-actions">
              <button aria-label={text("新增智能体", "New agent")} className="agent-catalog-icon-button" disabled={busyAction !== null} onClick={() => setEditorDraft(emptyAgentDraft(defaultAgentMetadataLocale(locale)))} title={text("新增智能体", "New agent")} type="button">
                <Plus size={15} />
              </button>
              <button aria-label={text("打开用户目录", "Open user directory")} className="agent-catalog-icon-button" disabled={busyAction !== null} onClick={() => runAction("open-user-dir", props.actions.onOpenAgentUserDirectory)} title={text("打开用户目录", "Open user directory")} type="button">
                <FolderOpen size={15} />
              </button>
              <button aria-label={t("skills.action.refresh")} className="agent-catalog-icon-button" disabled={busyAction !== null} onClick={() => runAction("refresh", props.actions.onRefreshAgentCatalog)} title={t("skills.action.refresh")} type="button">
                <RefreshCw className={busyAction === "refresh" ? "spin" : undefined} size={15} />
              </button>
            </div>
          </div>
          <div className="agent-catalog-filter-row">
            <input
              aria-label={text("搜索子智能体", "Search subagents")}
              className="agent-catalog-search-input"
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder={text("搜索子智能体...", "Search subagents...")}
              type="search"
              value={searchQuery}
            />
            <select
              aria-label={text("启停状态筛选", "Enabled-state filter")}
              className="agent-catalog-status-select"
              onChange={(event) => setStatusFilter(event.target.value as AgentStatusFilter)}
              value={statusFilter}
            >
              <option value="all">{text("全部", "All")}</option>
              <option value="enabled">{text("已启用", "Enabled")}</option>
              <option value="disabled">{text("已停用", "Disabled")}</option>
            </select>
          </div>
        </section>
        <AgentCatalogIssues catalog={props.workbench.subagentCatalog} />
        <div className="skill-card-list agent-skill-card-list">
          {props.workbench.subagentCatalog.definitions.length === 0 ? (
            <article className="skill-empty-card">
              <StatusPill status="pending" label="pending" />
              <strong>{text("暂无智能体定义", "No agent definitions")}</strong>
              <span>{text("可以新增智能体，或打开用户目录后手动复制 Markdown 与元数据文件。", "Create an agent manually, or open the user directory and copy Markdown plus metadata files there.")}</span>
            </article>
          ) : visibleDefinitions.length === 0 ? (
            <article className="skill-empty-card">
              <StatusPill status="pending" label="pending" />
              <strong>{text("没有匹配的智能体", "No matching agents")}</strong>
              <span>{text("调整分类、启停状态或搜索关键词。", "Adjust the category, enabled state, or search keywords.")}</span>
            </article>
          ) : visibleDefinitions.map((definition) => (
            <AgentDefinitionCard
              actions={props.actions}
              busy={busyAction !== null}
              categoryById={categoryById}
              definition={definition}
              key={definition.sourceKey}
              onEdit={() => setEditorDraft(draftFromDefinition(definition, defaultAgentMetadataLocale(locale)))}
              onOpenDetail={setDetailDefinition}
              onRunAction={runAction}
              onRunToggleAction={runToggleAction}
              toggleBusy={togglingAgentIds.has(definition.agentId)}
            />
          ))}
        </div>
        <AgentEditorForm
          busy={busyAction !== null}
          draft={editorDraft}
          onCancel={() => setEditorDraft(null)}
          onDraftChange={setEditorDraft}
          promptSkillOptions={editorPromptSkillOptions}
          providers={props.workbench.providers}
          onSubmit={submitEditorDraft}
          toolOptions={editorToolOptions}
        />
        <AgentDefinitionDetailDialog
          definition={detailDefinition}
          open={detailDefinition !== null}
          onOpenChange={(open) => {
            if (!open) {
              setDetailDefinition(null)
            }
          }}
        />
      </div>
    </section>
  )
}

// FloatingAgentCategoryMenu renders the adaptive agent category selector.
// FloatingAgentCategoryMenu 渲染自适应智能体分类选择器。
export function FloatingAgentCategoryMenu(props: {
  // Whether the menu is expanded.
  // 菜单是否展开。
  expanded: boolean
  // Updates the expanded menu state.
  // 更新菜单展开状态。
  onExpandedChange: (expanded: boolean) => void
  // Currently selected category id, or empty for all categories.
  // 当前选中的分类标识；空字符串表示全部分类。
  selectedCategoryId: string
  // Applies the selected category id.
  // 应用选中的分类标识。
  onSelectCategory: (categoryId: string) => void
  // Workbench snapshot containing the agent catalog.
  // 包含智能体目录的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { locale, t } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedAgentLiteral(locale, zh, en), [locale])
  // categories stores sorted category rows for the menu.
  // categories 保存菜单使用的已排序分类行。
  const categories = React.useMemo(() => sortedAgentCategories(props.workbench.subagentCatalog.categories, locale), [locale, props.workbench.subagentCatalog.categories])
  // countByCategory stores visible definition counts keyed by category id.
  // countByCategory 按分类标识保存可见定义数量。
  const countByCategory = React.useMemo(() => {
    const counts = new Map<string, number>()
    props.workbench.subagentCatalog.definitions.forEach((definition) => {
      const categoryId = definition.categoryId ?? ""
      counts.set(categoryId, (counts.get(categoryId) ?? 0) + 1)
    })
    return counts
  }, [props.workbench.subagentCatalog.definitions])
  // activeLabel stores the selected menu label for the collapsed pill.
  // activeLabel 保存收起胶囊展示的当前菜单标签。
  const activeLabel = React.useMemo(() => {
    if (!props.selectedCategoryId) {
      return text("全部", "All")
    }
    const selected = categories.find((category) => category.categoryId === props.selectedCategoryId)
    return selected ? localizedCategoryName(selected, locale) : props.selectedCategoryId
  }, [categories, locale, props.selectedCategoryId, text])

  if (!props.expanded) {
    return (
      <button
        className="workspace-tool-panel-menu-collapsed"
        onClick={() => props.onExpandedChange(true)}
        title={t("workspaceToolPanel.expandState")}
        type="button"
      >
        <span>{activeLabel}</span>
        <Maximize2 size={13} />
      </button>
    )
  }

  return (
    <aside className="workspace-tool-panel-menu agent-category-floating-menu" aria-label={text("智能体分类", "Agent categories")}>
      <div className="workspace-tool-panel-menu-header">
        <span>{text("分类", "Categories")}</span>
        <button
          aria-label={t("workspaceToolPanel.collapseToPill")}
          className="workspace-tool-panel-menu-icon-button"
          onClick={() => props.onExpandedChange(false)}
          type="button"
        >
          <Minimize2 size={13} />
        </button>
      </div>
      <div className="workspace-tool-panel-menu-list">
        <button
          className={`workspace-tool-panel-menu-row agent-category-floating-row ${props.selectedCategoryId === "" ? "active" : ""}`}
          onClick={() => props.onSelectCategory("")}
          type="button"
        >
          <Bot size={15} />
          <span>{text("全部", "All")}</span>
          <strong>{props.workbench.subagentCatalog.definitions.length}</strong>
        </button>
        {categories.map((category) => (
          <button
            className={`workspace-tool-panel-menu-row agent-category-floating-row ${props.selectedCategoryId === category.categoryId ? "active" : ""}`}
            key={category.categoryId}
            onClick={() => props.onSelectCategory(category.categoryId)}
            title={localizedCategoryDescription(category, locale) ?? category.categoryId}
            type="button"
          >
            <span className="agent-category-floating-icon" style={{ backgroundColor: colorSwatchValue(category.color) }}>{category.emoji}</span>
            <span>{localizedCategoryName(category, locale)}</span>
            <strong>{countByCategory.get(category.categoryId) ?? 0}</strong>
          </button>
        ))}
      </div>
    </aside>
  )
}

// AgentCatalogIssues renders non-fatal catalog loading diagnostics.
// AgentCatalogIssues 渲染非致命目录加载诊断。
function AgentCatalogIssues(props: {
  // Catalog summary containing loading errors.
  // 包含加载错误的目录摘要。
  catalog: SubagentCatalogSummary
}) {
  if (props.catalog.errors.length === 0) {
    return null
  }
  return (
    <div className="skill-warning-list">
      {props.catalog.errors.map((error) => (
        <article className="skill-warning-row" key={`${error.path}-${error.message}`}>
          <strong>{error.path}</strong>
          <span>{error.message}</span>
        </article>
      ))}
    </div>
  )
}

// AgentEditorForm renders the create/edit form for one agent definition.
// AgentEditorForm 渲染单个智能体定义的新建或编辑表单。
function AgentEditorForm(props: {
  // Whether a catalog action is running.
  // 目录动作是否正在运行。
  busy: boolean
  // Draft state rendered by the form.
  // 表单渲染使用的草稿状态。
  draft: AgentEditorDraft | null
  // Cancels the current edit.
  // 取消当前编辑。
  onCancel: () => void
  // Updates the draft state.
  // 更新草稿状态。
  onDraftChange: (draft: AgentEditorDraft) => void
  // Selectable prompt skill rows available to the three-state policy table.
  // 三态策略表可选择的提示词技能行。
  promptSkillOptions: AgentCapabilityPolicyOption[]
  // Enabled provider/model rows available to the model picker.
  // 模型选择器可用的已启用供应商/模型行。
  providers: WorkbenchSnapshot["providers"]
  // Submits the current draft.
  // 提交当前草稿。
  onSubmit: (event: React.FormEvent<HTMLFormElement>) => void
  // Selectable tool rows available to the three-state policy table.
  // 三态策略表可选择的工具行。
  toolOptions: AgentCapabilityPolicyOption[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedAgentLiteral(locale, zh, en), [locale])
  const draft = props.draft
  const editingSystemBackedDefinition = draft?.sourceScopeKey === "system" || Boolean(draft?.overrideOf)
  const readOnlySystemMetadata = Boolean(editingSystemBackedDefinition)
  const selectedMetadataLocale = draft?.metadataLocale ?? defaultAgentMetadataLocale(locale)
  const selectedMetadataName = draft?.localizedNames[selectedMetadataLocale] ?? ""
  const selectedMetadataDescription = draft?.localizedDescriptions[selectedMetadataLocale] ?? ""
  const fixedModelValue = draft?.fixedModelProviderId && draft.fixedModelId ? `${draft.fixedModelProviderId}/${draft.fixedModelId}` : ""
  const fixedModelMissingFromChoices = Boolean(fixedModelValue && !modelPickerChoiceExists(props.providers, fixedModelValue))
  const fixedModelUnavailable = Boolean(fixedModelValue && (draft?.fixedModelAvailable === false || fixedModelMissingFromChoices))
  const fixedModelUnavailableReason = draft?.fixedModelUnavailableReason || text("当前固定模型不在可用模型列表中，运行时会回退到会话模型。", "This fixed model is not in the available model list; runtime will fall back to the session model.")
  // updateDraftField applies one field update while preserving the rest of the draft.
  // updateDraftField 在保留其余草稿字段的同时应用单字段更新。
  const updateDraftField = React.useCallback(<Key extends keyof AgentEditorDraft>(key: Key, value: AgentEditorDraft[Key]) => {
    if (!props.draft) {
      return
    }
    props.onDraftChange({ ...props.draft, [key]: value })
  }, [props])
  // updateDraftMetadataField updates one localized display metadata field.
  // updateDraftMetadataField 更新单个本地化展示元数据字段。
  const updateDraftMetadataField = React.useCallback((field: "name" | "description", value: string) => {
    if (!props.draft) {
      return
    }
    const mapKey = field === "name" ? "localizedNames" : "localizedDescriptions"
    props.onDraftChange({
      ...props.draft,
      [mapKey]: {
        ...props.draft[mapKey],
        [props.draft.metadataLocale]: value,
      },
    })
  }, [props])
  // updateToolPolicyItems replaces the unified tool policy selector list.
  // updateToolPolicyItems 替换统一工具策略选择器列表。
  const updateToolPolicyItems = React.useCallback((toolIds: string[]) => {
    if (!props.draft) {
      return
    }
    props.onDraftChange({ ...props.draft, toolPolicyTools: listText(toolIds) })
  }, [props])
  // updatePromptSkillPolicyItems replaces the prompt skill policy selector list.
  // updatePromptSkillPolicyItems 替换提示词技能策略选择器列表。
  const updatePromptSkillPolicyItems = React.useCallback((skillNames: string[]) => {
    if (!props.draft) {
      return
    }
    props.onDraftChange({ ...props.draft, promptSkillPolicySkills: listText(skillNames) })
  }, [props])

  return (
    <Dialog open={draft !== null} onOpenChange={(open) => {
      if (!open && !props.busy) {
        props.onCancel()
      }
    }}>
      <DialogContent className="agent-editor-dialog">
        <DialogHeader>
          <DialogTitle>{draft?.existing ? text("编辑智能体", "Edit agent") : text("新增智能体", "New agent")}</DialogTitle>
          <DialogDescription>
            {editingSystemBackedDefinition
              ? text("系统内置智能体只允许保存用户侧元数据覆盖，例如工具策略和追加提示词。", "System agents only allow user metadata overrides, such as tool policy and appended prompts.")
              : text("保存后写入用户智能体目录。", "Saving writes to the user agent directory.")}
          </DialogDescription>
        </DialogHeader>
        {draft ? (
          <form className="agent-editor-form" onSubmit={props.onSubmit}>
            <div className="agent-editor-grid">
              <label>
                <span>{text("技能文件名", "Skill file name")}</span>
                <input disabled={props.busy || draft.existing} pattern="[A-Za-z0-9._/-]+" required value={draft.agentId} onChange={(event) => updateDraftField("agentId", normalizeAgentFileNameInput(event.target.value))} />
              </label>
              <label>
                <span>{text("用户包", "User package")}</span>
                <input disabled={props.busy || editingSystemBackedDefinition} pattern="[A-Za-z0-9._-]*" value={draft.packageId} onChange={(event) => updateDraftField("packageId", event.target.value)} />
              </label>
              <label>
                <span>{text("元数据语言", "Metadata language")}</span>
                <select disabled={props.busy || readOnlySystemMetadata} value={selectedMetadataLocale} onChange={(event) => updateDraftField("metadataLocale", event.target.value as AgentMetadataLocale)}>
                  {AGENT_METADATA_LOCALES.map((option) => (
                    <option key={option.locale} value={option.locale}>{text(option.zh, option.en)}</option>
                  ))}
                </select>
              </label>
              <label>
                <span>{text("名称", "Name")}</span>
                <input disabled={props.busy || readOnlySystemMetadata} value={selectedMetadataName} onChange={(event) => updateDraftMetadataField("name", event.target.value)} />
              </label>
              <label className="agent-editor-checkbox">
                <input checked={draft.enabled} disabled={props.busy || readOnlySystemMetadata} type="checkbox" onChange={(event) => updateDraftField("enabled", event.target.checked)} />
                <span>{text("暴露给模型", "Expose to model")}</span>
              </label>
              <label className="wide">
                <span>{text("描述", "Description")}</span>
                <input disabled={props.busy || readOnlySystemMetadata} value={selectedMetadataDescription} onChange={(event) => updateDraftMetadataField("description", event.target.value)} />
              </label>
              <label>
                <span>{text("图标", "Emoji")}</span>
                <input disabled={props.busy || readOnlySystemMetadata} value={draft.emoji} onChange={(event) => updateDraftField("emoji", event.target.value)} />
              </label>
              <div className="agent-editor-field">
                <span>{text("颜色标记", "Color marker")}</span>
                <AgentColorPicker disabled={props.busy || readOnlySystemMetadata} value={draft.color} onChange={(color) => updateDraftField("color", color)} />
              </div>
              <label className="wide">
                <span>vibe</span>
                <input disabled={props.busy || readOnlySystemMetadata} value={draft.vibe} onChange={(event) => updateDraftField("vibe", event.target.value)} />
              </label>
              <label className="wide agent-prompt-field primary">
                <span>{text("系统提示词", "System prompt")}</span>
                <textarea autoFocus={!readOnlySystemMetadata} disabled={props.busy || readOnlySystemMetadata} rows={14} value={draft.systemPrompt} onChange={(event) => updateDraftField("systemPrompt", event.target.value)} />
              </label>
              <label className="wide agent-prompt-field">
                <span>{text("追加提示词", "Prompt append")}</span>
                <textarea autoFocus={readOnlySystemMetadata} disabled={props.busy} rows={5} value={draft.systemPromptAppend} onChange={(event) => updateDraftField("systemPromptAppend", event.target.value)} />
              </label>
              <div className="wide agent-editor-model-field">
                <span>{text("模型", "Model")}</span>
                <div className="settings-card-actions">
                  <ModelPickerControl
                    ariaLabel={text("选择智能体固定模型", "Choose fixed agent model")}
                    fallbackLabel={text("选择模型", "Select model")}
                    filterPlaceholder={text("输入模型名称、ID 或 Provider 过滤", "Filter by model name, id, or provider")}
                    noMatchesLabel={text("当前过滤条件下没有匹配模型。", "No models match the current filter.")}
                    providers={props.providers}
                    value={fixedModelValue}
                    unavailableValueLabel={fixedModelValue ? `${text("不可用", "Unavailable")}: ${fixedModelValue}` : undefined}
                    disabled={props.busy}
                    onValueChange={(choice) => props.onDraftChange({
                      ...draft,
                      fixedModelProviderId: choice.providerId,
                      fixedModelId: choice.modelId,
                      fixedModelAvailable: true,
                      fixedModelUnavailableReason: "",
                    })}
                  />
                  <button
                    className="skill-action-button secondary"
                    disabled={props.busy || !fixedModelValue}
                    onClick={() => props.onDraftChange({ ...draft, fixedModelProviderId: "", fixedModelId: "", fixedModelAvailable: null, fixedModelUnavailableReason: "" })}
                    type="button"
                  >
                    <RotateCcw size={14} />
                    <span>{text("跟随会话", "Follow session")}</span>
                  </button>
                </div>
                <small className="settings-muted-line">
                  {fixedModelValue ? `${text("固定模型", "Fixed model")}: ${draft.fixedModelProviderId}/${draft.fixedModelId}` : text("当前跟随会话模型。", "Currently follows the session model.")}
                </small>
                {fixedModelUnavailable ? (
                  <small className="settings-muted-line">{fixedModelUnavailableReason}</small>
                ) : null}
              </div>
              <div className="wide">
                <AgentCapabilityPolicyTable
                  disabled={props.busy}
                  emptyLabel={text("没有匹配工具", "No matching tools")}
                  mode={draft.toolPolicyMode}
                  onModeChange={(mode) => props.onDraftChange({
                    ...draft,
                    toolPolicyMode: mode,
                    toolPolicyTools: mode === "all" ? "" : draft.toolPolicyTools,
                  })}
                  onSelectedItemsChange={updateToolPolicyItems}
                  options={props.toolOptions}
                  searchPlaceholder={text("搜索工具", "Search tools")}
                  selectedItems={listFromText(draft.toolPolicyTools)}
                  title={text("工具策略", "Tool policy")}
                />
              </div>
              <div className="wide">
                <AgentCapabilityPolicyTable
                  disabled={props.busy}
                  emptyLabel={text("没有匹配 Prompt Skills", "No matching Prompt Skills")}
                  mode={draft.promptSkillPolicyMode}
                  onModeChange={(mode) => props.onDraftChange({
                    ...draft,
                    promptSkillPolicyMode: mode,
                    promptSkillPolicySkills: mode === "all" ? "" : draft.promptSkillPolicySkills,
                  })}
                  onSelectedItemsChange={updatePromptSkillPolicyItems}
                  options={props.promptSkillOptions}
                  searchPlaceholder={text("搜索 Prompt Skills", "Search Prompt Skills")}
                  selectedItems={listFromText(draft.promptSkillPolicySkills)}
                  title="Prompt Skills"
                />
              </div>
            </div>
            <div className="agent-editor-context">
              <span>{text("当前层级", "Current layer")}: {draft.sourceScope || text("用户", "User")}</span>
              {draft.definitionFile ? <span>{draft.definitionFile}</span> : null}
              {editingSystemBackedDefinition ? <strong>{text("仅保存用户侧元数据覆盖", "Only user metadata override will be saved")}</strong> : null}
            </div>
            <DialogFooter>
              <button className="skill-action-button secondary" disabled={props.busy} onClick={props.onCancel} type="button">
                <X size={15} />
                <span>{text("取消", "Cancel")}</span>
              </button>
              <button className="skill-action-button" disabled={props.busy || draft.agentId.trim().length === 0 || !preferredDraftMetadataField(localizedSaveLocalesFromDraft(draft), "name")} type="submit">
                <Save size={15} />
                <span>{text("保存", "Save")}</span>
              </button>
            </DialogFooter>
          </form>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

// AgentColorPicker renders a native palette picker for agent metadata.
// AgentColorPicker 渲染智能体元数据使用的原生调色板选择器。
function AgentColorPicker(props: {
  // Current color token or CSS color value.
  // 当前颜色标识或 CSS 颜色值。
  value: string
  // Whether the picker is disabled.
  // 选择器是否禁用。
  disabled: boolean
  // Updates the selected color token.
  // 更新选中的颜色标识。
  onChange: (value: string) => void
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedAgentLiteral(locale, zh, en), [locale])
  const onChange = props.onChange
  const inputRef = React.useRef<HTMLInputElement | null>(null)
  const dirtyRef = React.useRef(false)
  const normalizedPropColor = colorInputValue(props.value)
  // draftColor keeps native palette dragging local so the large agent editor does not rerender per pixel.
  // draftColor 将原生调色板拖动限制在本地，避免大型智能体编辑器逐像素重渲染。
  const [draftColor, setDraftColor] = React.useState(normalizedPropColor)

  React.useEffect(() => {
    setDraftColor(normalizedPropColor)
  }, [normalizedPropColor])

  React.useEffect(() => {
    const input = inputRef.current
    if (!input) {
      return
    }
    // handleNativeChange commits only the browser's final color change event.
    // handleNativeChange 只提交浏览器最终确认的颜色 change 事件。
    const handleNativeChange = () => {
      if (!dirtyRef.current) {
        return
      }
      dirtyRef.current = false
      onChange(input.value)
    }
    input.addEventListener("change", handleNativeChange)
    return () => input.removeEventListener("change", handleNativeChange)
  }, [onChange])

  const commitDraftColor = React.useCallback(() => {
    if (!dirtyRef.current) {
      return
    }
    dirtyRef.current = false
    onChange(inputRef.current?.value ?? draftColor)
  }, [draftColor, onChange])

  const displayValue = draftColor
  return (
    <div className="agent-editor-color-picker">
      <label className={`agent-editor-color-trigger ${props.disabled ? "disabled" : ""}`}>
        <span className="agent-editor-color-swatch" style={{ backgroundColor: draftColor }} />
        <span>{displayValue || text("选择颜色", "Choose color")}</span>
        <input
          aria-label={text("选择颜色", "Choose color")}
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

// AgentCapabilityPolicyTable renders one mode-plus-list policy selector.
// AgentCapabilityPolicyTable 渲染一个规则加列表策略选择器。
function AgentCapabilityPolicyTable(props: {
  // Current policy mode.
  // 当前策略模式。
  mode: CapabilityPolicyMode
  // Changes the current policy mode.
  // 修改当前策略模式。
  onModeChange: (mode: CapabilityPolicyMode) => void
  // Policy selectors currently listed by the mode.
  // 当前模式列出的策略选择器。
  selectedItems: string[]
  // Updates policy selectors.
  // 更新策略选择器。
  onSelectedItemsChange: (items: string[]) => void
  // Whether the table controls are disabled.
  // 表格控件是否禁用。
  disabled: boolean
  // Empty search result label.
  // 空搜索结果标签。
  emptyLabel: string
  // Capability rows that can be selected.
  // 可供选择的能力行。
  options: AgentCapabilityPolicyOption[]
  // Search input placeholder.
  // 搜索输入框占位文案。
  searchPlaceholder: string
  // Table title.
  // 表格标题。
  title: string
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedAgentLiteral(locale, zh, en), [locale])
  // filterText stores the current local search query.
  // filterText 保存当前本地搜索条件。
  const [filterText, setFilterText] = React.useState("")
  // selectedSet accelerates selected selector lookup.
  // selectedSet 加速已选择选择器查询。
  const selectedSet = React.useMemo(() => new Set(props.selectedItems), [props.selectedItems])
  // visibleOptions applies the local search filter to known tools.
  // visibleOptions 对已知工具应用本地搜索过滤。
  const visibleOptions = React.useMemo(() => {
    const query = filterText.trim().toLowerCase()
    if (query.length === 0) {
      return props.options
    }
    return props.options.filter((option) => (
      option.id.toLowerCase().includes(query)
      || option.label.toLowerCase().includes(query)
      || option.family.toLowerCase().includes(query)
      || option.description.toLowerCase().includes(query)
      || option.detail.toLowerCase().includes(query)
    ))
  }, [filterText, props.options])

  return (
    <section className="agent-tool-policy-table" aria-label={props.title}>
      <div className="agent-tool-policy-toolbar">
        <div>
          <strong>{props.title}</strong>
          <span>{agentPolicyModeSummary(props.mode, props.selectedItems.length, locale)}</span>
        </div>
        <div className="skill-tool-segmented agent-tool-policy-mode" role="group" aria-label={props.title}>
          <button
            aria-pressed={props.mode === "all"}
            className={props.mode === "all" ? "active" : undefined}
            disabled={props.disabled}
            onClick={() => props.onModeChange("all")}
            type="button"
          >
            {text("全部", "All")}
          </button>
          <button
            aria-pressed={props.mode === "allow_only"}
            className={props.mode === "allow_only" ? "active" : undefined}
            disabled={props.disabled}
            onClick={() => props.onModeChange("allow_only")}
            type="button"
          >
            {text("仅允许", "Allow only")}
          </button>
          <button
            aria-pressed={props.mode === "deny_only"}
            className={props.mode === "deny_only" ? "active" : undefined}
            disabled={props.disabled}
            onClick={() => props.onModeChange("deny_only")}
            type="button"
          >
            {text("仅禁止", "Deny only")}
          </button>
        </div>
        <input
          disabled={props.disabled}
          placeholder={props.searchPlaceholder}
          value={filterText}
          onChange={(event) => setFilterText(event.target.value)}
        />
      </div>
      <div className="agent-tool-policy-list">
        {visibleOptions.length === 0 ? (
          <div className="skill-tool-row">
            <strong>{props.emptyLabel}</strong>
          </div>
        ) : visibleOptions.map((option) => {
          const checked = selectedSet.has(option.id)
          return (
            <div className="skill-tool-row agent-tool-policy-row" key={option.id}>
              <div className="agent-tool-policy-main">
                <div>
                  <strong>{option.label}</strong>
                  <span>{option.family}</span>
                </div>
                <code>{option.id}</code>
                {option.description ? <p>{option.description}</p> : null}
                {option.detail ? <span>{option.detail}</span> : null}
              </div>
              <label className="agent-tool-policy-checkbox">
                <input
                  checked={checked}
                  disabled={props.disabled || props.mode === "all"}
                  type="checkbox"
                  onChange={(event) => {
                    props.onSelectedItemsChange(setPolicyItemSelection(props.selectedItems, option.id, event.target.checked))
                  }}
                />
                <span>{policySelectionLabel(props.mode, locale)}</span>
              </label>
            </div>
          )
        })}
      </div>
    </section>
  )
}

// setPolicyItemSelection returns a normalized selector list after one checkbox change.
// setPolicyItemSelection 返回单次勾选变化后的规范化选择器列表。
function setPolicyItemSelection(items: string[], itemId: string, selected: boolean): string[] {
  const nextItems = items.filter((item) => item !== itemId)
  if (selected) {
    nextItems.push(itemId)
  }
  return nextItems
}

// agentPolicyModeSummary returns compact human text for one policy mode.
// agentPolicyModeSummary 返回单个策略模式的紧凑人类可读文本。
function agentPolicyModeSummary(mode: CapabilityPolicyMode, count: number, locale: string): string {
  if (mode === "allow_only") {
    return localizedAgentLiteral(locale, `仅允许 ${count} 个`, `Allow only ${count}`)
  }
  if (mode === "deny_only") {
    return localizedAgentLiteral(locale, `仅禁止 ${count} 个`, `Deny only ${count}`)
  }
  return localizedAgentLiteral(locale, "全部", "All")
}

// policySelectionLabel returns the checkbox label for one policy mode.
// policySelectionLabel 返回单个策略模式下的复选框标签。
function policySelectionLabel(mode: CapabilityPolicyMode, locale: string): string {
  if (mode === "allow_only") {
    return localizedAgentLiteral(locale, "允许", "Allow")
  }
  if (mode === "deny_only") {
    return localizedAgentLiteral(locale, "禁止", "Deny")
  }
  return localizedAgentLiteral(locale, "全部", "All")
}

// AgentSourceOriginLink renders a display-only provenance link for an agent.
// AgentSourceOriginLink 渲染智能体展示专用的来源链接。
function AgentSourceOriginLink(props: {
  // Display-only source metadata.
  // 仅用于展示的来源元数据。
  origin?: SubagentOriginSummary | null
  // Whether this link is rendered inside the detail dialog.
  // 当前链接是否渲染在详情弹窗内。
  detail?: boolean
}) {
  const origin = props.origin
  const url = origin?.url?.trim()
  if (!origin || !url) {
    return null
  }
  const iconKey = agentSourceOriginIconKey(origin)
  const label = iconKey === "github" ? "Github" : origin.kind?.trim() || "source"
  const className = props.detail ? "agent-source-origin-detail-link" : "agent-source-origin-link"
  // handleClick opens the source URL through the same host shell path as the status bar repository link.
  // handleClick 通过与状态栏仓库链接相同的宿主壳路径打开来源地址。
  const handleClick = React.useCallback((event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault()
    event.stopPropagation()
    void openExternalUrl(url).catch((error) => {
      toast.error(`打开来源失败：${error instanceof Error ? error.message : String(error)}`)
    })
  }, [url])
  return (
    <a aria-label={url} className={className} href={url} onClick={handleClick} rel="noreferrer" target="_blank" title={url}>
      {agentSourceOriginIcon(origin)}
      <span>{label}</span>
    </a>
  )
}

// agentSourceOriginIconKey normalizes the source icon key.
// agentSourceOriginIconKey 归一化来源图标键。
function agentSourceOriginIconKey(origin: SubagentOriginSummary): string {
  return (origin.icon || origin.kind || "").trim().toLowerCase()
}

// agentSourceOriginIcon chooses the compact icon for display-only source metadata.
// agentSourceOriginIcon 为展示专用来源元数据选择紧凑图标。
function agentSourceOriginIcon(origin: SubagentOriginSummary): React.ReactNode {
  const iconKey = agentSourceOriginIconKey(origin)
  if (iconKey === "github") {
    return <Github size={13} />
  }
  if (iconKey === "system") {
    return <Bot size={13} />
  }
  return <ExternalLink size={13} />
}

// AgentDefinitionCard renders one agent definition row.
// AgentDefinitionCard 渲染单个智能体定义行。
function AgentDefinitionCard(props: {
  // Service-backed agent page actions.
  // 服务端支持的智能体页面动作。
  actions: AgentActionProps
  // Whether another catalog action is running.
  // 是否已有其他目录操作正在运行。
  busy: boolean
  // Localized categories keyed by id.
  // 按标识索引的本地化分类。
  categoryById: Map<string, SubagentCategorySummary>
  // Definition row rendered by this card.
  // 当前卡片渲染的定义行。
  definition: SubagentDefinitionSummary
  // Opens this definition in the editor.
  // 在编辑器中打开当前定义。
  onEdit: () => void
  // Opens this definition in the detail dialog.
  // 在详情弹窗中打开当前定义。
  onOpenDetail: (definition: SubagentDefinitionSummary) => void
  // Runs a labeled async action with page-level busy state.
  // 使用页面级忙碌状态运行带标签的异步动作。
  onRunAction: (label: string, action: () => Promise<void>) => Promise<void>
  // Runs one enabled-state action with row-level busy state.
  // 使用行级忙碌状态运行单个启停动作。
  onRunToggleAction: (agentId: string, action: () => Promise<void>) => Promise<void>
  // Whether this row is currently persisting its enabled state.
  // 当前行是否正在持久化启停状态。
  toggleBusy: boolean
}) {
  const { locale, t } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedAgentLiteral(locale, zh, en), [locale])
  const definition = props.definition
  const displayName = localizedDefinitionName(definition, locale)
  const displayDescription = localizedDefinitionDescription(definition, locale)
  const category = definition.categoryId ? props.categoryById.get(definition.categoryId) : undefined
  const categoryLabel = category ? localizedCategoryName(category, locale) : definition.categoryId
  const fallbackIcon = definition.emoji?.trim() || displayName.trim().charAt(0).toUpperCase() || "*"
  const userOwnedDefinition = (definition.scopeKey === "user" || definition.scopeKey === "package") && !definition.overrideOf
  const editableDefinition = userOwnedDefinition || definition.scopeKey === "system" || Boolean(definition.overrideOf)
  const toolAccessLabel = agentDefinitionToolAccessLabel(definition, locale)
  const fixedModelSelector = agentDefinitionFixedModelSelector(definition)
  const fixedModelUnavailable = agentDefinitionFixedModelUnavailable(definition)
  const fixedModelTitle = agentDefinitionFixedModelTitle(definition, locale)
  return (
    <article className="agent-definition-row">
      <button className="agent-definition-row-icon" onClick={() => props.onOpenDetail(definition)} title={t("skills.action.details")} type="button">
        <span className="agent-definition-row-glyph">{fallbackIcon}</span>
        <span className="agent-definition-row-color-marker" style={{ backgroundColor: colorSwatchValue(definition.color) }} />
      </button>
      <div className="agent-definition-row-main">
        <div className="agent-definition-row-title-line">
          <button className="agent-definition-title-button" onClick={() => props.onOpenDetail(definition)} type="button">{displayName}</button>
          <div className="agent-definition-row-badges">
            <Badge variant="outline">{definition.scope}</Badge>
            <Badge variant="outline">{toolAccessLabel}</Badge>
            {definition.categoryId ? <Badge variant="outline" title={definition.categoryId}>{categoryLabel}</Badge> : null}
            {definition.packageId ? <Badge variant="outline">{definition.packageId}</Badge> : null}
            {definition.remoteUrl ? <Badge variant="outline">Remote</Badge> : null}
            {fixedModelSelector ? (
              <Badge variant={fixedModelUnavailable ? "destructive" : "outline"} title={fixedModelTitle}>
                {fixedModelUnavailable ? text("固定模型不可用", "Fixed model unavailable") : text("固定模型", "Fixed model")}
              </Badge>
            ) : null}
            <AgentSourceOriginLink origin={definition.sourceOrigin} />
          </div>
        </div>
        <p>{displayDescription || text("未填写描述。", "No description provided.")}</p>
        <span className="agent-definition-row-path">{definition.definitionFile}</span>
      </div>
      <div className="agent-definition-row-actions">
        <button
          aria-checked={definition.enabled}
          aria-label={definition.enabled ? t("skills.action.disable") : t("skills.action.enable")}
          className={`agent-definition-switch ${definition.enabled ? "on" : ""}`}
          disabled={props.busy || props.toggleBusy}
          onClick={() => void props.onRunToggleAction(definition.agentId, () => props.actions.onSetAgentDefinitionEnabled(definition.agentId, !definition.enabled))}
          role="switch"
          title={definition.enabled ? t("skills.action.disable") : t("skills.action.enable")}
          type="button"
        >
          <span />
        </button>
        {editableDefinition ? (
          <button className="agent-definition-icon-button" disabled={props.busy} onClick={props.onEdit} title={text("编辑", "Edit")} type="button">
            <Pencil size={15} />
          </button>
        ) : null}
        {userOwnedDefinition ? (
          <button
            className="agent-definition-icon-button danger"
            disabled={props.busy}
            onClick={() => void props.onRunAction("delete", () => props.actions.onDeleteAgentDefinition(definition.agentId))}
            title={text("删除", "Delete")}
            type="button"
          >
            <Trash2 size={15} />
          </button>
        ) : null}
      </div>
    </article>
  )
}

// AgentDefinitionDetailDialog renders metadata and prompt previews for one agent definition.
// AgentDefinitionDetailDialog 渲染单个智能体定义的元数据与提示词预览。
function AgentDefinitionDetailDialog(props: {
  // Definition selected for details.
  // 当前查看详情的智能体定义。
  definition: SubagentDefinitionSummary | null
  // Whether the details dialog is open.
  // 详情弹窗当前是否打开。
  open: boolean
  // Updates the details dialog open state.
  // 更新详情弹窗打开状态。
  onOpenChange: (open: boolean) => void
}) {
  const { locale, t } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedAgentLiteral(locale, zh, en), [locale])
  const definition = props.definition
  const displayName = definition ? localizedDefinitionName(definition, locale) : null
  const modelDetail = definition ? agentDefinitionProviderModelDetail(definition, locale) : null
  return (
    <Dialog open={props.open && definition !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="agent-skill-detail-dialog">
        <DialogHeader>
          <DialogTitle>{displayName ?? text("智能体详情", "Agent details")}</DialogTitle>
          <DialogDescription>{definition ? definition.scope : text("查看智能体定义。", "Inspect the agent definition.")}</DialogDescription>
        </DialogHeader>
        {definition ? (
          <div className="agent-skill-detail-body">
            <div className="lua-skill-detail-grid">
              <div>
                <span>{t("skills.label.currentStatus")}</span>
                <strong>{definition.enabled ? t("skills.status.enabled") : t("skills.status.stopped")}</strong>
              </div>
              <div>
                <span>{text("来源", "Scope")}</span>
                <strong>{definition.scope}</strong>
              </div>
              <div>
                <span>{text("工具策略", "Tool policy")}</span>
                <strong>{definition.allowedToolCount} allow / {definition.disabledToolCount} deny</strong>
              </div>
              <div>
                <span>Prompt Skills</span>
                <strong>{definition.enabledPromptSkillCount} allow / {definition.disabledPromptSkillCount} deny</strong>
              </div>
              <div className="wide">
                <span>{text("模型", "Model")}</span>
                <strong title={modelDetail?.title}>{modelDetail?.label}</strong>
              </div>
              <div className="wide">
                <span>{text("定义文件", "Definition file")}</span>
                <strong>{definition.definitionFile}</strong>
              </div>
              {definition.remoteUrl ? (
                <div className="wide">
                  <span>Remote URL</span>
                  <strong>{definition.remoteUrl}</strong>
                </div>
              ) : null}
              {definition.sourceOrigin?.url ? (
                <div className="wide">
                  <span>{text("展示来源", "Display source")}</span>
                  <strong>
                    <AgentSourceOriginLink detail origin={definition.sourceOrigin} />
                  </strong>
                </div>
              ) : null}
              {definition.overrideOf ? (
                <div className="wide">
                  <span>{text("覆盖目标", "Override target")}</span>
                  <strong>{definition.overrideOf}</strong>
                </div>
              ) : null}
            </div>
            {definition.capabilityDiagnostics.length > 0 ? (
              <div className="skill-tool-row">
                <strong>{text("能力诊断", "Capability diagnostics")}</strong>
                <p>{definition.capabilityDiagnostics.join(" ")}</p>
              </div>
            ) : null}
            {definition.systemPrompt ? (
              <div className="skill-tool-row">
                <strong>{text("系统提示词", "System prompt")}</strong>
                <pre className="skill-body-preview">{definition.systemPrompt}</pre>
              </div>
            ) : null}
            {definition.systemPromptAppend ? (
              <div className="skill-tool-row">
                <strong>{text("追加提示词", "Prompt append")}</strong>
                <pre className="skill-body-preview">{definition.systemPromptAppend}</pre>
              </div>
            ) : null}
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

// emptyAgentDraft builds the default draft for a new user agent.
// emptyAgentDraft 构建新用户智能体的默认草稿。
function emptyAgentDraft(metadataLocale: AgentMetadataLocale): AgentEditorDraft {
  return {
    agentId: "",
    originalAgentId: "",
    packageId: "",
    name: "",
    description: "",
    metadataLocale,
    localizedNames: {},
    localizedDescriptions: {},
    emoji: "🤖",
    color: "gray",
    vibe: "",
    enabled: true,
    systemPrompt: "",
    systemPromptAppend: "",
    fixedModelProviderId: "",
    fixedModelId: "",
    fixedModelAvailable: null,
    fixedModelUnavailableReason: "",
    toolPolicyMode: "all",
    toolPolicyTools: "",
    promptSkillPolicyMode: "all",
    promptSkillPolicySkills: "",
    existing: false,
    sourceScope: "用户",
    sourceScopeKey: "user",
    overrideOf: "",
    definitionFile: "",
  }
}

// draftFromDefinition converts one catalog row into editable form state.
// draftFromDefinition 将单个目录行转换为可编辑表单状态。
function draftFromDefinition(definition: SubagentDefinitionSummary, metadataLocale: AgentMetadataLocale): AgentEditorDraft {
  const localizedNames = localizedFieldMapFromDefinition(definition, "name", metadataLocale)
  const localizedDescriptions = localizedFieldMapFromDefinition(definition, "description", metadataLocale)
  return {
    agentId: definition.agentId,
    originalAgentId: definition.agentId,
    packageId: definition.packageId ?? "",
    name: definition.name,
    description: definition.description ?? "",
    metadataLocale,
    localizedNames,
    localizedDescriptions,
    emoji: definition.emoji || "🤖",
    color: definition.color || "gray",
    vibe: definition.vibe ?? "",
    enabled: definition.enabled,
    systemPrompt: definition.systemPrompt ?? "",
    systemPromptAppend: definition.systemPromptAppend ?? "",
    fixedModelProviderId: definition.providerModel?.providerId ?? "",
    fixedModelId: definition.providerModel?.modelId ?? "",
    fixedModelAvailable: definition.providerModel?.available ?? null,
    fixedModelUnavailableReason: definition.providerModel?.unavailableReason ?? "",
    toolPolicyMode: normalizedCapabilityPolicyMode(definition.toolPolicyMode, definition.allowedTools, definition.disabledTools),
    toolPolicyTools: listText(definition.toolPolicyTools?.length ? definition.toolPolicyTools : legacyPolicyTools(definition.allowedTools, definition.disabledTools)),
    promptSkillPolicyMode: normalizedCapabilityPolicyMode(definition.promptSkillPolicyMode, definition.enabledPromptSkills, definition.disabledPromptSkills),
    promptSkillPolicySkills: listText(definition.promptSkillPolicySkills?.length ? definition.promptSkillPolicySkills : legacyPolicyTools(definition.enabledPromptSkills, definition.disabledPromptSkills)),
    existing: true,
    sourceScope: definition.scope,
    sourceScopeKey: definition.scopeKey,
    overrideOf: definition.overrideOf ?? "",
    definitionFile: definition.definitionFile,
  }
}

// saveRequestFromDraft converts form state into the service request shape.
// saveRequestFromDraft 将表单状态转换为服务请求结构。
function saveRequestFromDraft(draft: AgentEditorDraft): SubagentDefinitionSaveRequest {
  const normalizedDraft = normalizedAgentEditorDraftFileIdentity(draft)
  const locales = localizedSaveLocalesFromDraft(draft)
  const name = preferredDraftMetadataField(locales, "name") ?? draft.name.trim()
  const description = preferredDraftMetadataField(locales, "description") ?? nonEmptyText(draft.description)
  return {
    agentId: normalizedDraft.agentId,
    originalAgentId: nonEmptyText(normalizedDraft.originalAgentId),
    packageId: nonEmptyText(normalizedDraft.packageId),
    name,
    description,
    locales,
    emoji: nullableTextPatch(draft.emoji),
    color: nullableTextPatch(draft.color),
    vibe: nullableTextPatch(draft.vibe),
    enabled: draft.enabled,
    systemPrompt: nonEmptyText(draft.systemPrompt),
    systemPromptAppend: nonEmptyText(draft.systemPromptAppend),
    providerModel: {
      providerId: nonEmptyText(draft.fixedModelProviderId),
      modelId: nonEmptyText(draft.fixedModelId),
    },
    allowedTools: [],
    disabledTools: [],
    toolPolicyMode: draft.toolPolicyMode,
    toolPolicyTools: listFromText(draft.toolPolicyTools),
    allowedAiCapabilities: [],
    disabledAiCapabilities: [],
    luaskillsOverrides: null,
    mcpServerOverrides: null,
    enabledPromptSkills: [],
    disabledPromptSkills: [],
    promptSkillPolicyMode: draft.promptSkillPolicyMode,
    promptSkillPolicySkills: listFromText(draft.promptSkillPolicySkills),
  }
}

// normalizedAgentEditorDraftFileIdentity normalizes file-backed identity fields before save.
// normalizedAgentEditorDraftFileIdentity 在保存前规范化文件承载的标识字段。
function normalizedAgentEditorDraftFileIdentity(draft: AgentEditorDraft): AgentEditorDraft {
  return {
    ...draft,
    agentId: normalizeAgentFileNameInput(draft.agentId),
    originalAgentId: normalizeAgentFileNameInput(draft.originalAgentId),
    packageId: normalizeAgentPackageIdInput(draft.packageId),
  }
}

// modelPickerChoiceExists returns whether a provider/model key is currently selectable.
// modelPickerChoiceExists 返回某个供应商/模型键当前是否可被选择。
function modelPickerChoiceExists(providers: WorkbenchSnapshot["providers"], value: string): boolean {
  return providers.some((provider) => (provider.models ?? []).some((model) => modelPickerChoiceKey({
    providerId: provider.id,
    providerName: provider.name,
    modelId: model.id,
    modelName: model.name,
    active: Boolean(model.active),
  }) === value))
}

// normalizeAgentFileNameInput keeps the editable value as a file stem without a trailing extension.
// normalizeAgentFileNameInput 将可编辑值保持为不带尾部扩展名的文件 stem。
function normalizeAgentFileNameInput(value: string): string {
  const normalizedSeparators = value.replace(/\\/g, "/")
  const filtered = normalizedSeparators.replace(/[^A-Za-z0-9._/-]/g, "")
  const segments = filtered
    .split("/")
    .filter((segment, index, segments) => segment.length > 0 || index < segments.length - 1)
  const lastSegmentIndex = segments.length - 1
  if (lastSegmentIndex >= 0) {
    segments[lastSegmentIndex] = stripAgentFileSegmentExtension(segments[lastSegmentIndex])
  }
  return segments.join("/").replace(/^\/+/, "")
}

// normalizeAgentPackageIdInput applies the package id character contract used by the form.
// normalizeAgentPackageIdInput 应用表单使用的用户包标识字符约束。
function normalizeAgentPackageIdInput(value: string): string {
  return value.replace(/[^A-Za-z0-9._-]/g, "")
}

// stripAgentFileSegmentExtension removes the last extension-like suffix from one path segment.
// stripAgentFileSegmentExtension 从单个路径片段移除最后一个扩展名形式的后缀。
function stripAgentFileSegmentExtension(segment: string): string {
  const dotIndex = segment.lastIndexOf(".")
  if (dotIndex <= 0) {
    return segment
  }
  return segment.slice(0, dotIndex)
}

// targetAgentIdFromDraft mirrors the service-side package assignment for duplicate checks.
// targetAgentIdFromDraft 镜像服务端包分配规则，用于重复检测。
function targetAgentIdFromDraft(draft: AgentEditorDraft): string {
  const packageId = normalizeAgentPackageIdInput(draft.packageId)
  let baseId = normalizeAgentFileNameInput(draft.agentId)
  if (!packageId) {
    return baseId
  }
  const packagePrefix = `${packageId}/`
  if (baseId.startsWith(packagePrefix)) {
    baseId = baseId.slice(packagePrefix.length)
  }
  return `${packagePrefix}${baseId}`
}

// normalizedCapabilityPolicyMode resolves a modern policy mode with legacy list fallback.
// normalizedCapabilityPolicyMode 解析新版策略模式，并用旧列表作为回退。
function normalizedCapabilityPolicyMode(mode: CapabilityPolicyMode | null | undefined, legacyAllowed: string[] = [], legacyDisabled: string[] = []): CapabilityPolicyMode {
  if (mode === "all" || mode === "allow_only" || mode === "deny_only") {
    return mode
  }
  if (legacyAllowed.length > 0) {
    return "allow_only"
  }
  if (legacyDisabled.length > 0) {
    return "deny_only"
  }
  return "all"
}

// legacyPolicyTools returns the selector list implied by legacy allow/deny lists.
// legacyPolicyTools 返回旧版允许/禁止列表隐含的选择器列表。
function legacyPolicyTools(legacyAllowed: string[] = [], legacyDisabled: string[] = []): string[] {
  if (legacyAllowed.length > 0) {
    const disabledSet = new Set(legacyDisabled.map((item) => item.trim().toLowerCase()))
    return legacyAllowed.filter((item) => !disabledSet.has(item.trim().toLowerCase()))
  }
  return legacyDisabled
}

// localizedFieldMapFromDefinition builds editable locale text maps from one definition row.
// localizedFieldMapFromDefinition 从单个定义行构建可编辑的本地化文本映射。
function localizedFieldMapFromDefinition(definition: SubagentDefinitionSummary, field: "name" | "description", fallbackLocale: AgentMetadataLocale): Record<string, string> {
  const values: Record<string, string> = {}
  const valuePriorities: Record<string, number> = {}
  Object.entries(definition.locales ?? {}).forEach(([locale, metadata]) => {
    const value = metadata?.[field]?.trim()
    const normalizedLocale = normalizedEditableMetadataLocale(locale) ?? locale.trim()
    if (value && normalizedLocale) {
      const priority = editableMetadataLocalePriority(locale, normalizedLocale)
      if (!values[normalizedLocale] || priority > (valuePriorities[normalizedLocale] ?? 0)) {
        values[normalizedLocale] = value
        valuePriorities[normalizedLocale] = priority
      }
    }
  })
  const rootValue = field === "name" ? definition.name.trim() : definition.description?.trim()
  if (Object.keys(values).length > 0) {
    if (rootValue && !localizedFieldMapHasLanguage(values, "en")) {
      values["en-US"] = rootValue
    }
    return values
  }
  if (rootValue) {
    values[fallbackLocale] = rootValue
    if (!localizedFieldMapHasLanguage(values, "en")) {
      values["en-US"] = rootValue
    }
  }
  return values
}

// localizedFieldMapHasLanguage returns whether a locale map already contains a language prefix.
// localizedFieldMapHasLanguage 返回 locale 映射是否已包含指定语言前缀。
function localizedFieldMapHasLanguage(values: Record<string, string>, language: string): boolean {
  const normalizedLanguage = language.toLowerCase()
  return Object.keys(values).some((locale) => {
    const normalizedLocale = locale.toLowerCase()
    return normalizedLocale === normalizedLanguage || normalizedLocale.startsWith(`${normalizedLanguage}-`)
  })
}

// localizedSaveLocalesFromDraft converts editable locale maps into the save request payload.
// localizedSaveLocalesFromDraft 将可编辑本地化映射转换为保存请求载荷。
function localizedSaveLocalesFromDraft(draft: AgentEditorDraft): Record<string, { name?: string | null; description?: string | null }> {
  const locales: Record<string, { name?: string | null; description?: string | null }> = {}
  const localePriorities: Record<string, number> = {}
  const localeKeys = new Set([...Object.keys(draft.localizedNames), ...Object.keys(draft.localizedDescriptions)])
  localeKeys.forEach((locale) => {
    const normalizedLocale = normalizedEditableMetadataLocale(locale) ?? locale.trim()
    if (!normalizedLocale) {
      return
    }
    const name = draft.localizedNames[locale]?.trim()
    const description = draft.localizedDescriptions[locale]?.trim()
    const priority = editableMetadataLocalePriority(locale, normalizedLocale)
    if (name || description) {
      if (priority < (localePriorities[normalizedLocale] ?? 0)) {
        return
      }
      locales[normalizedLocale] = {
        name: name || null,
        description: description || null,
      }
      localePriorities[normalizedLocale] = priority
    }
  })
  return locales
}

// normalizedEditableMetadataLocale maps compatible locale keys into the editor's canonical tabs.
// normalizedEditableMetadataLocale 将兼容语言键映射到编辑器的规范标签页。
function normalizedEditableMetadataLocale(locale: string): AgentMetadataLocale | null {
  const normalizedLocale = locale.trim().toLowerCase()
  if (normalizedLocale === "en" || normalizedLocale.startsWith("en-")) {
    return "en-US"
  }
  if (normalizedLocale === "zh" || normalizedLocale.startsWith("zh-")) {
    return "zh-CN"
  }
  return null
}

// editableMetadataLocalePriority prefers exact canonical metadata over compatible aliases.
// editableMetadataLocalePriority 优先使用精确规范语言元数据而不是兼容别名。
function editableMetadataLocalePriority(sourceLocale: string, targetLocale: string): number {
  const normalizedSourceLocale = sourceLocale.trim().toLowerCase()
  const normalizedTargetLocale = targetLocale.trim().toLowerCase()
  if (normalizedSourceLocale === normalizedTargetLocale) {
    return 3
  }
  const targetLanguage = normalizedTargetLocale.split("-")[0]
  if (normalizedSourceLocale === targetLanguage) {
    return 2
  }
  if (normalizedSourceLocale.startsWith(`${targetLanguage}-`)) {
    return 1
  }
  return 0
}

// preferredDraftMetadataField returns English metadata first and Chinese as fallback.
// preferredDraftMetadataField 返回英文优先、中文兜底的元数据字段。
function preferredDraftMetadataField(
  locales: Record<string, { name?: string | null; description?: string | null }>,
  field: "name" | "description",
): string | null {
  return nonEmptyText(locales["en-US"]?.[field] ?? "")
    ?? nonEmptyText(locales.en?.[field] ?? "")
    ?? nonEmptyText(locales["zh-CN"]?.[field] ?? "")
    ?? nonEmptyText(locales.zh?.[field] ?? "")
}

// defaultAgentMetadataLocale returns the form language that matches the UI locale.
// defaultAgentMetadataLocale 返回匹配 UI 语言的表单元数据语言。
function defaultAgentMetadataLocale(locale: string): AgentMetadataLocale {
  return locale.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US"
}

// buildAgentToolPolicyOptions builds selectable tool rows from the live workbench snapshot and draft policies.
// buildAgentToolPolicyOptions 从实时工作台快照与草稿策略构建可选择工具行。
function buildAgentToolPolicyOptions(workbench: WorkbenchSnapshot, draft: AgentEditorDraft | null, locale: string): AgentCapabilityPolicyOption[] {
  // optionMap keeps one row per persisted tool id.
  // optionMap 确保每个持久化工具标识只保留一行。
  const optionMap = new Map<string, AgentCapabilityPolicyOption>()
  workbench.hostTools?.nativeTools?.forEach((tool) => {
    const toolId = tool.id.trim()
    if (toolId.length === 0) {
      return
    }
    appendAgentCapabilityPolicyOption(optionMap, {
      id: toolId,
      label: tool.name || toolId,
      family: localizedAgentLiteral(locale, "原生工具", "Native"),
      description: tool.description,
      detail: tool.aiVisible
        ? localizedAgentLiteral(locale, "默认对 AI 可见", "AI-visible by default")
        : localizedAgentLiteral(locale, "运行时注册，但默认不直接对 AI 可见", "Runtime-registered, but not directly AI-visible by default"),
    })
  })
  workbench.luaSkills.skills.forEach((skill) => {
    skill.tools.forEach((tool) => {
      const toolId = tool.canonicalName.trim()
      if (toolId.length === 0) {
        return
      }
      appendAgentCapabilityPolicyOption(optionMap, {
        id: toolId,
        label: toolId,
        family: "LuaSkills",
        description: tool.description,
        detail: [skill.displayName || skill.skillId, tool.localName, tool.registrationLabel].filter(Boolean).join(" · "),
      })
    })
  })
  const serverNameById = new Map((workbench.mcpServers ?? []).map((server) => [server.id, server.name || server.id]))
  workbench.mcpTools?.forEach((tool) => {
    const toolId = tool.id.trim()
    if (toolId.length === 0) {
      return
    }
    const serverLabel = serverNameById.get(tool.serverId) ?? tool.serverId
    appendAgentCapabilityPolicyOption(optionMap, {
      id: toolId,
      label: tool.id || tool.name || toolId,
      family: "MCP",
      description: tool.description || localizedAgentLiteral(locale, "当前 MCP 工具未提供说明。", "This MCP tool does not provide a description."),
      detail: `${serverLabel} · ${tool.originToolId || tool.name || toolId}`,
    })
  })
  if (draft) {
    const configuredDescription = localizedAgentLiteral(
      locale,
      "已保存到当前智能体，但当前工作台快照未解析到对应工具。",
      "Saved on this agent, but not resolved in the current workbench snapshot.",
    )
    listFromText(draft.toolPolicyTools).forEach((toolId) => {
      appendAgentCapabilityPolicyOption(optionMap, {
        id: toolId,
        label: toolId,
        family: localizedAgentLiteral(locale, "已配置", "Configured"),
        description: configuredDescription,
        detail: "",
      })
    })
  }
  return Array.from(optionMap.values()).sort((left, right) => {
    const familyComparison = left.family.localeCompare(right.family)
    if (familyComparison !== 0) {
      return familyComparison
    }
    return left.label.localeCompare(right.label)
  })
}

// buildAgentPromptSkillPolicyOptions builds selectable prompt skill rows from the Agent Skills catalog and draft policies.
// buildAgentPromptSkillPolicyOptions 从 Agent Skills 目录与草稿策略构建可选择提示词技能行。
function buildAgentPromptSkillPolicyOptions(workbench: WorkbenchSnapshot, draft: AgentEditorDraft | null, locale: string): AgentCapabilityPolicyOption[] {
  // optionMap keeps one row per persisted prompt skill name.
  // optionMap 确保每个持久化提示词技能名称只保留一行。
  const optionMap = new Map<string, AgentCapabilityPolicyOption>()
  workbench.agentSkills.skills.forEach((skill) => {
    const skillName = skill.name.trim()
    if (skillName.length === 0) {
      return
    }
    appendAgentCapabilityPolicyOption(optionMap, {
      id: skillName,
      label: skill.displayName || skillName,
      family: skill.scope || "Agent Skills",
      description: skill.description,
      detail: skill.enabled
        ? localizedAgentLiteral(locale, "当前启用", "Currently enabled")
        : localizedAgentLiteral(locale, "当前停用，策略启用不会覆盖目录停用状态", "Currently disabled; a policy enable will not override catalog disabled state"),
    })
  })
  if (draft) {
    const configuredDescription = localizedAgentLiteral(
      locale,
      "已保存到当前智能体，但当前 Agent Skills 目录未解析到对应技能。",
      "Saved on this agent, but not resolved in the current Agent Skills catalog.",
    )
    listFromText(draft.promptSkillPolicySkills).forEach((skillName) => {
      appendAgentCapabilityPolicyOption(optionMap, {
        id: skillName,
        label: skillName,
        family: localizedAgentLiteral(locale, "已配置", "Configured"),
        description: configuredDescription,
        detail: "",
      })
    })
  }
  return Array.from(optionMap.values()).sort((left, right) => {
    const familyComparison = left.family.localeCompare(right.family)
    if (familyComparison !== 0) {
      return familyComparison
    }
    return left.label.localeCompare(right.label)
  })
}

// appendAgentCapabilityPolicyOption inserts an option only when its id is not present yet.
// appendAgentCapabilityPolicyOption 仅在标识不存在时插入能力选项。
function appendAgentCapabilityPolicyOption(optionMap: Map<string, AgentCapabilityPolicyOption>, option: AgentCapabilityPolicyOption): void {
  if (!optionMap.has(option.id)) {
    optionMap.set(option.id, option)
  }
}

// sortedAgentCategories returns stable category rows for the filter menu.
// sortedAgentCategories 返回用于筛选菜单的稳定分类行。
function sortedAgentCategories(categories: SubagentCategorySummary[], locale: string): SubagentCategorySummary[] {
  return [...categories].sort((left, right) => {
    const leftOrder = left.order ?? Number.MAX_SAFE_INTEGER
    const rightOrder = right.order ?? Number.MAX_SAFE_INTEGER
    if (leftOrder !== rightOrder) {
      return leftOrder - rightOrder
    }
    return localizedCategoryName(left, locale).localeCompare(localizedCategoryName(right, locale))
      || left.categoryId.localeCompare(right.categoryId)
  })
}

// localizedDefinitionName resolves the agent name for the active locale.
// localizedDefinitionName 解析当前语言下的智能体名称。
function localizedDefinitionName(definition: SubagentDefinitionSummary, locale: string): string {
  const localizedName = localizedDefinitionText(definition, locale, "name")
  if (localizedName) {
    return localizedName
  }
  return definition.name || definition.agentId
}

// localizedDefinitionDescription resolves the agent description for the active locale.
// localizedDefinitionDescription 解析当前语言下的智能体说明。
function localizedDefinitionDescription(definition: SubagentDefinitionSummary, locale: string): string | null {
  return localizedDefinitionText(definition, locale, "description") ?? definition.description ?? null
}

// agentDefinitionMatchesStatusFilter returns whether a definition passes the enabled-state filter.
// agentDefinitionMatchesStatusFilter 返回定义是否通过启停状态筛选。
function agentDefinitionMatchesStatusFilter(definition: SubagentDefinitionSummary, filter: AgentStatusFilter): boolean {
  if (filter === "enabled") {
    return definition.enabled
  }
  if (filter === "disabled") {
    return !definition.enabled
  }
  return true
}

// agentDefinitionMatchesSearch returns whether all keyword tokens match cross-locale metadata.
// agentDefinitionMatchesSearch 返回所有关键词是否命中跨语言元数据。
function agentDefinitionMatchesSearch(definition: SubagentDefinitionSummary, query: string): boolean {
  const tokens = normalizedAgentSearchTokens(query)
  if (tokens.length === 0) {
    return true
  }
  const searchableText = agentDefinitionSearchableText(definition)
  return tokens.every((token) => searchableText.includes(token))
}

// normalizedAgentSearchTokens normalizes one search input into lowercase keyword tokens.
// normalizedAgentSearchTokens 将搜索输入规范化为小写关键词。
function normalizedAgentSearchTokens(query: string): string[] {
  return query
    .trim()
    .toLocaleLowerCase()
    .split(/\s+/)
    .map((token) => token.trim())
    .filter(Boolean)
}

// agentDefinitionSearchableText builds a cross-locale search haystack without changing display language.
// agentDefinitionSearchableText 构建跨语言搜索文本且不改变展示语言。
function agentDefinitionSearchableText(definition: SubagentDefinitionSummary): string {
  const values = [
    definition.name,
    definition.description ?? "",
    ...Object.values(definition.locales ?? {}).flatMap((metadata) => [
      metadata?.name ?? "",
      metadata?.description ?? "",
    ]),
  ]
  return values.join("\n").toLocaleLowerCase()
}

// agentDefinitionToolAccessLabel returns the compact tool access summary for one row.
// agentDefinitionToolAccessLabel 返回单行使用的紧凑工具暴露摘要。
function agentDefinitionToolAccessLabel(definition: SubagentDefinitionSummary, locale: string): string {
  return localizedAgentToolPolicyLabel(definition.toolPolicyMode, definition.toolPolicyTools.length, locale)
}

// agentDefinitionFixedModelSelector returns the fixed provider/model selector for one definition.
// agentDefinitionFixedModelSelector 返回单个定义的固定供应商/模型选择器。
function agentDefinitionFixedModelSelector(definition: SubagentDefinitionSummary): string | null {
  const selector = definition.providerModel?.selector?.trim()
  if (selector) {
    return selector
  }
  const providerId = definition.providerModel?.providerId?.trim()
  const modelId = definition.providerModel?.modelId?.trim()
  return providerId && modelId ? `${providerId}/${modelId}` : null
}

// agentDefinitionFixedModelUnavailable returns whether the configured fixed model is unavailable.
// agentDefinitionFixedModelUnavailable 返回已配置固定模型当前是否不可用。
function agentDefinitionFixedModelUnavailable(definition: SubagentDefinitionSummary): boolean {
  return Boolean(agentDefinitionFixedModelSelector(definition) && definition.providerModel?.available === false)
}

// agentDefinitionFixedModelTitle returns the tooltip text for a fixed-model badge.
// agentDefinitionFixedModelTitle 返回固定模型徽标的悬停提示文本。
function agentDefinitionFixedModelTitle(definition: SubagentDefinitionSummary, locale: string): string {
  const selector = agentDefinitionFixedModelSelector(definition)
  if (!selector) {
    return localizedAgentLiteral(locale, "跟随会话模型", "Follow session model")
  }
  const unavailableReason = definition.providerModel?.unavailableReason?.trim()
  if (definition.providerModel?.available === false && unavailableReason) {
    return `${selector} - ${unavailableReason}`
  }
  return selector
}

// agentDefinitionProviderModelDetail returns detail-dialog copy for one definition's model binding.
// agentDefinitionProviderModelDetail 返回详情弹窗中单个定义的模型绑定文案。
function agentDefinitionProviderModelDetail(definition: SubagentDefinitionSummary, locale: string): { label: string; title: string } {
  const selector = agentDefinitionFixedModelSelector(definition)
  if (!selector) {
    const label = localizedAgentLiteral(locale, "跟随会话模型", "Follow session model")
    return { label, title: label }
  }
  const unavailableReason = definition.providerModel?.unavailableReason?.trim()
  if (definition.providerModel?.available === false) {
    const label = localizedAgentLiteral(locale, `固定模型不可用：${selector}`, `Fixed model unavailable: ${selector}`)
    return { label, title: unavailableReason ? `${selector} - ${unavailableReason}` : selector }
  }
  const label = localizedAgentLiteral(locale, `固定模型：${selector}`, `Fixed model: ${selector}`)
  return { label, title: selector }
}

// localizedAgentToolPolicyLabel returns the compact row badge for tool policy.
// localizedAgentToolPolicyLabel 返回工具策略的紧凑行徽标。
function localizedAgentToolPolicyLabel(mode: CapabilityPolicyMode, count: number, locale: string): string {
  if (mode === "allow_only") {
    return localizedAgentLiteral(locale, `仅允许 ${count} 个工具`, `Allow ${count} tools`)
  }
  if (mode === "deny_only") {
    return localizedAgentLiteral(locale, `仅禁止 ${count} 个工具`, `Deny ${count} tools`)
  }
  return localizedAgentLiteral(locale, "全部工具", "All tools")
}

// localizedDefinitionText selects exact or language-prefix metadata from definition locales.
// localizedDefinitionText 从定义 locales 中选择精确或语言前缀匹配的元数据。
function localizedDefinitionText(definition: SubagentDefinitionSummary, locale: string, field: "name" | "description"): string | null {
  const locales = definition.locales ?? {}
  return localizedMetadataText(locales, locale, field)
}

// localizedCategoryName resolves the category name for the active locale.
// localizedCategoryName 解析当前语言下的分类名称。
function localizedCategoryName(category: SubagentCategorySummary, locale: string): string {
  const localizedName = localizedCategoryText(category, locale, "name")
  if (localizedName) {
    return localizedName
  }
  return category.name || category.categoryId
}

// localizedCategoryDescription resolves the category description for the active locale.
// localizedCategoryDescription 解析当前语言下的分类说明。
function localizedCategoryDescription(category: SubagentCategorySummary, locale: string): string | null {
  return localizedCategoryText(category, locale, "description") ?? category.description ?? null
}

// localizedCategoryText selects exact or language-prefix metadata from category locales.
// localizedCategoryText 从分类 locales 中选择精确或语言前缀匹配的元数据。
function localizedCategoryText(category: SubagentCategorySummary, locale: string, field: "name" | "description"): string | null {
  const locales = category.locales ?? {}
  return localizedMetadataText(locales, locale, field)
}

// localizedMetadataText selects exact or language-prefix metadata from a locale map.
// localizedMetadataText 从 locale 映射中选择精确或语言前缀匹配的元数据。
function localizedMetadataText(
  locales: Record<string, { name?: string | null; description?: string | null }>,
  locale: string,
  field: "name" | "description",
): string | null {
  const exact = locales[locale]?.[field]?.trim()
  if (exact) {
    return exact
  }
  const language = locale.split("-")[0]
  const prefixEntry = Object.entries(locales).find(([localeKey, metadata]) => (
    localeKey === language || localeKey.startsWith(`${language}-`)
  ))
  const prefixValue = prefixEntry?.[1]?.[field]?.trim()
  return prefixValue || null
}

// nonEmptyText returns trimmed text only when it is non-empty.
// nonEmptyText 仅在文本非空时返回裁剪后的文本。
function nonEmptyText(value: string): string | null {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

// nullableTextPatch converts an empty editable field into an explicit clear request.
// nullableTextPatch 将空编辑字段转换为显式清空请求。
function nullableTextPatch(value: string): string | null {
  return nonEmptyText(value)
}

// colorSwatchValue returns a renderable color for an agent color token or CSS color value.
// colorSwatchValue 为智能体颜色标识或 CSS 色值返回可渲染颜色。
function colorSwatchValue(value: string): string {
  const trimmed = value.trim()
  if (trimmed.startsWith("#") || trimmed.startsWith("rgb") || trimmed.startsWith("hsl")) {
    return trimmed
  }
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

// colorInputValue normalizes metadata colors into the value shape required by native color inputs.
// colorInputValue 将元数据颜色规范化为原生颜色输入框要求的取值形式。
function colorInputValue(value: string): string {
  const renderedColor = colorSwatchValue(value).trim()
  const longHex = renderedColor.match(/^#([0-9a-f]{6})$/i)
  if (longHex) {
    return `#${longHex[1].toLowerCase()}`
  }
  const shortHex = renderedColor.match(/^#([0-9a-f])([0-9a-f])([0-9a-f])$/i)
  if (shortHex) {
    return `#${shortHex[1]}${shortHex[1]}${shortHex[2]}${shortHex[2]}${shortHex[3]}${shortHex[3]}`.toLowerCase()
  }
  return "#6b7280"
}

// listText formats one list for textarea editing.
// listText 将列表格式化为适合文本域编辑的文本。
function listText(values: string[]): string {
  return values.join("\n")
}

// listFromText parses comma-or-newline separated text into stable unique entries.
// listFromText 将逗号或换行分隔文本解析为稳定去重条目。
function listFromText(value: string): string[] {
  const next: string[] = []
  value.split(/[\n,]/).forEach((entry) => {
    const trimmed = entry.trim()
    if (trimmed.length > 0 && !next.includes(trimmed)) {
      next.push(trimmed)
    }
  })
  return next
}

// localizedAgentLiteral returns Chinese copy for Chinese UI and English otherwise.
// localizedAgentLiteral 在中文界面返回中文文案，否则返回英文文案。
function localizedAgentLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}
