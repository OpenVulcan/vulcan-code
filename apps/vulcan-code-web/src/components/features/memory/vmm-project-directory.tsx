import * as React from "react"
import { Button } from "@/components/ui/button"
import { Combobox } from "@/components/ui/combobox"
import { Input } from "@/components/ui/input"
import { useI18n } from "@/i18n"

// ProjectDirectoryEntry keeps host and memory identities explicit; descriptions disambiguate equal names.
// ProjectDirectoryEntry 明确保留宿主与记忆身份；说明用于区分同名项目。
export interface ProjectDirectoryEntry {
  // Stable identity, display name, readable identity description and authoritative binding/override flags.
  // 稳定身份、显示名称、可读身份说明以及权威绑定/覆盖标记。
  value: string
  label: string
  description: string
  bound: boolean
  hasOverride: boolean
}

// Renders a searchable directory and a narrow-panel picker for the same controlled selection.
// 为同一个受控选择渲染可搜索目录和窄面板选择器。
// Entries supply stable identities; onChange changes only selection. Returns the responsive directory.
// entries 提供稳定身份；onChange 仅改变选择。返回响应式目录。
export function VmmProjectDirectory({ entries, value, disabled, allowOverrides = true, onChange }: {
  entries: ProjectDirectoryEntry[]
  value: string
  disabled: boolean
  // Only host projects have the override fact supplied by the current project-scope contract.
  // 只有宿主项目拥有当前项目作用域契约提供的覆盖事实。
  allowOverrides?: boolean
  onChange: (value: string) => void
}) {
  // Search and filter affect visibility without replacing the currently edited project.
  // 搜索与筛选只改变可见性，不替换当前编辑项目。
  const [search, setSearch] = React.useState("")
  const [filter, setFilter] = React.useState("all")
  const { t } = useI18n()
  // Visible entries retain source order and exact identity matching.
  // 可见项保留来源顺序及准确身份匹配。
  const visible = entries.filter((entry) => `${entry.label} ${entry.description}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()) && (filter === "all" || (filter === "bound" ? entry.bound : filter === "unbound" ? !entry.bound : entry.hasOverride)))
  return <div className="vmm-project-directory">
    <div className="vmm-project-picker"><Combobox value={value} options={entries} onValueChange={onChange} disabled={disabled} ariaLabel={t("memory.nativeRecords.project")} placeholder={t("memory.nativeRecords.selectProject")} searchPlaceholder={t("memory.profile.searchProject")} emptyText={t("memory.nativeRecords.noProjects")} /></div>
    <div className="vmm-project-directory-expanded">
      <Input aria-label={t("memory.profile.searchProject")} placeholder={t("memory.profile.searchProject")} value={search} onChange={(event) => setSearch(event.target.value)} />
      <div className="vmm-directory-filters">{(["all", "bound", "unbound", "overrides"] as const).filter((kind) => kind !== "overrides" || allowOverrides).map((kind) => <Button type="button" key={kind} size="sm" variant={filter === kind ? "secondary" : "ghost"} aria-pressed={filter === kind} onClick={() => setFilter(kind)}>{t(`memory.workspace.${kind}`)}</Button>)}</div>
      <div className="vmm-project-list" role="group" aria-label={t("memory.nativeRecords.project")}>{visible.map((entry) => <Button key={entry.value} variant="ghost" disabled={disabled} aria-pressed={value === entry.value} title={`${entry.label}\n${entry.description}`} onClick={() => onChange(entry.value)}><span className="vmm-project-list-identity"><span className="block truncate">{entry.label}</span><span className="block truncate text-xs text-muted-foreground">{entry.description}</span></span><span className="vmm-project-list-state">{t(entry.bound ? "memory.workspace.bound" : "memory.workspace.unbound")}</span></Button>)}</div>
      {visible.length === 0 ? <p className="text-sm text-muted-foreground">{t("memory.workspace.noMatches")}</p> : null}
    </div>
  </div>
}
