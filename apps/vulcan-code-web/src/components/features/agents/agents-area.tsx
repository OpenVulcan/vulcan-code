import * as React from "react"
import { Bot, FolderOpen, Pencil, Plus, RefreshCw, RotateCcw, Save, Search, Trash2, X } from "lucide-react"
import { toast } from "@/ui-kit/primitives/toast"

import { ModelPickerControl, modelPickerGroupKey } from "@/components/composer/model-picker-control"
import { SourceOriginLink } from "@/components/features/source-origin-link"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button, IconButton } from "@/ui-kit/primitives/button"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { FilterChip, FilterChipGroup, SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui-kit/primitives/input-group"
import { PageEmptyState, PageToolbar } from "@/components/page"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { Textarea } from "@/ui-kit/primitives/textarea"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog"
import { DirectoryRow, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout"
import { formatMessage, useI18n, type Translate } from "@/i18n"
import type { CapabilityPolicyMode, SubagentCatalogSummary, SubagentCategorySummary, SubagentDefinitionPrompts, SubagentDefinitionSaveRequest, SubagentDefinitionSummary, WorkbenchSnapshot } from "@/types"
import type { AgentActionProps } from "./agent-area-types"
import { colorInputValue, colorSwatchValue } from "@/lib/agent-color"

export type { AgentActionProps } from "./agent-area-types"
export { activeAgentsHeaderSummary } from "./agent-area-header"

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
  // Human-facing display name; user agents have exactly one, in the author's language.
  // 面向用户的显示名称；用户智能体只有一个，使用作者的语言。
  name: string
  // Optional human-facing description in the author's language.
  // 使用作者语言的可选人类可读说明。
  description: string
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
  // Stable source scope key of the definition being edited.
  // 当前编辑定义的稳定来源作用域键。
  sourceScopeKey: SubagentDefinitionSummary["scopeKey"]
  // Optional system definition id overridden by this draft.
  // 当前草稿覆盖的可选系统定义标识。
  overrideOf: string
  // Current definition file path shown for audit context.
  // 用于审计上下文展示的当前定义文件路径。
  definitionFile: string
}

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
  // busyAction stores the catalog action currently in flight.
  // busyAction 保存当前正在执行的目录动作。
  const [busyAction, setBusyAction] = React.useState<string | null>(null)
  // busyActionRef closes the synchronous repeat-trigger window before React commits page busy state.
  // busyActionRef 在 React 提交页面忙碌状态前关闭同步重复触发窗口。
  const busyActionRef = React.useRef(false)
  // togglingAgentIds stores row-level enabled-state updates currently in flight.
  // togglingAgentIds 保存当前正在执行启停更新的行级智能体标识。
  const [togglingAgentIds, setTogglingAgentIds] = React.useState<ReadonlySet<string>>(() => new Set())
  // togglingAgentIdsRef rejects duplicate same-row toggles while preserving concurrency across different rows.
  // togglingAgentIdsRef 拒绝同一行的重复启停，同时保留不同行之间的并发能力。
  const togglingAgentIdsRef = React.useRef<ReadonlySet<string>>(new Set())
  // editorDraft stores the currently edited or newly created definition.
  // editorDraft 保存当前正在编辑或新建的定义。
  const [editorDraft, setEditorDraft] = React.useState<AgentEditorDraft | null>(null)
  // detailDefinition stores the agent selected for the details dialog.
  // detailDefinition 保存当前打开详情弹窗的智能体定义。
  const [detailDefinition, setDetailDefinition] = React.useState<SubagentDefinitionSummary | null>(null)
  // definitionConfirmation binds a destructive definition action to its exact selected row.
  // definitionConfirmation 将破坏性的定义操作绑定到准确的已选行。
  const [definitionConfirmation, setDefinitionConfirmation] = React.useState<{ definition: SubagentDefinitionSummary; operation: "delete" | "restore" } | null>(null)
  // actionError retains failure details without discarding an editor draft.
  // actionError 保留失败详情且不丢弃编辑草稿。
  const [actionError, setActionError] = React.useState<string | null>(null)
  // searchQuery stores the cross-locale keyword filter text.
  // searchQuery 保存跨语言关键词筛选文本。
  const [searchQuery, setSearchQuery] = React.useState("")
  // statusFilter stores the enabled-state filter.
  // statusFilter 保存启停状态筛选。
  const [statusFilter, setStatusFilter] = React.useState<AgentStatusFilter>("all")
  // localCategoryId stores standalone category filtering outside the adaptive shell.
  // localCategoryId 保存自适应外壳之外的独立分类筛选。
  const [localCategoryId, setLocalCategoryId] = React.useState("")
  // catalogActionDisabled blocks catalog-wide actions while any page action or row mutation is active.
  // catalogActionDisabled 在任意页面动作或行级变更执行时禁用目录级动作。
  const catalogActionDisabled = busyAction !== null || togglingAgentIds.size > 0
  // selectedCategoryId resolves controlled or standalone category filtering.
  // selectedCategoryId 解析受控或独立的分类筛选。
  const selectedCategoryId = props.selectedCategoryId ?? localCategoryId
  // onSelectCategory resolves controlled or standalone category updates.
  // onSelectCategory 解析受控或独立的分类更新。
  const onSelectCategory = props.onSelectCategory ?? setLocalCategoryId
  // editorToolOptions merges live tool rows with the draft's persisted policy ids.
  // editorToolOptions 合并实时工具行与草稿中已持久化的策略标识。
  const editorToolOptions = React.useMemo(() => buildAgentToolPolicyOptions(props.workbench, editorDraft, t), [editorDraft, locale, props.workbench])
  // editorPromptSkillOptions merges live Agent Skill rows with the draft's persisted policy ids.
  // editorPromptSkillOptions 合并实时 Agent Skill 行与草稿中已持久化的策略标识。
  const editorPromptSkillOptions = React.useMemo(() => buildAgentPromptSkillPolicyOptions(props.workbench, editorDraft, t), [editorDraft, locale, props.workbench])
  // categoryRows sorts catalog categories for the filter menu.
  // categoryRows 对目录分类排序以供筛选菜单使用。
  const categoryRows = React.useMemo(() => sortedAgentCategories(props.workbench.subagentCatalog.categories, locale, t), [locale, props.workbench.subagentCatalog.categories, t])
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

  // runAction executes one page-exclusive catalog mutation.
  // runAction 执行单个页面互斥的目录变更。
  //
  // Parameters: label identifies the busy indicator and action performs the service mutation.
  // 参数：label 标识忙碌提示，action 执行服务变更。
  // Returns: true only after an accepted action succeeds; rejected or failed actions return false.
  // 返回值：只有已接受动作成功后才返回 true；未接受或失败的动作返回 false。
  const runAction = React.useCallback(async (label: string, action: () => Promise<void>) => {
    if (busyActionRef.current || togglingAgentIdsRef.current.size > 0) {
      return false
    }
    busyActionRef.current = true
    setBusyAction(label)
    setActionError(null)
    try {
      await action()
      return true
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error))
      return false
    } finally {
      busyActionRef.current = false
      setBusyAction(null)
    }
  }, [])

  // runToggleAction executes one enabled-state update without blocking the whole page.
  // runToggleAction 执行单个启停更新且不阻塞整个页面。
  //
  // Parameters: agentId identifies the row guard and action performs the service mutation.
  // 参数：agentId 标识行级守卫，action 执行服务变更。
  // Returns: a promise that settles after the accepted row action releases its guard.
  // 返回值：在已接受行级动作释放守卫后结束的 Promise。
  const runToggleAction = React.useCallback(async (agentId: string, action: () => Promise<void>) => {
    if (busyActionRef.current || togglingAgentIdsRef.current.has(agentId)) {
      return
    }
    // activeAgentIds is the synchronous row-guard snapshot after accepting this toggle.
    // activeAgentIds 是接受本次启停后的同步行级守卫快照。
    const activeAgentIds = new Set(togglingAgentIdsRef.current)
    activeAgentIds.add(agentId)
    togglingAgentIdsRef.current = activeAgentIds
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
      // activeAgentIds is the synchronous row-guard snapshot after releasing this toggle.
      // activeAgentIds 是释放本次启停后的同步行级守卫快照。
      const activeAgentIds = new Set(togglingAgentIdsRef.current)
      activeAgentIds.delete(agentId)
      togglingAgentIdsRef.current = activeAgentIds
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
    if (busyActionRef.current || togglingAgentIdsRef.current.size > 0) {
      return
    }
    const normalizedDraft = editorDraft ? normalizedAgentEditorDraftFileIdentity(editorDraft) : null
    if (!normalizedDraft || normalizedDraft.agentId.trim().length === 0) {
      return
    }
    const conflictsWithExistingDefinition = !normalizedDraft.existing
      && props.workbench.subagentCatalog.definitions.some((definition) => definition.agentId === targetAgentIdFromDraft(normalizedDraft))
    if (conflictsWithExistingDefinition) {
      setEditorDraft(normalizedDraft)
      toast.error(t("agents.skillFileNameAlready"))
      return
    }
    let request: SubagentDefinitionSaveRequest
    try {
      request = saveRequestFromDraft(normalizedDraft)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
      return
    }
    if (await runAction("save", () => props.actions.onSaveAgentDefinition(request))) {
      setEditorDraft(null)
    }
  }, [editorDraft, props.actions, props.workbench.subagentCatalog.definitions, runAction, t])

  return (
    <section className="skill-management-surface agent-management-surface">
      <div className="skill-page-body agent-skill-page-body">
        <PageToolbar
          actions={<>
            <Button disabled={catalogActionDisabled} onClick={() => runAction("refresh", props.actions.onRefreshAgentCatalog)} size="sm" type="button" variant="ghost">
              <RefreshCw className={busyAction === "refresh" ? "vk-spin" : undefined} size={15} />
              {t("skills.action.refresh")}
            </Button>
            <Button disabled={catalogActionDisabled} onClick={() => runAction("open-user-dir", props.actions.onOpenAgentUserDirectory)} size="sm" type="button" variant="ghost">
              <FolderOpen size={15} />
              {t("agents.openUserDirectory")}
            </Button>
            <Button disabled={catalogActionDisabled} onClick={() => { setActionError(null); setEditorDraft(emptyAgentDraft()) }} size="sm" type="button">
              <Plus size={15} />
              {t("agents.newAgent")}
            </Button>
          </>}
          aria-label={t("agents.agentCatalogActions")}
          className="agent-catalog-toolbar"
          role="toolbar"
        >
          <InputGroup className="agent-catalog-search">
            <InputGroupAddon align="inline-start"><Search aria-hidden="true" /></InputGroupAddon>
            <InputGroupInput
              aria-label={t("agents.searchAgents")}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder={t("agents.searchAgents2")}
              type="search"
              value={searchQuery}
            />
          </InputGroup>
          <FilterChipGroup aria-label={t("agents.enabledStateFilter")}>
            <FilterChip onClick={() => setStatusFilter("all")} selected={statusFilter === "all"}>{t("agents.all")}</FilterChip>
            <FilterChip onClick={() => setStatusFilter("enabled")} selected={statusFilter === "enabled"}>{t("agents.enabled")}</FilterChip>
            <FilterChip onClick={() => setStatusFilter("disabled")} selected={statusFilter === "disabled"}>{t("agents.disabled")}</FilterChip>
          </FilterChipGroup>
        </PageToolbar>
        <AgentCatalogIssues catalog={props.workbench.subagentCatalog} />
        {actionError && !editorDraft ? (
          <div role="alert" className="agent-catalog-issues">
            <strong>{t("agents.theActionDidNot")}</strong>
            <details><summary>{t("agents.viewDetails")}</summary><p>{actionError}</p></details>
          </div>
        ) : null}
        <div className="skill-card-list agent-skill-card-list">
          {props.workbench.subagentCatalog.definitions.length === 0 ? (
            <PageEmptyState description={t("agents.createAnAgentManually")} icon={<Bot />} size="panel" title={t("agents.noAgentDefinitions")} />
          ) : visibleDefinitions.length === 0 ? (
            <PageEmptyState description={t("agents.adjustTheCategoryEnabled")} size="panel" title={t("agents.noMatchingAgents")} />
          ) : visibleDefinitions.map((definition) => (
            <AgentDefinitionCard
              actions={props.actions}
              busy={busyAction !== null}
              categoryById={categoryById}
              definition={definition}
              key={definition.sourceKey}
              onEdit={() => { setActionError(null); void runAction("load", async () => setEditorDraft(draftFromDefinition(definition, await props.actions.onLoadAgentDefinitionPrompts(definition.agentId), locale))) }}
              onOpenDetail={setDetailDefinition}
              onRequestDelete={() => { setActionError(null); setDefinitionConfirmation({ definition, operation: "delete" }) }}
              onRequestRestore={() => { setActionError(null); setDefinitionConfirmation({ definition, operation: "restore" }) }}
              onRunToggleAction={runToggleAction}
              toggleBusy={togglingAgentIds.has(definition.agentId)}
            />
          ))}
        </div>
        <AgentEditorForm
          error={actionError}
          busy={catalogActionDisabled}
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
          loadPrompts={props.actions.onLoadAgentDefinitionPrompts}
          open={detailDefinition !== null}
          onOpenChange={(open) => {
            if (!open) {
              setDetailDefinition(null)
            }
          }}
        />
        <AlertDialog open={definitionConfirmation !== null} onOpenChange={(open) => { if (!open && !busyActionRef.current) setDefinitionConfirmation(null) }}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{definitionConfirmation?.operation === "restore" ? t("agents.restoreAgentDefaults") : t("agents.deleteAgent")}</AlertDialogTitle>
              <AlertDialogDescription>{definitionConfirmation ? definitionConfirmation.operation === "restore"
                ? formatMessage(t("agents.removeTheUserOverride"), { value: localizedDefinitionName(definitionConfirmation.definition, locale) })
                : formatMessage(t("agents.deleteTheUserDefinition"), { value: localizedDefinitionName(definitionConfirmation.definition, locale) }) : ""}</AlertDialogDescription>
            </AlertDialogHeader>
            {actionError ? <p role="alert" className="vk-field-error vk-field-scroll">{actionError}</p> : null}
            <AlertDialogFooter>
              <AlertDialogCancel disabled={catalogActionDisabled}>{t("agents.cancel")}</AlertDialogCancel>
              <AlertDialogAction disabled={catalogActionDisabled} onClick={async (event) => {
                event.preventDefault()
                if (!definitionConfirmation) return
                if (await runAction(definitionConfirmation.operation, () => definitionConfirmation.operation === "restore"
                  ? props.actions.onRestoreAgentDefinitionDefault(definitionConfirmation.definition.agentId)
                  : props.actions.onDeleteAgentDefinition(definitionConfirmation.definition.agentId))) setDefinitionConfirmation(null)
              }} variant="destructive">{definitionConfirmation?.operation === "restore" ? t("agents.restoreDefaults") : t("agents.delete")}</AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </section>
  )
}

