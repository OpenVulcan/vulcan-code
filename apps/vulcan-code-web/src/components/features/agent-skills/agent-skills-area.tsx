import { DirectoryRow, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout";
import * as React from "react"
import { Copy, FolderOpen, Pencil, Plus, RefreshCw, RotateCcw, Search, Tags, Trash2 } from "lucide-react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"

import type { AgentSkillActionProps } from "@/components/features/agent-skills/agent-skills-types"
import { AgentSkillCategoryManagementDialog, AgentSkillEditorDialog } from "@/components/features/agent-skills/agent-skill-management-dialogs"
import { hasLinkedGitHubSourceOrigin, SourceOriginLink } from "@/components/features/source-origin-link"
import { PageEmptyState, PageToolbar } from "@/components/page"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui-kit/primitives/input-group"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button, IconButton } from "@/ui-kit/primitives/button"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog"
import { FilterChip, FilterChipGroup } from "@/ui-kit/primitives/choice-controls"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { formatMessage, useI18n, type Translate } from "@/i18n"
import type {
  AgentSkillCategorySummary,
  AgentSkillRecordSummary,
  WorkbenchSnapshot,
} from "@/types"
import { AGENT_SKILL_DEFAULT_COLOR } from "@/lib/agent-color";

// AgentSkillStatusFilter identifies one effective-state filter on the Agent Skills page.
// AgentSkillStatusFilter 标识 Agent Skills 页面上的一个有效状态筛选。
type AgentSkillStatusFilter = "all" | "enabled" | "disabled" | "issues"

// AgentSkillSourceFilter identifies one source-scope filter on the Agent Skills page.
// AgentSkillSourceFilter 标识 Agent Skills 页面上的一个来源作用域筛选。
type AgentSkillSourceFilter = "all" | "system" | "user" | "plugin" | "overrides"

// AgentSkillsArea renders the independent source-aware Agent Skills catalog.
// AgentSkillsArea 渲染独立且来源感知的 Agent Skills 目录。
export function AgentSkillsArea(props: {
  // Service-backed Agent Skills actions.
  // 服务端支持的 Agent Skills 动作。
  actions: AgentSkillActionProps
  // Currently selected logical category id, or empty for all categories.
  // 当前选中的逻辑分类标识；空字符串表示全部分类。
  selectedCategoryId?: string
  // Updates the selected logical category id.
  // 更新当前选中的逻辑分类标识。
  onSelectCategory?: (categoryId: string) => void
  // Workbench snapshot containing the complete Agent Skills domain.
  // 包含完整 Agent Skills 领域的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { locale, t } = useI18n()
  // busyAction stores one catalog-wide mutation currently in flight.
  // busyAction 保存当前正在执行的一项目录级变更。
  const [busyAction, setBusyAction] = React.useState<string | null>(null)
  // actionRunningRef closes the synchronous duplicate-action window.
  // actionRunningRef 关闭同步重复动作窗口。
  const actionRunningRef = React.useRef(false)
  // pendingMutation retains the exact destructive intent until the themed confirmation accepts it.
  // pendingMutation 保留精确破坏性意图，直到主题化确认框接受它。
  const [pendingMutation, setPendingMutation] = React.useState<{ kind: "delete" | "restore"; record: AgentSkillRecordSummary } | null>(null)
  // busyRows stores standard names with row-level state mutations in flight.
  // busyRows 保存正在执行行级状态变更的标准名称。
  const [busyRows, setBusyRows] = React.useState<ReadonlySet<string>>(() => new Set())
  // busyRowsRef enforces row identity before React renders the corresponding disabled controls.
  // busyRowsRef 在 React 渲染对应禁用控件前约束行身份。
  const busyRowsRef = React.useRef(new Set<string>())
  const pageBusy = busyAction !== null || busyRows.size > 0
  // actionError stores the latest page-level service mutation failure.
  // actionError 保存最近一次页面级服务变更失败。
  const [actionError, setActionError] = React.useState("")
  // searchQuery stores the cross-locale catalog search text.
  // searchQuery 保存跨语言目录搜索文本。
  const [searchQuery, setSearchQuery] = React.useState("")
  // statusFilter stores the effective enabled-state filter.
  // statusFilter 保存有效启停状态筛选。
  const [statusFilter, setStatusFilter] = React.useState<AgentSkillStatusFilter>("all")
  // sourceFilter stores the source-scope filter.
  // sourceFilter 保存来源作用域筛选。
  const [sourceFilter, setSourceFilter] = React.useState<AgentSkillSourceFilter>("all")
  // localCategoryId stores standalone filtering outside the adaptive workspace shell.
  // localCategoryId 保存自适应工作区外壳之外的独立筛选状态。
  const [localCategoryId, setLocalCategoryId] = React.useState("")
  // selectedCategoryId resolves controlled or standalone category filtering.
  // selectedCategoryId 解析受控或独立的分类筛选。
  const selectedCategoryId = props.selectedCategoryId ?? localCategoryId
  // onSelectCategory resolves controlled or standalone category updates.
  // onSelectCategory 解析受控或独立的分类更新。
  const onSelectCategory = props.onSelectCategory ?? setLocalCategoryId
  // detailRecord stores the source record opened in the detail dialog.
  // detailRecord 保存详情弹窗中打开的来源记录。
  const [detailRecord, setDetailRecord] = React.useState<AgentSkillRecordSummary | null>(null)
  // editorOpen stores whether the complete package editor is visible.
  // editorOpen 保存完整技能包编辑器是否可见。
  const [editorOpen, setEditorOpen] = React.useState(false)
  // editorRecord stores the effective source used to edit or create a user override.
  // editorRecord 保存用于编辑或创建用户覆盖的有效来源。
  const [editorRecord, setEditorRecord] = React.useState<AgentSkillRecordSummary | null>(null)
  // editorCopy stores whether the selected source should seed a new independent package.
  // editorCopy 保存选中来源是否应生成新的独立技能包。
  const [editorCopy, setEditorCopy] = React.useState(false)
  // categoryDialogOpen stores whether the user-category editor is visible.
  // categoryDialogOpen 保存用户分类编辑器是否可见。
  const [categoryDialogOpen, setCategoryDialogOpen] = React.useState(false)
  // effectiveRecords keeps one runtime-effective source record per standard name.
  // effectiveRecords 为每个标准名称保留一条运行时有效来源记录。
  const effectiveRecords = React.useMemo(
    () => props.workbench.agentSkills.records.filter((record) => record.effective),
    [props.workbench.agentSkills.records],
  )
  // categories keeps the backend-defined stable logical order.
  // categories 保留后端定义的稳定逻辑顺序。
  const categories = props.workbench.agentSkills.categories
  // categoryById resolves localized category metadata for record rows.
  // categoryById 为记录行解析本地化分类元数据。
  const categoryById = React.useMemo(
    () => new Map(categories.map((category) => [category.categoryId, category])),
    [categories],
  )
  // sourceRecords selects effective records by default and exposes all records for explicit source inspection.
  // sourceRecords 默认选择有效记录，并在明确来源检查时暴露全部记录。
  const sourceRecords = React.useMemo(() => {
    if (sourceFilter === "all") {
      return effectiveRecords
    }
    if (sourceFilter === "system") {
      return props.workbench.agentSkills.records.filter((record) => record.scopeKey === "system")
    }
    if (sourceFilter === "user") {
      return props.workbench.agentSkills.records.filter((record) => record.scopeKey === "user")
    }
    if (sourceFilter === "plugin") {
      return props.workbench.agentSkills.records.filter((record) => record.scopeKey === "plugin")
    }
    return props.workbench.agentSkills.records.filter((record) => record.scopeKey === "user" && record.overrideOf)
  }, [effectiveRecords, props.workbench.agentSkills.records, sourceFilter])
  // visibleRecords applies logical category, state, source, and cross-locale search filters.
  // visibleRecords 应用逻辑分类、状态、来源与跨语言搜索筛选。
  const visibleRecords = React.useMemo(() => sourceRecords.filter((record) => {
    if (selectedCategoryId && record.categoryId !== selectedCategoryId) {
      return false
    }
    if (statusFilter === "enabled" && (!record.effective || !record.enabled)) {
      return false
    }
    if (statusFilter === "disabled" && (!record.effective || record.enabled)) {
      return false
    }
    if (statusFilter === "issues") {
      return props.workbench.agentSkills.errors.some((issue) => issue.path.startsWith(record.baseDir))
    }
    return agentSkillMatchesSearch(record, searchQuery)
  }), [props.workbench.agentSkills.errors, searchQuery, selectedCategoryId, sourceRecords, statusFilter])

  React.useEffect(() => {
    if (selectedCategoryId && !categories.some((category) => category.categoryId === selectedCategoryId)) {
      onSelectCategory("")
    }
  }, [categories, onSelectCategory, selectedCategoryId])

  // runCatalogAction executes one page-exclusive Agent Skills mutation.
  // runCatalogAction 执行一项页面互斥的 Agent Skills 变更。
  const runCatalogAction = React.useCallback(async (actionId: string, action: () => Promise<void>) => {
    if (actionRunningRef.current || busyRowsRef.current.size > 0) {
      return
    }
    actionRunningRef.current = true
    setBusyAction(actionId)
    setActionError("")
    try {
      await action()
    } catch (caught) {
      setActionError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      actionRunningRef.current = false
      setBusyAction(null)
    }
  }, [busyAction])

  // runRowAction executes one independent row state mutation.
  // runRowAction 执行一项独立的行级状态变更。
  const runRowAction = React.useCallback(async (record: AgentSkillRecordSummary) => {
    if (actionRunningRef.current || busyRowsRef.current.has(record.name)) {
      return
    }
    busyRowsRef.current.add(record.name)
    setBusyRows((current) => new Set(current).add(record.name))
    setActionError("")
    try {
      await props.actions.onSetAgentSkillEnabled(record.name, !record.enabled)
    } catch (caught) {
      setActionError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      busyRowsRef.current.delete(record.name)
      setBusyRows((current) => {
        const next = new Set(current)
        next.delete(record.name)
        return next
      })
    }
  }, [busyRows, props.actions])

  // openEditor opens the package editor with an exact source baseline or a blank draft.
  // openEditor 使用精确来源基线或空白草稿打开技能包编辑器。
  const openEditor = React.useCallback((record: AgentSkillRecordSummary | null, copy = false) => {
    setEditorRecord(record)
    setEditorCopy(copy)
    setEditorOpen(true)
  }, [])

  // deleteUserSkill confirms and deletes one exact user source record.
  // deleteUserSkill 确认并删除一个精确用户来源记录。
  const deleteUserSkill = React.useCallback(async (record: AgentSkillRecordSummary) => {
    setPendingMutation({ kind: "delete", record })
  }, [])

  // restoreSystemSkill confirms and removes one exact user override.
  // restoreSystemSkill 确认并移除一个精确用户覆盖。
  const restoreSystemSkill = React.useCallback(async (record: AgentSkillRecordSummary) => {
    setPendingMutation({ kind: "restore", record })
  }, [])

  return (
    <section className="skill-management-surface agent-management-surface agent-skills-management-surface">
      <div className="skill-page-body agent-skill-page-body">
        <PageToolbar
          actions={<>
            <Button disabled={pageBusy} onClick={() => void runCatalogAction("refresh", props.actions.onRefreshAgentSkills)} size="sm" variant="ghost">
              <RefreshCw size={15} />
              {t("agentSkills.refreshCatalog")}
            </Button>
            <Button disabled={pageBusy} onClick={() => void runCatalogAction("open-directory", props.actions.onOpenAgentSkillsUserDirectory)} size="sm" variant="ghost">
              <FolderOpen size={15} />
              {t("agentSkills.skillDirectory")}
            </Button>
            <Button disabled={pageBusy} onClick={() => setCategoryDialogOpen(true)} size="sm" variant="ghost">
              <Tags size={15} />
              {t("agentSkills.manageCategories")}
            </Button>
            <Button disabled={pageBusy} onClick={() => openEditor(null)} size="sm">
              <Plus size={15} />
              {t("agentSkills.createSkill")}
            </Button>
          </>}
          className="agent-catalog-toolbar"
        >
            <InputGroup className="agent-catalog-search">
              <InputGroupAddon align="inline-start"><Search aria-hidden="true" /></InputGroupAddon>
              <InputGroupInput
                aria-label={t("agentSkills.searchSkills")}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder={t("agentSkills.searchNamesDescriptionsTags")}
                value={searchQuery}
              />
            </InputGroup>
            <FilterChipGroup aria-label={t("agentSkills.filterSkillsByState")}>
              <FilterChip onClick={() => setStatusFilter("all")} selected={statusFilter === "all"}>{t("agentSkills.all")}</FilterChip>
              <FilterChip onClick={() => setStatusFilter("enabled")} selected={statusFilter === "enabled"}>{t("agentSkills.enabled")}</FilterChip>
              <FilterChip onClick={() => setStatusFilter("disabled")} selected={statusFilter === "disabled"}>{t("agentSkills.disabled")}</FilterChip>
              <FilterChip onClick={() => setStatusFilter("issues")} selected={statusFilter === "issues"}>{t("agentSkills.issues")}</FilterChip>
            </FilterChipGroup>
            <Select onValueChange={(value) => setSourceFilter(value as AgentSkillSourceFilter)} value={sourceFilter}>
              <SelectTrigger aria-label={t("agentSkills.filterSkillsBySource")} size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("agentSkills.allSources")}</SelectItem>
                <SelectItem value="system">{t("agentSkills.system")}</SelectItem>
                <SelectItem value="user">{t("agentSkills.user")}</SelectItem>
                <SelectItem value="plugin">{t("agentSkills.plugins")}</SelectItem>
                <SelectItem value="overrides">{t("agentSkills.overrides")}</SelectItem>
              </SelectContent>
            </Select>
        </PageToolbar>

        {props.workbench.agentSkills.errors.length > 0 ? (
          <div className="agent-catalog-issues">
            {props.workbench.agentSkills.errors.map((issue) => (
              <div key={`${issue.path}:${issue.message}`}><strong>{issue.path}</strong><span>{issue.message}</span></div>
            ))}
          </div>
        ) : null}
        {actionError ? <p className="agent-skill-form-error" role="alert">{actionError}</p> : null}

        <div className="skill-card-list agent-skill-card-list">
          {visibleRecords.length === 0 ? (
            <PageEmptyState size="panel" title={t("agentSkills.noSkillsMatchThe")} />
          ) : visibleRecords.map((record) => (
            <article className="agent-definition-row" key={record.skillKey}>
              <button className="agent-definition-row-icon" onClick={() => setDetailRecord(record)} title={t("agentSkills.details")} type="button">
                <span className="agent-definition-row-glyph">
                  {record.emoji?.trim() || localizedAgentSkillName(record, locale).slice(0, 1).toUpperCase()}
                </span>
                <span className="agent-definition-row-color-marker" style={{ backgroundColor: record.color ?? categoryById.get(record.categoryId)?.color ?? AGENT_SKILL_DEFAULT_COLOR }} />
              </button>
              <div className="agent-definition-row-main">
                <div className="agent-definition-row-title-line">
                  <button className="agent-definition-title-button" onClick={() => setDetailRecord(record)} type="button">{localizedAgentSkillName(record, locale)}</button>
                  <div className="agent-definition-row-badges">
                    <Badge variant="outline">{localizedAgentSkillScope(record, t)}</Badge>
                    <Badge variant="outline">{localizedAgentSkillCategory(categoryById.get(record.categoryId), locale, t)}</Badge>
                    {hasLinkedGitHubSourceOrigin(record.origin)
                      ? <SourceOriginLink origin={record.origin} />
                      : <Badge variant="outline">{localizedAgentSkillOrigin(record, locale, t)}</Badge>}
                    {record.overrideOf ? <Badge variant="outline">{t("agentSkills.override")}</Badge> : null}
                    {record.owningPluginId ? <Badge variant="outline">{record.owningPluginId}</Badge> : null}
                    {record.owningPluginId ? (
                      <Badge variant={record.enabled ? "secondary" : "outline"}>
                        {record.enabled
                          ? t("agentSkills.enabled")
                          : t("agentSkills.disabled")}
                      </Badge>
                    ) : null}
                    {!record.effective ? <Badge variant="outline">{t("agentSkills.inactiveSource")}</Badge> : null}
                  </div>
                </div>
                <p>{localizedAgentSkillDescription(record, locale) || (t("agentSkills.noDescriptionProvided"))}</p>
                <span className="agent-definition-row-path">{record.skillFile}</span>
              </div>
              <div className="agent-definition-row-actions">
                {record.canToggle ? <Switch
                  checked={record.enabled}
                  aria-label={record.enabled ? t("agentSkills.disable") : t("agentSkills.enable")}
                  disabled={!record.effective || busyRows.has(record.name) || pageBusy}
                  onCheckedChange={() => void runRowAction(record)}
                  title={record.enabled ? t("agentSkills.disable") : t("agentSkills.enable")}
                /> : null}
                {record.effective && record.canEdit ? (
                  <IconButton aria-label={record.scopeKey === "system" ? t("agentSkills.createOverride") : t("agentSkills.edit")} disabled={pageBusy} onClick={() => openEditor(record)} title={record.scopeKey === "system" ? t("agentSkills.createOverride") : t("agentSkills.edit")} type="button">
                    <Pencil size={14} />
                  </IconButton>
                ) : null}
                {record.canCopy ? <IconButton aria-label={t("agentSkills.copy")} disabled={pageBusy} onClick={() => openEditor(record, true)} title={t("agentSkills.copy")} type="button">
                  <Copy size={14} />
                </IconButton> : null}
                {record.canRestoreDefault ? (
                  <IconButton aria-label={t("agentSkills.restoreSystemVersion")} disabled={pageBusy} onClick={() => void runCatalogAction(`restore:${record.name}`, () => restoreSystemSkill(record))} title={t("agentSkills.restoreSystemVersion")} type="button">
                    <RotateCcw size={14} />
                  </IconButton>
                ) : null}
                {record.canDelete ? (
                  <IconButton aria-label={t("agentSkills.delete")} disabled={pageBusy} onClick={() => void runCatalogAction(`delete:${record.name}`, () => deleteUserSkill(record))} title={t("agentSkills.delete")} type="button" variant="destructive">
                    <Trash2 size={14} />
                  </IconButton>
                ) : null}
              </div>
            </article>
          ))}
        </div>
      </div>
      <AgentSkillDetailDialog categories={categories} locale={locale} onOpenChange={(open) => !open && setDetailRecord(null)} record={detailRecord} />
      <AgentSkillEditorDialog
        categories={categories}
        copy={editorCopy}
        locale={locale}
        onOpenChange={setEditorOpen}
        onSave={props.actions.onSaveAgentSkill}
        open={editorOpen}
        record={editorRecord}
      />
      <AgentSkillCategoryManagementDialog
        categories={categories}
        locale={locale}
        onDelete={props.actions.onDeleteAgentSkillCategory}
        onOpenChange={setCategoryDialogOpen}
        onSave={props.actions.onSaveAgentSkillCategory}
        open={categoryDialogOpen}
      />
      <AlertDialog open={pendingMutation !== null} onOpenChange={(open) => { if (!open && !actionRunningRef.current) setPendingMutation(null) }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{pendingMutation?.kind === "restore" ? t("agentSkills.restoreSystemVersion") : t("agentSkills.deleteSkill")}</AlertDialogTitle>
            <AlertDialogDescription>{pendingMutation ? (pendingMutation.kind === "restore"
              ? (formatMessage(t("agentSkills.removeTheUserOverride"), { value: localizedAgentSkillName(pendingMutation.record, locale) }))
              : (formatMessage(t("agentSkills.deleteUserSkillAnd"), { value: localizedAgentSkillName(pendingMutation.record, locale) }))) : ""}</AlertDialogDescription>
          </AlertDialogHeader>
          {actionError ? <p role="alert" className="vk-field-error">{actionError}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pageBusy}>{t("agentSkills.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={pageBusy} onClick={(event) => {
              event.preventDefault()
              if (!pendingMutation) return
              void runCatalogAction(pendingMutation.kind, async () => {
                if (pendingMutation.kind === "restore") await props.actions.onRestoreAgentSkillDefault(pendingMutation.record.name)
                else await props.actions.onDeleteAgentSkill(pendingMutation.record.name)
                setDetailRecord(null)
                setPendingMutation(null)
              })
            }}>{pageBusy ? t("agentSkills.working") : t("agentSkills.confirm")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  )
}

// AgentSkillCategoryMenu renders the fixed prompt-skill category directory.
// AgentSkillCategoryMenu 渲染固定提示词技能分类目录。
export function AgentSkillCategoryMenu(props: {
  // Currently selected logical category id, or empty for all categories.
  // 当前选中的逻辑分类标识；空字符串表示全部分类。
  selectedCategoryId: string
  // Applies the selected logical category id.
  // 应用选中的逻辑分类标识。
  onSelectCategory: (categoryId: string) => void
  // Workbench snapshot containing the complete Agent Skills domain.
  // 包含完整 Agent Skills 领域的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { locale, t } = useI18n()
  // effectiveRecords keeps one runtime-effective source record per standard name.
  // effectiveRecords 为每个标准名称保留一条运行时有效来源记录。
  const effectiveRecords = React.useMemo(
    () => props.workbench.agentSkills.records.filter((record) => record.effective),
    [props.workbench.agentSkills.records],
  )
  // countByCategory stores effective skill counts keyed by category id.
  // countByCategory 按分类标识保存有效技能数量。
  const countByCategory = React.useMemo(() => {
    const counts = new Map<string, number>()
    effectiveRecords.forEach((record) => counts.set(record.categoryId, (counts.get(record.categoryId) ?? 0) + 1))
    return counts
  }, [effectiveRecords])

  return (
    <WorkspaceDirectoryMenu ariaLabel={t("agentSkills.skillCategories")} heading={t("agentSkills.categories")}>
      <DirectoryRow active={props.selectedCategoryId === ""} count={effectiveRecords.length} icon={<Tags />} label={t("agentSkills.all")} onSelect={() => props.onSelectCategory("")} />
      {props.workbench.agentSkills.categories.map((category) => (
        <DirectoryRow
          active={props.selectedCategoryId === category.categoryId}
          count={countByCategory.get(category.categoryId) ?? 0}
          key={category.categoryId}
          label={localizedAgentSkillCategory(category, locale, t)}
          onSelect={() => props.onSelectCategory(category.categoryId)}
          swatch={{ glyph: category.emoji, color: category.color }}
        />
      ))}
    </WorkspaceDirectoryMenu>
  )
}

// AgentSkillDetailDialog renders standard, provenance, resource, and instruction details.
// AgentSkillDetailDialog 渲染标准、来源、资源与说明详情。
function AgentSkillDetailDialog(props: {
  // Available logical categories used to localize the selected assignment.
  // 用于本地化所选归属的可用逻辑分类。
  categories: AgentSkillCategorySummary[]
  // Current interface locale.
  // 当前界面语言。
  locale: string
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Selected source record.
  // 选中的来源记录。
  record: AgentSkillRecordSummary | null
}) {
  const { t } = useI18n();
  const record = props.record
  // category resolves the selected record's exact logical category.
  // category 解析所选记录的精确逻辑分类。
  const category = record ? props.categories.find((item) => item.categoryId === record.categoryId) : undefined
  return (
    <Dialog onOpenChange={props.onOpenChange} open={record !== null}>
      <DialogContent className="agent-skill-detail-dialog" size="lg" scrollMode="content-scroll">
        <DialogHeader>
          <DialogTitle>{record ? localizedAgentSkillName(record, props.locale) : "Agent Skill"}</DialogTitle>
          <DialogDescription>{record ? localizedAgentSkillScope(record, t) : ""}</DialogDescription>
        </DialogHeader>
        {record ? (
          <div className="agent-skill-detail-body">
            <div className="lua-skill-detail-grid">
              <div><span>{t("agentSkills.status")}</span><strong>{record.enabled ? t("agentSkills.enabled") : t("agentSkills.disabled")}</strong></div>
              <div><span>{t("agentSkills.scope")}</span><strong>{localizedAgentSkillScope(record, t)}</strong></div>
              {record.owningPluginId ? <div><span>{t("agentSkills.owningPlugin")}</span><strong>{record.owningPluginId}</strong></div> : null}
              <div><span>{t("agentSkills.category")}</span><strong>{localizedAgentSkillCategory(category, props.locale, t)}</strong></div>
              <div><span>{t("agentSkills.origin")}</span><strong>{hasLinkedGitHubSourceOrigin(record.origin)
                ? <SourceOriginLink detail origin={record.origin} />
                : localizedAgentSkillOrigin(record, props.locale, t)}</strong></div>
              <div><span>{t("agentSkills.license")}</span><strong>{record.standard.license ?? "—"}</strong></div>
              <div><span>{t("agentSkills.compatibility")}</span><strong>{record.standard.compatibility ?? "—"}</strong></div>
              <div className="wide"><span>{t("agentSkills.resources")}</span><strong>{formatMessage(t("agentSkills.scriptsReferencesAssets"), { scripts: record.resources.scripts, references: record.resources.references, assets: record.resources.assets })}</strong></div>
              <div className="wide"><span>SKILL.md</span><strong>{record.skillFile}</strong></div>
            </div>
            <div className="skill-tool-row">
              <strong>{t("agentSkills.skillInstructions")}</strong>
              <div className="agent-definition-prompt-preview">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{record.body}</ReactMarkdown>
              </div>
            </div>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

// localizedAgentSkillName resolves the current-locale display name without changing model metadata.
// localizedAgentSkillName 在不改变模型元数据的情况下解析当前语言显示名称。
function localizedAgentSkillName(record: AgentSkillRecordSummary, locale: string): string {
  return localizedAgentSkillLocale(record, locale)?.displayName?.trim() || record.displayName || record.name
}

// localizedAgentSkillDescription resolves the current-locale compact description.
// localizedAgentSkillDescription 解析当前语言的紧凑说明。
function localizedAgentSkillDescription(record: AgentSkillRecordSummary, locale: string): string {
  return localizedAgentSkillLocale(record, locale)?.shortDescription?.trim() || record.description || record.standard.description
}

// localizedAgentSkillLocale resolves exact then language-prefix display metadata.
// localizedAgentSkillLocale 按精确语言再语言前缀解析展示元数据。
function localizedAgentSkillLocale(record: AgentSkillRecordSummary, locale: string) {
  return record.locales[locale]
    ?? Object.entries(record.locales).find(([candidate]) => candidate.split("-")[0] === locale.split("-")[0])?.[1]
}

// localizedAgentSkillCategory resolves the current-locale logical category name.
// localizedAgentSkillCategory 解析当前语言的逻辑分类名称。
function localizedAgentSkillCategory(category: AgentSkillCategorySummary | undefined, locale: string, t: Translate): string {
  if (!category) {
    return t("agentSkills.uncategorized")
  }
  return category.locales[locale]?.name
    ?? Object.entries(category.locales).find(([candidate]) => candidate.split("-")[0] === locale.split("-")[0])?.[1].name
    ?? category.name
}

// localizedAgentSkillOrigin resolves host-known provenance kinds without rewriting third-party labels.
// localizedAgentSkillOrigin 在不改写第三方标签的情况下解析宿主已知来源类型。
function localizedAgentSkillOrigin(record: AgentSkillRecordSummary, locale: string, t: Translate): string {
  if (record.origin.kind === "builtin") {
    return t("agentSkills.vulcanBuiltIn")
  }
  if (record.origin.kind === "local-created") {
    return t("agentSkills.userCreated")
  }
  if (record.origin.kind === "local-override") {
    return t("agentSkills.userOverride")
  }
  if (record.origin.kind === "unknown") {
    return t("agentSkills.unknownSource")
  }
  return record.origin.label?.trim() || record.origin.kind
}

// localizedAgentSkillScope resolves the exact source scope for the active interface language.
// localizedAgentSkillScope 为当前界面语言解析精确来源作用域。
function localizedAgentSkillScope(record: AgentSkillRecordSummary, t: Translate): string {
  if (record.scopeKey === "plugin") return t("agentSkills.plugin")
  return t(record.scopeKey === "system" ? "agentSkills.scopeSystem" : "agentSkills.scopeUser")
}

// agentSkillMatchesSearch matches every normalized query token across all deterministic metadata.
// agentSkillMatchesSearch 在全部确定性元数据中匹配每个规范化查询词。
function agentSkillMatchesSearch(record: AgentSkillRecordSummary, query: string): boolean {
  const tokens = query.trim().toLocaleLowerCase().split(/\s+/u).filter(Boolean)
  if (tokens.length === 0) {
    return true
  }
  const searchable = [
    record.name,
    record.displayName,
    record.description,
    record.standard.description,
    record.origin.kind,
    record.origin.label ?? "",
    record.categoryId,
    ...record.tags,
    ...Object.values(record.locales).flatMap((metadata) => [metadata.displayName ?? "", metadata.shortDescription ?? ""]),
  ].join("\n").toLocaleLowerCase()
  return tokens.every((token) => searchable.includes(token))
}
// Directory-only rules follow this lazy owner; shared workbench and responsive rules remain global.
// 目录专有规则随此懒加载归属载入；共用工作台与响应式规则保持全局。
import "@/ui-kit/styles/features/skill-directory.css";
