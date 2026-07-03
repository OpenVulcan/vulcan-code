import * as React from "react"
import { Bot, ChevronDown, ExternalLink, FileText, FolderOpen, Github, Maximize2, Minimize2, Pencil, Plus, Power, RefreshCw, RotateCcw, Save, X } from "lucide-react"
import { toast } from "sonner"

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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { StatusPill } from "@/components/shared/status"
import { useI18n } from "@/i18n"
import { openExternalUrl } from "@/service/project-actions-service"
import type { SubagentCatalogSummary, SubagentCategorySummary, SubagentDefinitionSaveRequest, SubagentDefinitionSummary, SubagentOriginSummary, WorkbenchSnapshot } from "@/types"
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
  // Newline-separated tool ids forced on by the three-state policy.
  // 三态策略中按换行分隔的强制启用工具标识。
  allowedTools: string
  // Newline-separated tool ids forced off by the three-state policy.
  // 三态策略中按换行分隔的强制关闭工具标识。
  disabledTools: string
  // Newline-separated AI capability selectors forced on.
  // 按换行分隔的强制启用 AI 能力选择器。
  allowedAiCapabilities: string
  // Newline-separated AI capability selectors forced off.
  // 按换行分隔的强制关闭 AI 能力选择器。
  disabledAiCapabilities: string
  // JSON payload for LuaSkills-specific exposure overrides.
  // LuaSkills 专属可见性覆盖 JSON 载荷。
  luaskillsOverridesJson: string
  // JSON payload for MCP-specific exposure overrides.
  // MCP 专属可见性覆盖 JSON 载荷。
  mcpServerOverridesJson: string
  // Newline-separated enabled prompt skill names.
  // 按换行分隔的启用提示词技能名称。
  enabledPromptSkills: string
  // Newline-separated disabled prompt skill names.
  // 按换行分隔的禁用提示词技能名称。
  disabledPromptSkills: string
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