// AgentCategoryMenu renders the fixed agent category selector.
// AgentCategoryMenu 渲染固定智能体分类选择器。
export function AgentCategoryMenu(props: {
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
  // categories stores sorted category rows for the menu.
  // categories 保存菜单使用的已排序分类行。
  const categories = React.useMemo(() => sortedAgentCategories(props.workbench.subagentCatalog.categories, locale, t), [locale, props.workbench.subagentCatalog.categories, t])
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

  return (
    <WorkspaceDirectoryMenu ariaLabel={t("agents.agentCategories")} heading={t("agents.categories")}>
      <DirectoryRow
        active={props.selectedCategoryId === ""}
        count={props.workbench.subagentCatalog.definitions.length}
        icon={<Bot />}
        label={t("agents.all")}
        onSelect={() => props.onSelectCategory("")}
      />
      {categories.map((category) => (
        <DirectoryRow
          active={props.selectedCategoryId === category.categoryId}
          count={countByCategory.get(category.categoryId) ?? 0}
          key={category.categoryId}
          label={localizedCategoryName(category, locale, t)}
          onSelect={() => props.onSelectCategory(category.categoryId)}
          swatch={{ glyph: category.emoji, color: colorSwatchValue(category.color) }}
          title={localizedCategoryDescription(category, locale) ?? category.categoryId}
        />
      ))}
    </WorkspaceDirectoryMenu>
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
  // Latest accepted action failure, retained with the editable draft until retry or dismissal.
  // 最近已接受动作的失败信息，与可编辑草稿一起保留，直到重试或关闭。
  error: string | null
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
  const { t } = useI18n()
  // Keep initial focus at the title so opening a long editor never skips its first fields.
  // 初始焦点放在标题上，避免打开长编辑表单时跳过开头字段。
  const editorTitleRef = React.useRef<HTMLHeadingElement>(null)
  const draft = props.draft
  const editingSystemBackedDefinition = draft?.sourceScopeKey === "system" || Boolean(draft?.overrideOf)
  const readOnlySystemMetadata = Boolean(editingSystemBackedDefinition)
  const fixedModelValue = draft?.fixedModelProviderId && draft.fixedModelId
    ? modelPickerGroupKey({ providerId: draft.fixedModelProviderId, modelId: draft.fixedModelId })
    : ""
  const fixedModelMissingFromChoices = Boolean(fixedModelValue && !modelPickerChoiceExists(props.providers, fixedModelValue))
  const fixedModelUnavailable = Boolean(fixedModelValue && (draft?.fixedModelAvailable === false || fixedModelMissingFromChoices))
  const fixedModelUnavailableReason = draft?.fixedModelUnavailableReason || t("agents.thisFixedModelIs")
  // updateDraftField applies one field update while preserving the rest of the draft.
  // updateDraftField 在保留其余草稿字段的同时应用单字段更新。
  const updateDraftField = React.useCallback(<Key extends keyof AgentEditorDraft>(key: Key, value: AgentEditorDraft[Key]) => {
    if (!props.draft) {
      return
    }
    props.onDraftChange({ ...props.draft, [key]: value })
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
      <DialogContent className="agent-editor-dialog" size="xl" scrollMode="fixed-footer" onOpenAutoFocus={(event) => {
        event.preventDefault()
        editorTitleRef.current?.focus({ preventScroll: true })
      }}>
        <DialogHeader>
          <DialogTitle ref={editorTitleRef} tabIndex={-1}>{draft?.existing ? t("agents.editAgent") : t("agents.newAgent")}</DialogTitle>
          <DialogDescription>
            {editingSystemBackedDefinition
              ? t("agents.systemAgentsOnlyAllow")
              : t("agents.savingWritesToThe")}
          </DialogDescription>
        </DialogHeader>
        {draft ? (
          <form className="agent-editor-form" onSubmit={props.onSubmit}>
            <div className="agent-editor-body">
            <details className="agent-editor-definition" key={draft.agentId} open={readOnlySystemMetadata ? undefined : true}>
              <summary>{t("agents.definitionAndSystemPrompt")}{readOnlySystemMetadata ? <small>{t("agents.builtInReadOnly")}</small> : null}</summary>
            <div className="agent-editor-grid">
              <label>
                <span>{t("agents.skillFileName")}</span>
                <Input disabled={props.busy || draft.existing} pattern="[A-Za-z0-9._/-]+" required value={draft.agentId} onChange={(event) => updateDraftField("agentId", normalizeAgentFileNameInput(event.target.value))} />
              </label>
              <label>
                <span>{t("agents.userPackage")}</span>
                <Input disabled={props.busy || editingSystemBackedDefinition} pattern="[A-Za-z0-9._-]*" value={draft.packageId} onChange={(event) => updateDraftField("packageId", event.target.value)} />
              </label>
              <label>
                <span>{t("agents.name")}</span>
                <Input disabled={props.busy || readOnlySystemMetadata} value={draft.name} onChange={(event) => updateDraftField("name", event.target.value)} />
              </label>
              <label className="agent-editor-checkbox">
                <Switch checked={draft.enabled} disabled={props.busy || readOnlySystemMetadata} onCheckedChange={(checked) => updateDraftField("enabled", checked)} />
                <span>{t("agents.exposeToModel")}</span>
              </label>
              <label className="wide">
                <span>{t("agents.description")}</span>
                <Input disabled={props.busy || readOnlySystemMetadata} value={draft.description} onChange={(event) => updateDraftField("description", event.target.value)} />
              </label>
              <label>
                <span>{t("agents.emoji")}</span>
                <Input disabled={props.busy || readOnlySystemMetadata} value={draft.emoji} onChange={(event) => updateDraftField("emoji", event.target.value)} />
              </label>
              <div className="agent-editor-field">
                <span>{t("agents.colorMarker")}</span>
                <AgentColorPicker disabled={props.busy || readOnlySystemMetadata} value={draft.color} onChange={(color) => updateDraftField("color", color)} />
              </div>
              <label className="wide">
                <span>vibe</span>
                <Input disabled={props.busy || readOnlySystemMetadata} value={draft.vibe} onChange={(event) => updateDraftField("vibe", event.target.value)} />
              </label>
              <label className="wide agent-prompt-field primary">
                <span>{t("agents.systemPrompt")}</span>
                <Textarea density="large" disabled={props.busy} readOnly={readOnlySystemMetadata} rows={10} value={draft.systemPrompt} onChange={(event) => updateDraftField("systemPrompt", event.target.value)} />
              </label>
            </div>
            </details>
            <div className="agent-editor-grid">
              <label className="wide agent-prompt-field">
                <span>{t("agents.promptAppend")}</span>
                <Textarea disabled={props.busy} rows={5} value={draft.systemPromptAppend} onChange={(event) => updateDraftField("systemPromptAppend", event.target.value)} />
              </label>
              <div className="wide agent-editor-model-field">
                <span>{t("agents.model")}</span>
                <div className="vk-form-inline">
                  <ModelPickerControl
                    selectionScope="model"
                    ariaLabel={t("agents.chooseFixedAgentModel")}
                    fallbackLabel={t("agents.followSessionModel")}
                    filterPlaceholder={t("agents.filterByModelName")}
                    noMatchesLabel={t("agents.noModelsMatchThe")}
                    providers={props.providers}
                    value={fixedModelValue}
                    unavailableValueLabel={fixedModelValue ? `${t("agents.unavailable")}: ${draft.fixedModelProviderId}/${draft.fixedModelId}` : undefined}
                    disabled={props.busy}
                    onValueChange={(choice) => props.onDraftChange({
                      ...draft,
                      fixedModelProviderId: choice.providerId,
                      fixedModelId: choice.modelId,
                      fixedModelAvailable: true,
                      fixedModelUnavailableReason: "",
                    })}
                  />
                  <Button
                    disabled={props.busy || !fixedModelValue}
                    onClick={() => props.onDraftChange({ ...draft, fixedModelProviderId: "", fixedModelId: "", fixedModelAvailable: null, fixedModelUnavailableReason: "" })}
                    type="button"
                    variant="secondary"
                  >
                    <RotateCcw size={14} />
                    <span>{t("agents.followSession")}</span>
                  </Button>
                </div>
                <small className="vk-form-note">
                  {fixedModelValue ? `${t("agents.fixedModel")}: ${draft.fixedModelProviderId}/${draft.fixedModelId}` : t("agents.currentlyFollowsTheSession")}
                </small>
                {fixedModelUnavailable ? (
                  <small className="vk-form-note">{fixedModelUnavailableReason}</small>
                ) : null}
              </div>
              <div className="wide">
                <AgentCapabilityPolicyTable
                  disabled={props.busy}
                  emptyLabel={t("agents.noMatchingTools")}
                  mode={draft.toolPolicyMode}
                  onModeChange={(mode) => props.onDraftChange({
                    ...draft,
                    toolPolicyMode: mode,
                    toolPolicyTools: mode === "all" ? "" : draft.toolPolicyTools,
                  })}
                  onSelectedItemsChange={updateToolPolicyItems}
                  options={props.toolOptions}
                  searchPlaceholder={t("agents.searchTools")}
                  selectedItems={listFromText(draft.toolPolicyTools)}
                  title={t("agents.toolPolicy")}
                />
              </div>
              <div className="wide">
                <AgentCapabilityPolicyTable
                  disabled={props.busy}
                  emptyLabel={t("agents.noMatchingPromptSkills")}
                  mode={draft.promptSkillPolicyMode}
                  onModeChange={(mode) => props.onDraftChange({
                    ...draft,
                    promptSkillPolicyMode: mode,
                    promptSkillPolicySkills: mode === "all" ? "" : draft.promptSkillPolicySkills,
                  })}
                  onSelectedItemsChange={updatePromptSkillPolicyItems}
                  options={props.promptSkillOptions}
                  searchPlaceholder={t("agents.searchPromptSkills")}
                  selectedItems={listFromText(draft.promptSkillPolicySkills)}
                  title={t("agents.promptSkillPolicy")}
                />
              </div>
            </div>
            <div className="agent-editor-context">
              <span>{t("agents.currentLayer")}: {localizedAgentScopeLabel(draft.sourceScopeKey, t)}</span>
              {draft.definitionFile ? <span>{draft.definitionFile}</span> : null}
              {editingSystemBackedDefinition ? <strong>{t("agents.onlyUserMetadataOverride")}</strong> : null}
            </div>
            </div>
            <DialogFooter>
              {props.error ? (
                <div role="alert" className="vk-field-error vk-form-fill">
                  <strong>{t("agents.saveFailedYourDraft")}</strong>
                  <details><summary>{t("agents.viewDetails")}</summary><p className="vk-diag">{props.error}</p></details>
                </div>
              ) : null}
              <Button disabled={props.busy} onClick={props.onCancel} type="button" variant="secondary">
                <X size={15} />
                <span>{t("agents.cancel")}</span>
              </Button>
              <Button disabled={props.busy || draft.agentId.trim().length === 0 || draft.name.trim().length === 0} type="submit">
                <Save size={15} />
                <span>{t("agents.save")}</span>
              </Button>
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
  const { t } = useI18n()
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
        <span>{displayValue || t("agents.chooseColor")}</span>
        <input
          aria-label={t("agents.chooseColor")}
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
  const { t } = useI18n()
  // filterText stores the current local search query.
  // filterText 保存当前本地搜索条件。
  const [filterText, setFilterText] = React.useState("")
  // selectableItemIds stores selectors that are still valid choices in this policy table.
  // selectableItemIds 保存当前策略表中仍然可选的选择器。
  const selectableItemIds = React.useMemo(() => new Set(props.options.map((option) => option.id)), [props.options])
  // selectedPolicyItems removes stale or forbidden selectors that are no longer selectable.
  // selectedPolicyItems 移除已经不可选的过期或禁用选择器。
  const selectedPolicyItems = React.useMemo(
    () => props.selectedItems.filter((item) => selectableItemIds.has(item)),
    [props.selectedItems, selectableItemIds],
  )
  // selectedSet accelerates selected selector lookup.
  // selectedSet 加速已选择选择器查询。
  const selectedSet = React.useMemo(() => new Set(selectedPolicyItems), [selectedPolicyItems])
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
          <span>{agentPolicyModeSummary(props.mode, selectedPolicyItems.length, t)}</span>
        </div>
        <SegmentedControl
          aria-label={props.title}
          className="agent-tool-policy-mode"
          disabled={props.disabled}
          onValueChange={(value) => {
            if (value) {
              props.onModeChange(value as CapabilityPolicyMode)
            }
          }}
          value={props.mode}
        >
          <SegmentedControlItem value="all">
            {t("agents.all")}
          </SegmentedControlItem>
          <SegmentedControlItem value="allow_only">
            {t("agents.allowOnly")}
          </SegmentedControlItem>
          <SegmentedControlItem value="deny_only">
            {t("agents.denyOnly")}
          </SegmentedControlItem>
        </SegmentedControl>
        <input
          aria-label={props.searchPlaceholder}
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
                {option.description ? <details className="agent-policy-description"><summary>{t("agents.description2")}</summary><p>{option.description}</p></details> : null}
                {option.detail ? <span>{option.detail}</span> : null}
              </div>
              <label className="agent-tool-policy-checkbox">
                <Checkbox
                  aria-label={`${policySelectionLabel(props.mode, t)} ${option.label}`}
                  checked={checked}
                  disabled={props.disabled || props.mode === "all"}
                  onCheckedChange={(nextChecked) => {
                    props.onSelectedItemsChange(setPolicyItemSelection(selectedPolicyItems, option.id, nextChecked === true))
                  }}
                />
                <span>{policySelectionLabel(props.mode, t)}</span>
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
function agentPolicyModeSummary(mode: CapabilityPolicyMode, count: number, t: Translate): string {
  if (mode === "allow_only") {
    return formatMessage(t("agents.allowOnly2"), { count })
  }
  if (mode === "deny_only") {
    return formatMessage(t("agents.denyOnly2"), { count })
  }
  return t("agents.all")
}

// policySelectionLabel returns the checkbox label for one policy mode.
// policySelectionLabel 返回单个策略模式下的复选框标签。
function policySelectionLabel(mode: CapabilityPolicyMode, t: Translate): string {
  if (mode === "allow_only") {
    return t("agents.allow")
  }
  if (mode === "deny_only") {
    return t("agents.deny")
  }
  return t("agents.all")
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
  // Requests confirmation before deleting the current definition.
  // 删除当前定义前请求确认。
  onRequestDelete: () => void
  // Requests confirmation before removing a system definition's user override.
  // 移除系统定义的用户覆盖前请求确认。
  onRequestRestore: () => void
  // Runs one enabled-state action with row-level busy state.
  // 使用行级忙碌状态运行单个启停动作。
  onRunToggleAction: (agentId: string, action: () => Promise<void>) => Promise<void>
  // Whether this row is currently persisting its enabled state.
  // 当前行是否正在持久化启停状态。
  toggleBusy: boolean
}) {
  const { locale, t } = useI18n()
  const definition = props.definition
  const displayName = localizedDefinitionName(definition, locale)
  const displayDescription = localizedDefinitionDescription(definition, locale)
  const category = definition.categoryId ? props.categoryById.get(definition.categoryId) : undefined
  const categoryLabel = category ? localizedCategoryName(category, locale, t) : definition.categoryId
  const fallbackIcon = definition.emoji?.trim() || displayName.trim().charAt(0).toUpperCase() || "*"
  const toolAccessLabel = agentDefinitionToolAccessLabel(definition, t)
  const fixedModelSelector = agentDefinitionFixedModelSelector(definition)
  const fixedModelUnavailable = agentDefinitionFixedModelUnavailable(definition)
  const fixedModelTitle = agentDefinitionFixedModelTitle(definition, t)
  const scopeLabel = localizedAgentScopeLabel(definition.scopeKey, t)
  return (
    <article className="agent-definition-row">
      <button aria-label={`${t("skills.action.details")} · ${displayName}`} className="agent-definition-row-icon" onClick={() => props.onOpenDetail(definition)} title={t("skills.action.details")} type="button">
        <span className="agent-definition-row-glyph">{fallbackIcon}</span>
        <span className="agent-definition-row-color-marker" style={{ backgroundColor: colorSwatchValue(definition.color) }} />
      </button>
      <div className="agent-definition-row-main">
        <div className="agent-definition-row-title-line">
          <button className="agent-definition-title-button" onClick={() => props.onOpenDetail(definition)} type="button">{displayName}</button>
          <div className="agent-definition-row-badges">
            <Badge variant="outline">{scopeLabel}</Badge>
            <Badge variant="outline">{toolAccessLabel}</Badge>
            {definition.categoryId ? <Badge variant="outline" title={definition.categoryId}>{categoryLabel}</Badge> : null}
            {definition.packageId ? <Badge variant="outline">{definition.packageId}</Badge> : null}
            {definition.owningPluginId ? <Badge variant="outline">{definition.owningPluginId}</Badge> : null}
            {definition.remoteUrl ? <Badge variant="outline">{t("agents.remote")}</Badge> : null}
            {fixedModelSelector ? (
              <Badge variant={fixedModelUnavailable ? "destructive" : "outline"} title={fixedModelTitle}>
                {fixedModelUnavailable ? t("agents.fixedModelUnavailable") : t("agents.fixedModel")}
              </Badge>
            ) : null}
            <SourceOriginLink origin={definition.sourceOrigin} />
          </div>
        </div>
        <p>{displayDescription || t("agents.noDescriptionProvided")}</p>
        {definition.owningPluginId && definition.promptSkillPolicySkills.length > 0 ? (
          <div className="agent-definition-row-badges" aria-label={t("agents.attachedSkills")}>
            {definition.promptSkillPolicySkills.map((skillId) => (
              <Badge key={skillId} variant="secondary">{skillId}</Badge>
            ))}
          </div>
        ) : null}
        <span className="agent-definition-row-path">{definition.definitionFile}</span>
      </div>
      <div className="agent-definition-row-actions">
        {definition.canToggle ? <Switch
          checked={definition.enabled}
          aria-label={definition.enabled ? t("skills.action.disable") : t("skills.action.enable")}
          disabled={props.busy || props.toggleBusy}
          onCheckedChange={() => void props.onRunToggleAction(definition.agentId, () => props.actions.onSetAgentDefinitionEnabled(definition.agentId, !definition.enabled))}
          title={definition.enabled ? t("skills.action.disable") : t("skills.action.enable")}
        /> : null}
        {definition.canEdit ? (
          <IconButton aria-label={t("agents.edit")} disabled={props.busy} onClick={props.onEdit} title={t("agents.edit")} type="button">
            <Pencil size={15} />
          </IconButton>
        ) : null}
        {definition.canDelete ? (
          <IconButton
            aria-label={t("agents.delete")}
            disabled={props.busy}
            onClick={props.onRequestDelete}
            title={t("agents.delete")}
            type="button"
            variant="destructive"
          >
            <Trash2 size={15} />
          </IconButton>
        ) : null}
        {definition.canRestoreDefault ? (
          <IconButton aria-label={t("agents.restoreDefaults")} disabled={props.busy || props.toggleBusy} onClick={props.onRequestRestore} title={t("agents.restoreDefaults")} type="button">
            <RotateCcw size={15} />
          </IconButton>
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
  // Reads the prompt text shown in the dialog.
  // 读取弹窗展示的提示词正文。
  loadPrompts: AgentActionProps["onLoadAgentDefinitionPrompts"]
}) {
  const { locale, t } = useI18n()
  const definition = props.definition
  const loaded = useAgentDefinitionPrompts(props.open ? definition?.agentId ?? null : null, props.loadPrompts)
  const displayName = definition ? localizedDefinitionName(definition, locale) : null
  const modelDetail = definition ? agentDefinitionProviderModelDetail(definition, t) : null
  const scopeLabel = definition ? localizedAgentScopeLabel(definition.scopeKey, t) : null
  return (
    <Dialog open={props.open && definition !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="agent-skill-detail-dialog" size="lg" scrollMode="content-scroll">
        <DialogHeader>
          <DialogTitle>{displayName ?? t("agents.agentDetails")}</DialogTitle>
          <DialogDescription>{scopeLabel ?? t("agents.inspectTheAgentDefinition")}</DialogDescription>
        </DialogHeader>
        {definition ? (
          <div className="agent-skill-detail-body">
            <div className="lua-skill-detail-grid">
              <div>
                <span>{t("skills.label.currentStatus")}</span>
                <strong>{definition.enabled ? t("skills.status.enabled") : t("skills.status.stopped")}</strong>
              </div>
              <div>
                <span>{t("agents.scope")}</span>
                <strong>{scopeLabel}</strong>
              </div>
              {definition.owningPluginId ? (
                <div>
                  <span>{t("agents.owningPlugin")}</span>
                  <strong>{definition.owningPluginId}</strong>
                </div>
              ) : null}
              <div>
                <span>{t("agents.toolPolicy")}</span>
                <strong>{formatMessage(t("agents.allowDeny"), { allowedToolCount: definition.allowedToolCount, disabledToolCount: definition.disabledToolCount })}</strong>
              </div>
              <div>
                <span>{t("agents.promptSkillPolicy")}</span>
                <strong>{formatMessage(t("agents.allowDeny2"), { enabledPromptSkillCount: definition.enabledPromptSkillCount, disabledPromptSkillCount: definition.disabledPromptSkillCount })}</strong>
              </div>
              <div className="wide">
                <span>{t("agents.model")}</span>
                <strong title={modelDetail?.title}>{modelDetail?.label}</strong>
              </div>
              <div className="wide">
                <span>{t("agents.definitionFile")}</span>
                <strong>{definition.definitionFile}</strong>
              </div>
              {definition.remoteUrl ? (
                <div className="wide">
                  <span>{t("agents.remoteUrl")}</span>
                  <strong>{definition.remoteUrl}</strong>
                </div>
              ) : null}
              {definition.sourceOrigin?.url ? (
                <div className="wide">
                  <span>{t("agents.displaySource")}</span>
                  <strong>
                    <SourceOriginLink detail origin={definition.sourceOrigin} />
                  </strong>
                </div>
              ) : null}
              {definition.overrideOf ? (
                <div className="wide">
                  <span>{t("agents.overrideTarget")}</span>
                  <strong>{definition.overrideOf}</strong>
                </div>
              ) : null}
            </div>
            <div className="skill-tool-row">
              <strong>{t("agents.capabilityDiagnostics")}</strong>
              <p>{agentCapabilityDiagnostics(definition, t).join(" ")}</p>
            </div>
            {definition.promptSkillPolicySkills.length > 0 ? (
              <div className="skill-tool-row">
                <strong>{t("agents.attachedSkills")}</strong>
                <div className="agent-definition-row-badges">
                  {definition.promptSkillPolicySkills.map((skillId) => (
                    <Badge key={skillId} variant="secondary">{skillId}</Badge>
                  ))}
                </div>
              </div>
            ) : null}
            {loaded?.error ? <p role="alert" className="vk-field-error">{loaded.error}</p> : null}
            {loaded?.prompts?.systemPrompt ? (
              <div className="skill-tool-row">
                <strong>{t("agents.systemPrompt")}</strong>
                <pre className="skill-body-preview">{loaded.prompts.systemPrompt}</pre>
              </div>
            ) : null}
            {loaded?.prompts?.systemPromptAppend ? (
              <div className="skill-tool-row">
                <strong>{t("agents.promptAppend")}</strong>
                <pre className="skill-body-preview">{loaded.prompts.systemPromptAppend}</pre>
              </div>
            ) : null}
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

// useAgentDefinitionPrompts loads one agent's prompt text while its detail dialog is open.
// useAgentDefinitionPrompts 在详情弹窗打开期间加载单个智能体的提示词正文。
// Parameters: agentId is null while closed; load reads the prompts. Returns: the result for agentId, or null while loading.
// 参数：关闭时 agentId 为 null；load 读取提示词。返回值：agentId 对应的结果，加载中为 null。
function useAgentDefinitionPrompts(agentId: string | null, load: AgentActionProps["onLoadAgentDefinitionPrompts"]) {
  const [result, setResult] = React.useState<{ agentId: string; prompts?: SubagentDefinitionPrompts; error?: string } | null>(null)
  React.useEffect(() => {
    if (agentId === null) return
    let cancelled = false
    load(agentId).then(
      (prompts) => { if (!cancelled) setResult({ agentId, prompts }) },
      (error: unknown) => { if (!cancelled) setResult({ agentId, error: error instanceof Error ? error.message : String(error) }) },
    )
    return () => { cancelled = true }
  }, [agentId, load])
  return result?.agentId === agentId ? result : null
}

// emptyAgentDraft builds the default draft for a new user agent.
// emptyAgentDraft 构建新用户智能体的默认草稿。
function emptyAgentDraft(): AgentEditorDraft {
  return {
    agentId: "",
    originalAgentId: "",
    packageId: "",
    name: "",
    description: "",
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
    sourceScopeKey: "user",
    overrideOf: "",
    definitionFile: "",
  }
}

// draftFromDefinition converts one catalog row into editable form state.
// draftFromDefinition 将单个目录行转换为可编辑表单状态。
// Parameters: prompts carries the on-demand prompt text; locale selects the shown name of localized system agents, whose metadata stays read-only.
// 参数：prompts 携带按需读取的提示词正文；locale 选择本地化系统智能体的显示名称，其元数据保持只读。
function draftFromDefinition(definition: SubagentDefinitionSummary, prompts: SubagentDefinitionPrompts, locale: string): AgentEditorDraft {
  return {
    agentId: definition.agentId,
    originalAgentId: definition.agentId,
    packageId: definition.packageId ?? "",
    name: localizedDefinitionName(definition, locale),
    description: localizedDefinitionDescription(definition, locale) ?? "",
    emoji: definition.emoji || "🤖",
    color: definition.color || "gray",
    vibe: definition.vibe ?? "",
    enabled: definition.enabled,
    systemPrompt: prompts.systemPrompt ?? "",
    systemPromptAppend: prompts.systemPromptAppend ?? "",
    fixedModelProviderId: definition.providerModel?.providerId ?? "",
    fixedModelId: definition.providerModel?.modelId ?? "",
    fixedModelAvailable: definition.providerModel?.available ?? null,
    fixedModelUnavailableReason: definition.providerModel?.unavailableReason ?? "",
    toolPolicyMode: normalizedCapabilityPolicyMode(definition.toolPolicyMode, definition.allowedTools, definition.disabledTools),
    toolPolicyTools: listText(definition.toolPolicyTools?.length ? definition.toolPolicyTools : legacyPolicyTools(definition.allowedTools, definition.disabledTools)),
    promptSkillPolicyMode: normalizedCapabilityPolicyMode(definition.promptSkillPolicyMode, definition.enabledPromptSkills, definition.disabledPromptSkills),
    promptSkillPolicySkills: listText(definition.promptSkillPolicySkills?.length ? definition.promptSkillPolicySkills : legacyPolicyTools(definition.enabledPromptSkills, definition.disabledPromptSkills)),
    existing: true,
    sourceScopeKey: definition.scopeKey,
    overrideOf: definition.overrideOf ?? "",
    definitionFile: definition.definitionFile,
  }
}

// saveRequestFromDraft converts form state into the service request shape.
// saveRequestFromDraft 将表单状态转换为服务请求结构。
function saveRequestFromDraft(draft: AgentEditorDraft): SubagentDefinitionSaveRequest {
  const normalizedDraft = normalizedAgentEditorDraftFileIdentity(draft)
  return {
    agentId: normalizedDraft.agentId,
    originalAgentId: nonEmptyText(normalizedDraft.originalAgentId),
    packageId: nonEmptyText(normalizedDraft.packageId),
    name: draft.name.trim(),
    description: nonEmptyText(draft.description),
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
  return providers.some((provider) => (provider.models ?? []).some((model) => modelPickerGroupKey({
    providerId: provider.sourceProviderId ?? provider.id,
    modelId: model.id,
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

// buildAgentToolPolicyOptions builds selectable tool rows from the live workbench snapshot and draft policies.
// buildAgentToolPolicyOptions 从实时工作台快照与草稿策略构建可选择工具行。
function buildAgentToolPolicyOptions(workbench: WorkbenchSnapshot, draft: AgentEditorDraft | null, t: Translate): AgentCapabilityPolicyOption[] {
  // optionMap keeps one row per persisted tool id.
  // optionMap 确保每个持久化工具标识只保留一行。
  const optionMap = new Map<string, AgentCapabilityPolicyOption>()
  const forbiddenToolSelectors = agentForbiddenToolSelectorSet(workbench.subagentCatalog.childAgentForbiddenToolIds ?? [])
  workbench.hostTools?.nativeTools?.forEach((tool) => {
    const toolId = tool.id.trim()
    if (toolId.length === 0 || isAgentToolForbidden(toolId, forbiddenToolSelectors)) {
      return
    }
    appendAgentCapabilityPolicyOption(optionMap, {
      id: toolId,
      label: tool.name || toolId,
      family: t("agents.native"),
      description: tool.description,
      detail: tool.aiVisible
        ? t("agents.aiVisibleByDefault")
        : t("agents.runtimeRegisteredButNot"),
    })
  })
  workbench.plugins
    .filter((plugin) => plugin.managementKind === "vulcanPlugin" && plugin.enabled)
    .forEach((plugin) => {
      plugin.tools.forEach((tool) => {
        const toolId = tool.id.trim()
        if (toolId.length === 0 || isAgentToolForbidden(toolId, forbiddenToolSelectors)) {
          return
        }
        appendAgentCapabilityPolicyOption(optionMap, {
          id: toolId,
          label: toolId,
          family: t("agents.vulcanPlugin"),
          description: tool.description,
          detail: [plugin.name || plugin.id, tool.group].filter(Boolean).join(" · "),
        })
      })
    })
  workbench.luaSkills.skills.forEach((skill) => {
    skill.tools.forEach((tool) => {
      const toolId = tool.canonicalName.trim()
      if (toolId.length === 0 || isAgentToolForbidden(toolId, forbiddenToolSelectors)) {
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
    if (toolId.length === 0 || isAgentToolForbidden(toolId, forbiddenToolSelectors)) {
      return
    }
    const serverLabel = serverNameById.get(tool.serverId) ?? tool.serverId
    appendAgentCapabilityPolicyOption(optionMap, {
      id: toolId,
      label: tool.id || tool.name || toolId,
      family: "MCP",
      description: tool.description || t("agents.thisMcpToolDoes"),
      detail: `${serverLabel} · ${tool.originToolId || tool.name || toolId}`,
    })
  })
  if (draft) {
    const configuredDescription = t("agents.savedOnThisAgent")
    listFromText(draft.toolPolicyTools).forEach((toolId) => {
      if (isAgentToolForbidden(toolId, forbiddenToolSelectors)) {
        return
      }
      appendAgentCapabilityPolicyOption(optionMap, {
        id: toolId,
        label: toolId,
        family: t("agents.configured"),
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
function buildAgentPromptSkillPolicyOptions(workbench: WorkbenchSnapshot, draft: AgentEditorDraft | null, t: Translate): AgentCapabilityPolicyOption[] {
  // optionMap keeps one row per persisted prompt skill name.
  // optionMap 确保每个持久化提示词技能名称只保留一行。
  const optionMap = new Map<string, AgentCapabilityPolicyOption>()
  workbench.agentSkills.records.filter((skill) => skill.effective).forEach((skill) => {
    const skillName = skill.name.trim()
    if (skillName.length === 0) {
      return
    }
    appendAgentCapabilityPolicyOption(optionMap, {
      id: skillName,
      label: skill.displayName || skillName,
      family: skill.scopeKey || "Agent Skills",
      description: skill.description,
      detail: skill.enabled
        ? t("agents.currentlyEnabled")
        : t("agents.currentlyDisabledAPolicy"),
    })
  })
  if (draft) {
    const configuredDescription = t("agents.savedOnThisAgent2")
    listFromText(draft.promptSkillPolicySkills).forEach((skillName) => {
      appendAgentCapabilityPolicyOption(optionMap, {
        id: skillName,
        label: skillName,
        family: t("agents.configured"),
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

// agentForbiddenToolSelectorSet builds a normalized lookup set from backend-projected forbidden tool ids.
// agentForbiddenToolSelectorSet 根据后端投影的禁用工具标识构建规范化查找集合。
function agentForbiddenToolSelectorSet(toolIds: string[]): Set<string> {
  return new Set(toolIds.map(normalizedAgentToolSelectorKey).filter(Boolean))
}

// isAgentToolForbidden returns whether one selectable tool id is forbidden for agents.
// isAgentToolForbidden 返回某个可选工具标识是否被智能体禁用。
function isAgentToolForbidden(toolId: string, forbiddenSelectors: Set<string>): boolean {
  const normalized = normalizedAgentToolSelectorKey(toolId)
  return normalized.length > 0 && forbiddenSelectors.has(normalized)
}

// normalizedAgentToolSelectorKey mirrors the backend selector normalization used for tool policies.
// normalizedAgentToolSelectorKey 对齐后端工具策略使用的选择器规范化规则。
function normalizedAgentToolSelectorKey(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/_+/g, "_")
}

// sortedAgentCategories returns stable category rows for the filter menu.
// sortedAgentCategories 返回用于筛选菜单的稳定分类行。
function sortedAgentCategories(categories: SubagentCategorySummary[], locale: string, t: Translate): SubagentCategorySummary[] {
  return [...categories].sort((left, right) => {
    const leftOrder = left.order ?? Number.MAX_SAFE_INTEGER
    const rightOrder = right.order ?? Number.MAX_SAFE_INTEGER
    if (leftOrder !== rightOrder) {
      return leftOrder - rightOrder
    }
    return localizedCategoryName(left, locale, t).localeCompare(localizedCategoryName(right, locale, t))
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
function agentDefinitionToolAccessLabel(definition: SubagentDefinitionSummary, t: Translate): string {
  return localizedAgentToolPolicyLabel(definition.toolPolicyMode, definition.toolPolicyTools.length, t)
}

// agentCapabilityDiagnostics explains how the declared policy relates to the final runtime capabilities.
// agentCapabilityDiagnostics 说明声明的策略与运行时最终能力之间的关系。
function agentCapabilityDiagnostics(definition: SubagentDefinitionSummary, t: Translate): string[] {
  const diagnostics: string[] = []
  if (!definition.enabled) {
    diagnostics.push(t("agents.diagnostics.disabled"))
  }
  const unrestricted = definition.allowedTools.length === 0
    && definition.disabledTools.length === 0
    && definition.luaskillsOverrides == null
    && definition.mcpServerOverrides == null
    && definition.enabledPromptSkills.length === 0
    && definition.disabledPromptSkills.length === 0
  diagnostics.push(t(unrestricted ? "agents.diagnostics.unrestricted" : "agents.diagnostics.policyDeclared"))
  if (definition.allowedTools.length > 0) {
    diagnostics.push(t("agents.diagnostics.allowedTools").replace("{count}", String(definition.allowedTools.length)))
  }
  if (definition.enabledPromptSkills.length > 0) {
    diagnostics.push(t("agents.diagnostics.enabledPromptSkills").replace("{count}", String(definition.enabledPromptSkills.length)))
  }
  return diagnostics
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
function agentDefinitionFixedModelTitle(definition: SubagentDefinitionSummary, t: Translate): string {
  const selector = agentDefinitionFixedModelSelector(definition)
  if (!selector) {
    return t("agents.followSessionModel")
  }
  const unavailableReason = definition.providerModel?.unavailableReason?.trim()
  if (definition.providerModel?.available === false && unavailableReason) {
    return `${selector} - ${unavailableReason}`
  }
  return selector
}

// agentDefinitionProviderModelDetail returns detail-dialog copy for one definition's model binding.
// agentDefinitionProviderModelDetail 返回详情弹窗中单个定义的模型绑定文案。
function agentDefinitionProviderModelDetail(definition: SubagentDefinitionSummary, t: Translate): { label: string; title: string } {
  const selector = agentDefinitionFixedModelSelector(definition)
  if (!selector) {
    const label = t("agents.followSessionModel")
    return { label, title: label }
  }
  const unavailableReason = definition.providerModel?.unavailableReason?.trim()
  if (definition.providerModel?.available === false) {
    const label = formatMessage(t("agents.fixedModelUnavailable2"), { selector })
    return { label, title: unavailableReason ? `${selector} - ${unavailableReason}` : selector }
  }
  const label = formatMessage(t("agents.fixedModel2"), { selector })
  return { label, title: selector }
}

// localizedAgentToolPolicyLabel returns the compact row badge for tool policy.
// localizedAgentToolPolicyLabel 返回工具策略的紧凑行徽标。
function localizedAgentToolPolicyLabel(mode: CapabilityPolicyMode, count: number, t: Translate): string {
  if (mode === "allow_only") {
    return formatMessage(t("agents.allowTools"), { count })
  }
  if (mode === "deny_only") {
    return formatMessage(t("agents.denyTools"), { count })
  }
  return t("agents.allTools")
}

// localizedDefinitionText selects exact or language-prefix metadata from definition locales.
// localizedDefinitionText 从定义 locales 中选择精确或语言前缀匹配的元数据。
function localizedDefinitionText(definition: SubagentDefinitionSummary, locale: string, field: "name" | "description"): string | null {
  const locales = definition.locales ?? {}
  return localizedMetadataText(locales, locale, field)
}

// PLUGIN_AGENT_CATEGORY_ID is the host-owned category holding every plugin-contributed agent. No category file
// describes it, so the interface catalog names it in every language.
// PLUGIN_AGENT_CATEGORY_ID 是容纳全部插件贡献智能体的宿主分类。它没有分类文件描述，因此由界面目录为各语言命名。
const PLUGIN_AGENT_CATEGORY_ID = "plugins"

// localizedCategoryName resolves the category name for the active locale.
// localizedCategoryName 解析当前语言下的分类名称。
function localizedCategoryName(category: SubagentCategorySummary, locale: string, t: Translate): string {
  if (category.categoryId === PLUGIN_AGENT_CATEGORY_ID) return t("agentSkills.builtinCategory.plugins")
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

// localizedAgentScopeLabel converts the stable scope key into the active locale label.
// localizedAgentScopeLabel 将稳定的作用域键转换为当前语言的显示标签。
function localizedAgentScopeLabel(
  scopeKey: SubagentDefinitionSummary["scopeKey"],
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (scopeKey) {
    case "system":
      return t("skills.label.system")
    case "user":
      return t("skills.label.user")
    case "package":
      return t("skills.label.package")
    case "plugin":
      return t("skills.label.plugin")
  }
}
// Directory-only rules follow this lazy owner; shared workbench and responsive rules remain global.
// 目录专有规则随此懒加载归属载入；共用工作台与响应式规则保持全局。
import "@/ui-kit/styles/features/skill-directory.css";
import "@/ui-kit/styles/features/settings-forms.css";
