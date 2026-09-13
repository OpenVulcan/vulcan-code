import { WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout";
import * as React from "react"
import { Copy, FolderOpen, Pencil, Plus, RefreshCw, RotateCcw, Search, Tags, Trash2 } from "lucide-react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"

import { agentSkillsCatalogSummary, type AgentSkillActionProps } from "@/components/features/agent-skills/agent-skills-types"
import { AgentSkillCategoryManagementDialog, AgentSkillEditorDialog } from "@/components/features/agent-skills/agent-skill-management-dialogs"
import { hasLinkedGitHubSourceOrigin, SourceOriginLink } from "@/components/features/source-origin-link"
import { Badge } from "@/components/ui/badge"
import { Button, IconButton } from "@/components/ui/button"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog"
import { FilterChip, FilterChipGroup } from "@/components/ui/choice-controls"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import type {
  AgentSkillCategorySummary,
  AgentSkillRecordSummary,
  WorkbenchSnapshot,
} from "@/types"

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
        <div className="agent-catalog-toolbar">
          <div className="agent-catalog-toolbar-main">
            <div className="agent-catalog-toolbar-heading">
              <h2>{locale.startsWith("zh") ? "技能" : "Skills"}</h2>
              <p>{agentSkillsCatalogSummary(props.workbench.agentSkills, t)}</p>
            </div>
            <div className="agent-catalog-toolbar-actions">
              <Button disabled={pageBusy} onClick={() => openEditor(null)}>
                <Plus size={15} />
                {locale.startsWith("zh") ? "新建技能" : "Create skill"}
              </Button>
              <Button
                disabled={pageBusy}
                onClick={() => {
                  setCategoryDialogOpen(true)
                }}
                variant="outline"
              >
                <Tags size={15} />
                {locale.startsWith("zh") ? "分类管理" : "Manage categories"}
              </Button>
              <Button
                disabled={pageBusy}
                onClick={() => void runCatalogAction("open-directory", props.actions.onOpenAgentSkillsUserDirectory)}
                variant="outline"
              >
                <FolderOpen size={15} />
                {locale.startsWith("zh") ? "技能目录" : "Skill directory"}
              </Button>
              <Button
                disabled={pageBusy}
                onClick={() => void runCatalogAction("refresh", props.actions.onRefreshAgentSkills)}
                variant="outline"
              >
                <RefreshCw size={15} />
                {locale.startsWith("zh") ? "刷新目录" : "Refresh catalog"}
              </Button>
            </div>
          </div>
          <div className="agent-catalog-filter-row">
            <label className="agent-catalog-search-input">
              <Search size={15} />
              <input
                aria-label={locale.startsWith("zh") ? "搜索技能" : "Search skills"}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder={locale.startsWith("zh") ? "搜索名称、描述、标签或来源" : "Search names, descriptions, tags, or sources"}
                value={searchQuery}
              />
            </label>
            <FilterChipGroup aria-label={locale.startsWith("zh") ? "按状态筛选技能" : "Filter skills by state"}>
              <FilterChip onClick={() => setStatusFilter("all")} selected={statusFilter === "all"}>{locale.startsWith("zh") ? "全部" : "All"}</FilterChip>
              <FilterChip onClick={() => setStatusFilter("enabled")} selected={statusFilter === "enabled"}>{locale.startsWith("zh") ? "已启用" : "Enabled"}</FilterChip>
              <FilterChip onClick={() => setStatusFilter("disabled")} selected={statusFilter === "disabled"}>{locale.startsWith("zh") ? "已停用" : "Disabled"}</FilterChip>
              <FilterChip onClick={() => setStatusFilter("issues")} selected={statusFilter === "issues"}>{locale.startsWith("zh") ? "问题" : "Issues"}</FilterChip>
            </FilterChipGroup>
            <Select onValueChange={(value) => setSourceFilter(value as AgentSkillSourceFilter)} value={sourceFilter}>
              <SelectTrigger aria-label={locale.startsWith("zh") ? "按来源筛选技能" : "Filter skills by source"} className="agent-catalog-status-select">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{locale.startsWith("zh") ? "全部来源" : "All sources"}</SelectItem>
                <SelectItem value="system">{locale.startsWith("zh") ? "系统技能" : "System"}</SelectItem>
                <SelectItem value="user">{locale.startsWith("zh") ? "用户技能" : "User"}</SelectItem>
                <SelectItem value="plugin">{locale.startsWith("zh") ? "插件技能" : "Plugins"}</SelectItem>
                <SelectItem value="overrides">{locale.startsWith("zh") ? "用户覆盖" : "Overrides"}</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>

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
            <div className="skill-empty-state">{locale.startsWith("zh") ? "没有符合当前筛选条件的技能。" : "No skills match the current filters."}</div>
          ) : visibleRecords.map((record) => (
            <article className="agent-definition-row" key={record.skillKey}>
              <button className="agent-definition-row-icon" onClick={() => setDetailRecord(record)} title={locale.startsWith("zh") ? "详情" : "Details"} type="button">
                <span className="agent-definition-row-glyph">
                  {record.emoji?.trim() || localizedAgentSkillName(record, locale).slice(0, 1).toUpperCase()}
                </span>
                <span className="agent-definition-row-color-marker" style={{ backgroundColor: record.color ?? categoryById.get(record.categoryId)?.color ?? "#64748B" }} />
              </button>
              <div className="agent-definition-row-main">
                <div className="agent-definition-row-title-line">
                  <button className="agent-definition-title-button" onClick={() => setDetailRecord(record)} type="button">{localizedAgentSkillName(record, locale)}</button>
                  <div className="agent-definition-row-badges">
                    <Badge variant="outline">{localizedAgentSkillScope(record, locale)}</Badge>
                    <Badge variant="outline">{localizedAgentSkillCategory(categoryById.get(record.categoryId), locale)}</Badge>
                    {hasLinkedGitHubSourceOrigin(record.origin)
                      ? <SourceOriginLink origin={record.origin} />
                      : <Badge variant="outline">{localizedAgentSkillOrigin(record, locale)}</Badge>}
                    {record.overrideOf ? <Badge variant="outline">{locale.startsWith("zh") ? "用户覆盖" : "Override"}</Badge> : null}
                    {record.owningPluginId ? <Badge variant="outline">{record.owningPluginId}</Badge> : null}
                    {record.owningPluginId ? (
                      <Badge variant={record.enabled ? "secondary" : "outline"}>
                        {record.enabled
                          ? (locale.startsWith("zh") ? "已启用" : "Enabled")
                          : (locale.startsWith("zh") ? "已停用" : "Disabled")}
                      </Badge>
                    ) : null}
                    {!record.effective ? <Badge variant="outline">{locale.startsWith("zh") ? "非生效来源" : "Inactive source"}</Badge> : null}
                  </div>
                </div>
                <p>{localizedAgentSkillDescription(record, locale) || (locale.startsWith("zh") ? "未填写描述。" : "No description provided.")}</p>
                <span className="agent-definition-row-path">{record.skillFile}</span>
              </div>
              <div className="agent-definition-row-actions">
                {record.canToggle ? <Switch
                  checked={record.enabled}
                  aria-label={record.enabled ? (locale.startsWith("zh") ? "停用" : "Disable") : (locale.startsWith("zh") ? "启用" : "Enable")}
                  disabled={!record.effective || busyRows.has(record.name) || pageBusy}
                  onCheckedChange={() => void runRowAction(record)}
                  title={record.enabled ? (locale.startsWith("zh") ? "停用" : "Disable") : (locale.startsWith("zh") ? "启用" : "Enable")}
                /> : null}
                {record.effective && record.canEdit ? (
                  <IconButton aria-label={record.scopeKey === "system" ? (locale.startsWith("zh") ? "创建用户覆盖" : "Create override") : (locale.startsWith("zh") ? "编辑" : "Edit")} disabled={pageBusy} onClick={() => openEditor(record)} title={record.scopeKey === "system" ? (locale.startsWith("zh") ? "创建用户覆盖" : "Create override") : (locale.startsWith("zh") ? "编辑" : "Edit")} type="button">
                    <Pencil size={14} />
                  </IconButton>
                ) : null}
                {record.canCopy ? <IconButton aria-label={locale.startsWith("zh") ? "复制" : "Copy"} disabled={pageBusy} onClick={() => openEditor(record, true)} title={locale.startsWith("zh") ? "复制" : "Copy"} type="button">
                  <Copy size={14} />
                </IconButton> : null}
                {record.canRestoreDefault ? (
                  <IconButton aria-label={locale.startsWith("zh") ? "恢复系统版本" : "Restore system version"} disabled={pageBusy} onClick={() => void runCatalogAction(`restore:${record.name}`, () => restoreSystemSkill(record))} title={locale.startsWith("zh") ? "恢复系统版本" : "Restore system version"} type="button">
                    <RotateCcw size={14} />
                  </IconButton>
                ) : null}
                {record.canDelete ? (
                  <IconButton aria-label={locale.startsWith("zh") ? "删除" : "Delete"} disabled={pageBusy} onClick={() => void runCatalogAction(`delete:${record.name}`, () => deleteUserSkill(record))} title={locale.startsWith("zh") ? "删除" : "Delete"} type="button" variant="destructive">
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
            <AlertDialogTitle>{pendingMutation?.kind === "restore" ? (locale.startsWith("zh") ? "恢复系统版本" : "Restore system version") : (locale.startsWith("zh") ? "删除技能" : "Delete skill")}</AlertDialogTitle>
            <AlertDialogDescription>{pendingMutation ? (pendingMutation.kind === "restore"
              ? (locale.startsWith("zh") ? `移除“${localizedAgentSkillName(pendingMutation.record, locale)}”的用户覆盖并恢复系统版本？` : `Remove the user override for “${localizedAgentSkillName(pendingMutation.record, locale)}” and restore the system version?`)
              : (locale.startsWith("zh") ? `删除用户技能“${localizedAgentSkillName(pendingMutation.record, locale)}”及其资源目录？` : `Delete user skill “${localizedAgentSkillName(pendingMutation.record, locale)}” and its resource directory?`)) : ""}</AlertDialogDescription>
          </AlertDialogHeader>
          {actionError ? <p role="alert" className="text-sm text-destructive">{actionError}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pageBusy}>{locale.startsWith("zh") ? "取消" : "Cancel"}</AlertDialogCancel>
            <AlertDialogAction disabled={pageBusy} onClick={(event) => {
              event.preventDefault()
              if (!pendingMutation) return
              void runCatalogAction(pendingMutation.kind, async () => {
                if (pendingMutation.kind === "restore") await props.actions.onRestoreAgentSkillDefault(pendingMutation.record.name)
                else await props.actions.onDeleteAgentSkill(pendingMutation.record.name)
                setDetailRecord(null)
                setPendingMutation(null)
              })
            }}>{pageBusy ? (locale.startsWith("zh") ? "处理中…" : "Working…") : (locale.startsWith("zh") ? "确认" : "Confirm")}</AlertDialogAction>
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
  const { locale } = useI18n()
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
    <WorkspaceDirectoryMenu ariaLabel={locale.startsWith("zh") ? "技能分类" : "Skill categories"} heading={locale.startsWith("zh") ? "分类" : "Categories"}>
      <button className={`workspace-tool-panel-menu-row agent-category-floating-row ${props.selectedCategoryId === "" ? "active" : ""}`} aria-current={props.selectedCategoryId === "" ? "page" : undefined} onClick={() => props.onSelectCategory("")} type="button">
        <Tags size={15} />
        <span>{locale.startsWith("zh") ? "全部" : "All"}</span>
        <strong>{effectiveRecords.length}</strong>
      </button>
      {props.workbench.agentSkills.categories.map((category) => (
        <div className={`workspace-tool-panel-menu-row agent-skill-category-floating-row ${props.selectedCategoryId === category.categoryId ? "active" : ""}`} key={category.categoryId}>
          <button className="agent-skill-category-floating-main" aria-current={props.selectedCategoryId === category.categoryId ? "page" : undefined} onClick={() => props.onSelectCategory(category.categoryId)} type="button">
            <span className="agent-category-floating-icon" style={{ backgroundColor: category.color }}>{category.emoji}</span>
            <span>{localizedAgentSkillCategory(category, locale)}</span>
            <strong>{countByCategory.get(category.categoryId) ?? 0}</strong>
          </button>
        </div>
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
  const record = props.record
  // chinese selects localized detail labels and resource units.
  // chinese 选择本地化详情标签与资源单位。
  const chinese = props.locale.startsWith("zh")
  // category resolves the selected record's exact logical category.
  // category 解析所选记录的精确逻辑分类。
  const category = record ? props.categories.find((item) => item.categoryId === record.categoryId) : undefined
  return (
    <Dialog onOpenChange={props.onOpenChange} open={record !== null}>
      <DialogContent className="agent-skill-detail-dialog grid-rows-[auto_minmax(0,1fr)]" size="lg" scrollMode="content-scroll">
        <DialogHeader>
          <DialogTitle>{record ? localizedAgentSkillName(record, props.locale) : "Agent Skill"}</DialogTitle>
          <DialogDescription>{record ? localizedAgentSkillScope(record, props.locale) : ""}</DialogDescription>
        </DialogHeader>
        {record ? (
          <div className="agent-skill-detail-body">
            <div className="lua-skill-detail-grid">
              <div><span>{chinese ? "当前状态" : "Status"}</span><strong>{record.enabled ? (chinese ? "已启用" : "Enabled") : (chinese ? "已停用" : "Disabled")}</strong></div>
              <div><span>{chinese ? "来源" : "Scope"}</span><strong>{localizedAgentSkillScope(record, props.locale)}</strong></div>
              {record.owningPluginId ? <div><span>{chinese ? "所属插件" : "Owning plugin"}</span><strong>{record.owningPluginId}</strong></div> : null}
              <div><span>{chinese ? "分类" : "Category"}</span><strong>{localizedAgentSkillCategory(category, props.locale)}</strong></div>
              <div><span>{chinese ? "来源类型" : "Origin"}</span><strong>{hasLinkedGitHubSourceOrigin(record.origin)
                ? <SourceOriginLink detail origin={record.origin} />
                : localizedAgentSkillOrigin(record, props.locale)}</strong></div>
              <div><span>{chinese ? "许可证" : "License"}</span><strong>{record.standard.license ?? "—"}</strong></div>
              <div><span>{chinese ? "兼容性" : "Compatibility"}</span><strong>{record.standard.compatibility ?? "—"}</strong></div>
              <div className="wide"><span>{chinese ? "资源" : "Resources"}</span><strong>{chinese
                ? `${record.resources.scripts} 个脚本 · ${record.resources.references} 个参考资料 · ${record.resources.assets} 个资源文件`
                : `${record.resources.scripts} scripts · ${record.resources.references} references · ${record.resources.assets} assets`}</strong></div>
              <div className="wide"><span>SKILL.md</span><strong>{record.skillFile}</strong></div>
            </div>
            <div className="skill-tool-row">
              <strong>{chinese ? "技能说明" : "Skill instructions"}</strong>
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
function localizedAgentSkillCategory(category: AgentSkillCategorySummary | undefined, locale: string): string {
  if (!category) {
    return locale.startsWith("zh") ? "未分配" : "Uncategorized"
  }
  return category.locales[locale]?.name
    ?? Object.entries(category.locales).find(([candidate]) => candidate.split("-")[0] === locale.split("-")[0])?.[1].name
    ?? category.name
}

// localizedAgentSkillOrigin resolves host-known provenance kinds without rewriting third-party labels.
// localizedAgentSkillOrigin 在不改写第三方标签的情况下解析宿主已知来源类型。
function localizedAgentSkillOrigin(record: AgentSkillRecordSummary, locale: string): string {
  // chinese stores whether known host provenance should use Chinese copy.
  // chinese 保存宿主已知来源是否应使用中文文案。
  const chinese = locale.startsWith("zh")
  if (record.origin.kind === "builtin") {
    return chinese ? "Vulcan 内置" : "Vulcan built-in"
  }
  if (record.origin.kind === "local-created") {
    return chinese ? "用户创建" : "User-created"
  }
  if (record.origin.kind === "local-override") {
    return chinese ? "用户覆盖" : "User override"
  }
  if (record.origin.kind === "unknown") {
    return chinese ? "未知来源" : "Unknown source"
  }
  return record.origin.label?.trim() || record.origin.kind
}

// localizedAgentSkillScope resolves the exact source scope for the active interface language.
// localizedAgentSkillScope 为当前界面语言解析精确来源作用域。
function localizedAgentSkillScope(record: AgentSkillRecordSummary, locale: string): string {
  if (record.scopeKey === "plugin") {
    return locale.startsWith("zh") ? "插件" : "Plugin"
  }
  if (locale.startsWith("zh")) {
    return record.scopeKey === "system" ? "系统" : "用户"
  }
  return record.scopeKey === "system" ? "System" : "User"
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
import "@/styles/skill-directory.css";