// AgentCapabilityPolicyState represents the per-agent override state for one capability item.
// AgentCapabilityPolicyState 表示单个能力项在当前智能体中的覆盖状态。
type AgentCapabilityPolicyState = "default" | "allow" | "deny"

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
  // editorDraft stores the currently edited or newly created definition.
  // editorDraft 保存当前正在编辑或新建的定义。
  const [editorDraft, setEditorDraft] = React.useState<AgentEditorDraft | null>(null)
  // detailDefinition stores the agent selected for the details dialog.
  // detailDefinition 保存当前打开详情弹窗的智能体定义。
  const [detailDefinition, setDetailDefinition] = React.useState<SubagentDefinitionSummary | null>(null)
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
  // editorMcpToolOptions merges discovered MCP tools with draft policy ids.
  // editorMcpToolOptions 合并已发现 MCP 工具与草稿策略标识。
  const editorMcpToolOptions = React.useMemo(() => buildAgentMcpToolPolicyOptions(props.workbench, editorDraft, locale), [editorDraft, locale, props.workbench])
  // categoryRows sorts catalog categories for the filter menu.
  // categoryRows 对目录分类排序以供筛选菜单使用。
  const categoryRows = React.useMemo(() => sortedAgentCategories(props.workbench.subagentCatalog.categories, locale), [locale, props.workbench.subagentCatalog.categories])
  // categoryById resolves localized category metadata for definition cards.
  // categoryById 为定义卡片解析本地化分类元数据。
  const categoryById = React.useMemo(() => new Map(categoryRows.map((category) => [category.categoryId, category])), [categoryRows])
  // visibleDefinitions applies the selected category filter.
  // visibleDefinitions 应用当前选中的分类筛选。
  const visibleDefinitions = React.useMemo(() => {
    if (!selectedCategoryId) {
      return props.workbench.subagentCatalog.definitions
    }
    return props.workbench.subagentCatalog.definitions.filter((definition) => definition.categoryId === selectedCategoryId)
  }, [props.workbench.subagentCatalog.definitions, selectedCategoryId])

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

  // submitEditorDraft persists the current draft through the user catalog.
  // submitEditorDraft 通过用户目录持久化当前草稿。
  const submitEditorDraft = React.useCallback(async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!editorDraft || editorDraft.agentId.trim().length === 0) {
      return
    }
    let request: SubagentDefinitionSaveRequest
    try {
      request = saveRequestFromDraft(editorDraft)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
      return
    }
    await runAction("save", () => props.actions.onSaveAgentDefinition(request))
    setEditorDraft(null)
  }, [editorDraft, props.actions, runAction])

  return (
    <section className="skill-management-surface agent-management-surface">
      <div className="skill-page-body agent-skill-page-body">
        <section className="skill-toolbar agent-catalog-toolbar" aria-label={text("智能体目录操作", "Agent catalog actions")}>
          <div className="agent-catalog-toolbar-main">
            <button className="skill-action-button" disabled={busyAction !== null} onClick={() => setEditorDraft(emptyAgentDraft(defaultAgentMetadataLocale(locale)))} type="button">
              <Plus size={15} />
              <span>{text("新增智能体", "New agent")}</span>
            </button>
            <button className="skill-action-button secondary" disabled={busyAction !== null} onClick={() => runAction("refresh", props.actions.onRefreshAgentCatalog)} type="button">
              <RefreshCw size={15} />
              <span>{busyAction === "refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
            </button>
            <button className="skill-action-button secondary" disabled={busyAction !== null} onClick={() => runAction("open-user-dir", props.actions.onOpenAgentUserDirectory)} type="button">
              <FolderOpen size={15} />
              <span>{text("打开用户目录", "Open user directory")}</span>
            </button>
            <span className="agent-catalog-notice">{props.workbench.subagentCatalog.notice}</span>
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
              <strong>{text("当前分类暂无智能体", "No agents in this category")}</strong>
              <span>{text("切换到全部或选择其他分类。", "Switch to All or choose another category.")}</span>
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
            />
          ))}
        </div>
        <AgentEditorForm
          busy={busyAction !== null}
          draft={editorDraft}
          onCancel={() => setEditorDraft(null)}
          onDraftChange={setEditorDraft}
          mcpToolOptions={editorMcpToolOptions}
          promptSkillOptions={editorPromptSkillOptions}
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
  // Selectable MCP tool rows available to the three-state policy table.
  // 三态策略表可选择的 MCP 工具行。
  mcpToolOptions: AgentCapabilityPolicyOption[]
  // Selectable prompt skill rows available to the three-state policy table.
  // 三态策略表可选择的提示词技能行。
  promptSkillOptions: AgentCapabilityPolicyOption[]
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
  const selectedMetadataLocale = draft?.metadataLocale ?? defaultAgentMetadataLocale(locale)
  const selectedMetadataName = draft?.localizedNames[selectedMetadataLocale] ?? ""
  const selectedMetadataDescription = draft?.localizedDescriptions[selectedMetadataLocale] ?? ""
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
  // updateToolPolicyState applies a mutually exclusive state to one tool id.
  // updateToolPolicyState 为单个工具标识应用互斥的策略状态。
  const updateToolPolicyState = React.useCallback((toolId: string, state: AgentCapabilityPolicyState) => {
    if (!props.draft) {
      return
    }
    props.onDraftChange(setDraftToolPolicyState(props.draft, toolId, state))
  }, [props])
  // updateMcpToolPolicyState applies a mutually exclusive state to one MCP tool selector.
  // updateMcpToolPolicyState 为单个 MCP 工具选择器应用互斥策略状态。
  const updateMcpToolPolicyState = React.useCallback((toolId: string, state: AgentCapabilityPolicyState) => {
    if (!props.draft) {
      return
    }
    props.onDraftChange(setDraftMcpToolPolicyState(props.draft, toolId, state))
  }, [props])
  // updatePromptSkillPolicyState applies a mutually exclusive state to one prompt skill.
  // updatePromptSkillPolicyState 为单个提示词技能应用互斥的策略状态。
  const updatePromptSkillPolicyState = React.useCallback((skillName: string, state: AgentCapabilityPolicyState) => {
    if (!props.draft) {
      return
    }
    props.onDraftChange(setDraftPromptSkillPolicyState(props.draft, skillName, state))
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
              ? text("这是系统内置智能体。保存后不会修改系统文件，而是写入用户目录覆盖。", "This is a system agent. Saving writes a user-layer override instead of changing the system file.")
              : text("保存后写入用户智能体目录。", "Saving writes to the user agent directory.")}
          </DialogDescription>
        </DialogHeader>
        {draft ? (
          <form className="agent-editor-form" onSubmit={props.onSubmit}>
            <div className="agent-editor-grid">
              <label>
                <span>agent_id</span>
                <input disabled={props.busy || draft.existing} pattern="[A-Za-z0-9._/-]+" required value={draft.agentId} onChange={(event) => updateDraftField("agentId", event.target.value)} />
              </label>
              <label>
                <span>{text("用户包", "User package")}</span>
                <input disabled={props.busy || editingSystemBackedDefinition} pattern="[A-Za-z0-9._-]*" value={draft.packageId} onChange={(event) => updateDraftField("packageId", event.target.value)} />
              </label>
              <label>
                <span>{text("元数据语言", "Metadata language")}</span>
                <select disabled={props.busy} value={selectedMetadataLocale} onChange={(event) => updateDraftField("metadataLocale", event.target.value as AgentMetadataLocale)}>
                  {AGENT_METADATA_LOCALES.map((option) => (
                    <option key={option.locale} value={option.locale}>{text(option.zh, option.en)}</option>
                  ))}
                </select>
              </label>
              <label>
                <span>{text("名称", "Name")}</span>
                <input disabled={props.busy} value={selectedMetadataName} onChange={(event) => updateDraftMetadataField("name", event.target.value)} />
              </label>
              <label className="agent-editor-checkbox">
                <input checked={draft.enabled} disabled={props.busy} type="checkbox" onChange={(event) => updateDraftField("enabled", event.target.checked)} />
                <span>{text("暴露给模型", "Expose to model")}</span>
              </label>
              <label className="wide">
                <span>{text("描述", "Description")}</span>
                <input disabled={props.busy} value={selectedMetadataDescription} onChange={(event) => updateDraftMetadataField("description", event.target.value)} />
              </label>
              <label>
                <span>{text("图标", "Emoji")}</span>
                <input disabled={props.busy} value={draft.emoji} onChange={(event) => updateDraftField("emoji", event.target.value)} />
              </label>
              <label>
                <span>{text("颜色", "Color")}</span>
                <span className="agent-editor-color-field">
                  <span className="agent-editor-color-swatch" style={{ backgroundColor: colorSwatchValue(draft.color) }} />
                  <input disabled={props.busy} value={draft.color} onChange={(event) => updateDraftField("color", event.target.value)} />
                </span>
              </label>
              <label className="wide">
                <span>vibe</span>
                <input disabled={props.busy} value={draft.vibe} onChange={(event) => updateDraftField("vibe", event.target.value)} />
              </label>
              <label className="wide agent-prompt-field primary">
                <span>{text("系统提示词", "System prompt")}</span>
                <textarea autoFocus disabled={props.busy} rows={14} value={draft.systemPrompt} onChange={(event) => updateDraftField("systemPrompt", event.target.value)} />
              </label>
              <label className="wide agent-prompt-field">
                <span>{text("追加提示词", "Prompt append")}</span>
                <textarea disabled={props.busy} rows={5} value={draft.systemPromptAppend} onChange={(event) => updateDraftField("systemPromptAppend", event.target.value)} />
              </label>
              <div className="wide">
                <AgentCapabilityPolicyTable
                  allowCountLabel={text("启用", "Enable")}
                  allowedItems={listFromText(draft.allowedTools)}
                  disabled={props.busy}
                  disabledItems={listFromText(draft.disabledTools)}
                  denyCountLabel={text("关闭", "Disable")}
                  emptyLabel={text("没有匹配工具", "No matching tools")}
                  onStateChange={updateToolPolicyState}
                  options={props.toolOptions}
                  searchPlaceholder={text("搜索工具", "Search tools")}
                  title={text("工具策略", "Tool policy")}
                />
              </div>
              <div className="wide">
                <AgentCapabilityPolicyTable
                  allowCountLabel={text("启用", "Enable")}
                  allowedItems={mcpExposurePolicyFromJson(draft.mcpServerOverridesJson).allowedTools}
                  disabled={props.busy}
                  disabledItems={mcpExposurePolicyFromJson(draft.mcpServerOverridesJson).disabledTools}
                  denyCountLabel={text("关闭", "Disable")}
                  emptyLabel={text("没有匹配 MCP 工具", "No matching MCP tools")}
                  onStateChange={updateMcpToolPolicyState}
                  options={props.mcpToolOptions}
                  searchPlaceholder={text("搜索 MCP 工具", "Search MCP tools")}
                  title={text("MCP 工具策略", "MCP tool policy")}
                />
              </div>
              <label className="wide agent-prompt-field">
                <span>{text("AI 能力允许", "Allowed AI capabilities")}</span>
                <textarea disabled={props.busy} rows={3} value={draft.allowedAiCapabilities} onChange={(event) => updateDraftField("allowedAiCapabilities", event.target.value)} />
              </label>
              <label className="wide agent-prompt-field">
                <span>{text("AI 能力关闭", "Disabled AI capabilities")}</span>
                <textarea disabled={props.busy} rows={3} value={draft.disabledAiCapabilities} onChange={(event) => updateDraftField("disabledAiCapabilities", event.target.value)} />
              </label>
              <label className="wide agent-prompt-field">
                <span>{text("LuaSkills 覆盖 JSON", "LuaSkills override JSON")}</span>
                <textarea disabled={props.busy} rows={4} value={draft.luaskillsOverridesJson} onChange={(event) => updateDraftField("luaskillsOverridesJson", event.target.value)} />
              </label>
              <label className="wide agent-prompt-field">
                <span>{text("MCP 覆盖 JSON", "MCP override JSON")}</span>
                <textarea disabled={props.busy} rows={4} value={draft.mcpServerOverridesJson} onChange={(event) => updateDraftField("mcpServerOverridesJson", event.target.value)} />
              </label>
              <div className="wide">
                <AgentCapabilityPolicyTable
                  allowCountLabel={text("启用", "Enable")}
                  allowedItems={listFromText(draft.enabledPromptSkills)}
                  disabled={props.busy}
                  disabledItems={listFromText(draft.disabledPromptSkills)}
                  denyCountLabel={text("关闭", "Disable")}
                  emptyLabel={text("没有匹配 Prompt Skills", "No matching Prompt Skills")}
                  onStateChange={updatePromptSkillPolicyState}
                  options={props.promptSkillOptions}
                  searchPlaceholder={text("搜索 Prompt Skills", "Search Prompt Skills")}
                  title="Prompt Skills"
                />
              </div>
            </div>
            <div className="agent-editor-context">
              <span>{text("当前层级", "Current layer")}: {draft.sourceScope || text("用户", "User")}</span>
              {draft.definitionFile ? <span>{draft.definitionFile}</span> : null}
              {editingSystemBackedDefinition ? <strong>{text("保存目标：用户目录覆盖", "Save target: user override")}</strong> : null}
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

// AgentCapabilityPolicyTable renders one mutually exclusive policy selector per capability item.
// AgentCapabilityPolicyTable 为每个能力项渲染一个互斥策略选择器。
function AgentCapabilityPolicyTable(props: {
  // Label used for the allow count and button.
  // 用于启用计数与按钮的标签。
  allowCountLabel: string
  // Policy ids currently forced on.
  // 当前被强制启用的策略标识。
  allowedItems: string[]
  // Whether the table controls are disabled.
  // 表格控件是否禁用。
  disabled: boolean
  // Policy ids currently forced off.
  // 当前被强制关闭的策略标识。
  disabledItems: string[]
  // Label used for the deny count and button.
  // 用于关闭计数与按钮的标签。
  denyCountLabel: string
  // Empty search result label.
  // 空搜索结果标签。
  emptyLabel: string
  // Applies the next state for one capability id.
  // 应用单个能力标识的下一个状态。
  onStateChange: (itemId: string, state: AgentCapabilityPolicyState) => void
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
  // allowedSet accelerates state lookup for enabled overrides.
  // allowedSet 加速启用覆盖状态查询。
  const allowedSet = React.useMemo(() => new Set(props.allowedItems), [props.allowedItems])
  // disabledSet accelerates state lookup for disabled overrides.
  // disabledSet 加速关闭覆盖状态查询。
  const disabledSet = React.useMemo(() => new Set(props.disabledItems), [props.disabledItems])
  // stateCounts summarizes current mutually exclusive table states.
  // stateCounts 汇总当前互斥表格状态。
  const stateCounts = React.useMemo(() => props.options.reduce(
    (counts, option) => {
      const state = capabilityPolicyState(option.id, allowedSet, disabledSet)
      counts[state] += 1
      return counts
    },
    { default: 0, allow: 0, deny: 0 } satisfies Record<AgentCapabilityPolicyState, number>,
  ), [allowedSet, disabledSet, props.options])
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
          <span>{text("默认", "Default")} {stateCounts.default} · {props.allowCountLabel} {stateCounts.allow} · {props.denyCountLabel} {stateCounts.deny}</span>
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
          const currentState = capabilityPolicyState(option.id, allowedSet, disabledSet)
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
              <div className="skill-tool-segmented agent-tool-policy-state" role="group" aria-label={option.id}>
                <button
                  aria-pressed={currentState === "default"}
                  className={currentState === "default" ? "active" : undefined}
                  disabled={props.disabled}
                  onClick={() => props.onStateChange(option.id, "default")}
                  type="button"
                >
                  {text("默认", "Default")}
                </button>
                <button
                  aria-pressed={currentState === "allow"}
                  className={currentState === "allow" ? "active" : undefined}
                  disabled={props.disabled}
                  onClick={() => props.onStateChange(option.id, "allow")}
                  type="button"
                >
                  {props.allowCountLabel}
                </button>
                <button
                  aria-pressed={currentState === "deny"}
                  className={currentState === "deny" ? "active" : undefined}
                  disabled={props.disabled}
                  onClick={() => props.onStateChange(option.id, "deny")}
                  type="button"
                >
                  {props.denyCountLabel}
                </button>
              </div>
            </div>
          )
        })}
      </div>
    </section>
  )
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
  const label = iconKey === "github" ? "github" : origin.kind?.trim() || "source"
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
}) {
  const { locale, t } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedAgentLiteral(locale, zh, en), [locale])
  const definition = props.definition
  const displayName = localizedDefinitionName(definition, locale)
  const displayDescription = localizedDefinitionDescription(definition, locale)
  const category = definition.categoryId ? props.categoryById.get(definition.categoryId) : undefined
  const categoryLabel = category ? localizedCategoryName(category, locale) : definition.categoryId
  const fallbackIcon = definition.emoji?.trim() || displayName.trim().charAt(0).toUpperCase() || "*"
  return (
    <article className="skill-card agent-skill-card subagent-definition-card">
      <header className="skill-card-header">
        <div className="agent-skill-title">
          <span className="agent-skill-icon fallback">{fallbackIcon}</span>
          <div>
            <strong>{displayName}</strong>
            <div className="skill-card-badges">
              {definition.enabled ? <Badge className="skill-card-enabled-badge">{t("skills.status.enabled")}</Badge> : <Badge variant="destructive">{t("skills.status.stopped")}</Badge>}
              <Badge variant="outline">{definition.scope}</Badge>
              {definition.categoryId ? <Badge variant="outline" title={definition.categoryId}>{categoryLabel}</Badge> : null}
              {definition.packageId ? <Badge variant="outline">{definition.packageId}</Badge> : null}
              {definition.overrideOf ? <Badge variant="outline">{text("覆盖", "Override")}</Badge> : null}
              {definition.remoteUrl ? <Badge variant="outline">Remote</Badge> : null}
              <AgentSourceOriginLink origin={definition.sourceOrigin} />
            </div>
          </div>
        </div>
      </header>
      <p>{displayDescription || text("未填写描述。", "No description provided.")}</p>
      <div className="skill-card-metrics">
        <span>{text("工具", "Tools")} {definition.allowedToolCount}/{definition.disabledToolCount}</span>
        <span>Prompt Skills {definition.enabledPromptSkillCount}/{definition.disabledPromptSkillCount}</span>
      </div>
      <div className="skill-actions-row">
        <button className="skill-action-button secondary" onClick={() => props.onOpenDetail(definition)} type="button">
          <FileText size={15} />
          <span>{t("skills.action.details")}</span>
        </button>
        <button className="skill-action-button secondary" disabled={props.busy} onClick={props.onEdit} type="button">
          <Pencil size={15} />
          <span>{text("编辑", "Edit")}</span>
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="skill-action-button secondary" disabled={props.busy} type="button">
              <span>{t("skills.action.action")}</span>
              <ChevronDown size={14} />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="lua-skill-action-menu">
            <DropdownMenuItem onSelect={() => void props.onRunAction("toggle", () => props.actions.onSetAgentDefinitionEnabled(definition.agentId, !definition.enabled))}>
              <Power size={15} />
              <span>{definition.enabled ? t("skills.action.disable") : t("skills.action.enable")}</span>
            </DropdownMenuItem>
            {definition.canRestoreDefault ? (
              <DropdownMenuItem onSelect={() => void props.onRunAction("restore", () => props.actions.onRestoreAgentDefinitionDefault(definition.agentId))}>
                <RotateCcw size={15} />
                <span>{text("恢复系统默认", "Restore default")}</span>
              </DropdownMenuItem>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>
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
    allowedTools: "",
    disabledTools: "",
    allowedAiCapabilities: "",
    disabledAiCapabilities: "",
    luaskillsOverridesJson: "",
    mcpServerOverridesJson: "",
    enabledPromptSkills: "",
    disabledPromptSkills: "",
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
    allowedTools: listText(definition.allowedTools),
    disabledTools: listText(definition.disabledTools),
    allowedAiCapabilities: listText(definition.allowedAiCapabilities ?? []),
    disabledAiCapabilities: listText(definition.disabledAiCapabilities ?? []),
    luaskillsOverridesJson: jsonTextFromValue(definition.luaskillsOverrides),
    mcpServerOverridesJson: jsonTextFromValue(definition.mcpServerOverrides),
    enabledPromptSkills: listText(definition.enabledPromptSkills),
    disabledPromptSkills: listText(definition.disabledPromptSkills),
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
  const locales = localizedSaveLocalesFromDraft(draft)
  const name = preferredDraftMetadataField(locales, "name") ?? draft.name.trim()
  const description = preferredDraftMetadataField(locales, "description") ?? nonEmptyText(draft.description)
  return {
    agentId: draft.agentId.trim(),
    originalAgentId: nonEmptyText(draft.originalAgentId),
    packageId: nonEmptyText(draft.packageId),
    name,
    description,
    locales,
    emoji: nullableTextPatch(draft.emoji),
    color: nullableTextPatch(draft.color),
    vibe: nullableTextPatch(draft.vibe),
    enabled: draft.enabled,
    systemPrompt: nonEmptyText(draft.systemPrompt),
    systemPromptAppend: nonEmptyText(draft.systemPromptAppend),
    allowedTools: listFromText(draft.allowedTools),
    disabledTools: listFromText(draft.disabledTools),
    allowedAiCapabilities: listFromText(draft.allowedAiCapabilities),
    disabledAiCapabilities: listFromText(draft.disabledAiCapabilities),
    luaskillsOverrides: jsonValueFromText(draft.luaskillsOverridesJson, "LuaSkills override JSON"),
    mcpServerOverrides: jsonValueFromText(draft.mcpServerOverridesJson, "MCP override JSON"),
    enabledPromptSkills: listFromText(draft.enabledPromptSkills),
    disabledPromptSkills: listFromText(draft.disabledPromptSkills),
  }
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
        label: tool.localName || toolId,
        family: "LuaSkills",
        description: tool.description,
        detail: `${skill.displayName || skill.skillId} · ${tool.registrationLabel}`,
      })
    })
  })
  if (draft) {
    const configuredDescription = localizedAgentLiteral(
      locale,
      "已保存到当前智能体，但当前工作台快照未解析到对应工具。",
      "Saved on this agent, but not resolved in the current workbench snapshot.",
    )
    listFromText(`${draft.allowedTools}\n${draft.disabledTools}`).forEach((toolId) => {
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
    listFromText(`${draft.enabledPromptSkills}\n${draft.disabledPromptSkills}`).forEach((skillName) => {
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

// buildAgentMcpToolPolicyOptions builds selectable MCP tool rows from the workbench snapshot and draft policies.
// buildAgentMcpToolPolicyOptions 从工作台快照与草稿策略构建可选择 MCP 工具行。
function buildAgentMcpToolPolicyOptions(workbench: WorkbenchSnapshot, draft: AgentEditorDraft | null, locale: string): AgentCapabilityPolicyOption[] {
  // optionMap keeps one row per MCP tool selector.
  // optionMap 确保每个 MCP 工具选择器只保留一行。
  const optionMap = new Map<string, AgentCapabilityPolicyOption>()
  const serverNameById = new Map((workbench.mcpServers ?? []).map((server) => [server.id, server.name || server.id]))
  workbench.mcpTools?.forEach((tool) => {
    const toolId = tool.id.trim()
    if (toolId.length === 0) {
      return
    }
    const serverLabel = serverNameById.get(tool.serverId) ?? tool.serverId
    appendAgentCapabilityPolicyOption(optionMap, {
      id: toolId,
      label: tool.name || toolId,
      family: "MCP",
      description: tool.description || localizedAgentLiteral(locale, "当前 MCP 工具未提供说明。", "This MCP tool does not provide a description."),
      detail: `${serverLabel} · ${tool.originToolId || toolId}`,
    })
  })
  if (draft) {
    const policy = mcpExposurePolicyFromJson(draft.mcpServerOverridesJson)
    const configuredDescription = localizedAgentLiteral(
      locale,
      "已保存到当前智能体，但当前工作台快照未解析到对应 MCP 工具。",
      "Saved on this agent, but not resolved in the current workbench snapshot.",
    )
    ;[...policy.allowedTools, ...policy.disabledTools].forEach((toolId) => {
      appendAgentCapabilityPolicyOption(optionMap, {
        id: toolId,
        label: toolId,
        family: localizedAgentLiteral(locale, "已配置 MCP", "Configured MCP"),
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

// setDraftToolPolicyState updates both persisted policy lists as one mutually exclusive choice.
// setDraftToolPolicyState 将两个持久化策略列表作为一个互斥选择进行更新。
function setDraftToolPolicyState(draft: AgentEditorDraft, toolId: string, state: AgentCapabilityPolicyState): AgentEditorDraft {
  const allowedTools = listFromText(draft.allowedTools).filter((entry) => entry !== toolId)
  const disabledTools = listFromText(draft.disabledTools).filter((entry) => entry !== toolId)
  if (state === "allow") {
    allowedTools.push(toolId)
  }
  if (state === "deny") {
    disabledTools.push(toolId)
  }
  return {
    ...draft,
    allowedTools: listText(allowedTools),
    disabledTools: listText(disabledTools),
  }
}

// setDraftPromptSkillPolicyState updates both persisted prompt skill policy lists as one mutually exclusive choice.
// setDraftPromptSkillPolicyState 将两个持久化提示词技能策略列表作为一个互斥选择进行更新。
function setDraftPromptSkillPolicyState(draft: AgentEditorDraft, skillName: string, state: AgentCapabilityPolicyState): AgentEditorDraft {
  const enabledPromptSkills = listFromText(draft.enabledPromptSkills).filter((entry) => entry !== skillName)
  const disabledPromptSkills = listFromText(draft.disabledPromptSkills).filter((entry) => entry !== skillName)
  if (state === "allow") {
    enabledPromptSkills.push(skillName)
  }
  if (state === "deny") {
    disabledPromptSkills.push(skillName)
  }
  return {
    ...draft,
    enabledPromptSkills: listText(enabledPromptSkills),
    disabledPromptSkills: listText(disabledPromptSkills),
  }
}

// setDraftMcpToolPolicyState updates the MCP override JSON while preserving unrelated MCP selectors.
// setDraftMcpToolPolicyState 更新 MCP 覆盖 JSON，同时保留无关 MCP 选择器。
function setDraftMcpToolPolicyState(draft: AgentEditorDraft, toolId: string, state: AgentCapabilityPolicyState): AgentEditorDraft {
  const payload = objectFromJsonText(draft.mcpServerOverridesJson)
  const policy = mcpExposurePolicyFromObject(payload)
  const allowedTools = policy.allowedTools.filter((entry) => entry !== toolId)
  const disabledTools = policy.disabledTools.filter((entry) => entry !== toolId)
  if (state === "allow") {
    allowedTools.push(toolId)
  }
  if (state === "deny") {
    disabledTools.push(toolId)
  }
  const nextPayload = {
    ...payload,
    tools: allowedTools,
    disabled: disabledTools,
  }
  removeMcpToolPolicyAliasKeys(nextPayload)
  return {
    ...draft,
    mcpServerOverridesJson: jsonTextFromValue(pruneEmptyMcpPolicyArrays(nextPayload)),
  }
}

// AgentMcpExposurePolicy stores parsed MCP tool and server selectors.
// AgentMcpExposurePolicy 保存解析后的 MCP 工具与服务器选择器。
interface AgentMcpExposurePolicy {
  // MCP tool selectors forced on.
  // 被强制启用的 MCP 工具选择器。
  allowedTools: string[]
  // MCP tool selectors forced off.
  // 被强制关闭的 MCP 工具选择器。
  disabledTools: string[]
  // MCP server ids forced on.
  // 被强制启用的 MCP 服务器标识。
  allowedServers: string[]
  // MCP server ids forced off.
  // 被强制关闭的 MCP 服务器标识。
  disabledServers: string[]
}

// mcpExposurePolicyFromJson parses MCP selectors from an editable JSON payload.
// mcpExposurePolicyFromJson 从可编辑 JSON 载荷解析 MCP 选择器。
function mcpExposurePolicyFromJson(value: string): AgentMcpExposurePolicy {
  return mcpExposurePolicyFromObject(objectFromJsonText(value))
}

// mcpExposurePolicyFromObject extracts supported MCP selector keys from an object payload.
// mcpExposurePolicyFromObject 从对象载荷中提取受支持的 MCP 选择器键。
function mcpExposurePolicyFromObject(value: Record<string, unknown>): AgentMcpExposurePolicy {
  return {
    allowedTools: stringListFromUnknownValues([value.tools, value.mcp_tools, value.allow, value.allowed, value.enabled, value.include]),
    disabledTools: stringListFromUnknownValues([value.disabled, value.deny, value.denied, value.disable, value.exclude]),
    allowedServers: stringListFromUnknown(value.servers ?? value.server ?? value.server_ids ?? value.mcp_servers),
    disabledServers: stringListFromUnknown(value.disabled_servers ?? value.deny_servers ?? value.exclude_servers),
  }
}

// removeMcpToolPolicyAliasKeys removes non-canonical tool selector keys after table edits.
// removeMcpToolPolicyAliasKeys 在表格编辑后移除非规范工具选择器键。
function removeMcpToolPolicyAliasKeys(value: Record<string, unknown>): void {
  delete value.mcp_tools
  delete value.allow
  delete value.allowed
  delete value.enabled
  delete value.include
  delete value.deny
  delete value.denied
  delete value.disable
  delete value.exclude
}

// pruneEmptyMcpPolicyArrays removes empty table-managed arrays from MCP override payloads.
// pruneEmptyMcpPolicyArrays 从 MCP 覆盖载荷中移除表格管理的空数组。
function pruneEmptyMcpPolicyArrays(value: Record<string, unknown>): Record<string, unknown> {
  const next = { ...value }
  if (Array.isArray(next.servers) && next.servers.length === 0) {
    delete next.servers
  }
  if (Array.isArray(next.tools) && next.tools.length === 0) {
    delete next.tools
  }
  if (Array.isArray(next.disabled) && next.disabled.length === 0) {
    delete next.disabled
  }
  if (Array.isArray(next.disabled_servers) && next.disabled_servers.length === 0) {
    delete next.disabled_servers
  }
  return next
}

// capabilityPolicyState resolves the effective editor state for one capability id.
// capabilityPolicyState 解析单个能力标识在编辑器中的有效状态。
function capabilityPolicyState(itemId: string, allowedSet: Set<string>, disabledSet: Set<string>): AgentCapabilityPolicyState {
  if (disabledSet.has(itemId)) {
    return "deny"
  }
  if (allowedSet.has(itemId)) {
    return "allow"
  }
  return "default"
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

// jsonTextFromValue formats JSON for stable textarea editing.
// jsonTextFromValue 将 JSON 格式化为稳定的文本域内容。
function jsonTextFromValue(value: unknown): string {
  if (value === null || value === undefined) {
    return ""
  }
  if (typeof value === "object" && Object.keys(value as Record<string, unknown>).length === 0) {
    return ""
  }
  return JSON.stringify(value, null, 2)
}

// jsonValueFromText parses optional JSON text for service persistence.
// jsonValueFromText 解析可选 JSON 文本以供服务端持久化。
function jsonValueFromText(value: string, label: string): unknown {
  const trimmed = value.trim()
  if (trimmed.length === 0) {
    return null
  }
  try {
    return JSON.parse(trimmed)
  } catch (error) {
    throw new Error(`${label}: ${error instanceof Error ? error.message : String(error)}`)
  }
}

// objectFromJsonText parses JSON object text and falls back to an empty object for local table edits.
// objectFromJsonText 解析 JSON 对象文本，并在本地表格编辑时回退为空对象。
function objectFromJsonText(value: string): Record<string, unknown> {
  try {
    const parsed = jsonValueFromText(value, "MCP override JSON")
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>
    }
  } catch {
    return {}
  }
  return {}
}

// stringListFromUnknown normalizes unknown JSON values into a string list.
// stringListFromUnknown 将未知 JSON 值规范化为字符串列表。
function stringListFromUnknown(value: unknown): string[] {
  if (typeof value === "string") {
    return listFromText(value)
  }
  if (Array.isArray(value)) {
    return value.filter((entry): entry is string => typeof entry === "string").map((entry) => entry.trim()).filter((entry) => entry.length > 0)
  }
  return []
}

// stringListFromUnknownValues merges multiple unknown JSON values into one de-duplicated string list.
// stringListFromUnknownValues 将多个未知 JSON 值合并为去重字符串列表。
function stringListFromUnknownValues(values: unknown[]): string[] {
  const merged = new Set<string>()
  values.forEach((value) => {
    stringListFromUnknown(value).forEach((entry) => merged.add(entry))
  })
  return Array.from(merged)
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
