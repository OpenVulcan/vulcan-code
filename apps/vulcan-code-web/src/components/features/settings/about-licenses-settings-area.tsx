import * as React from "react"
import "@/styles/licenses-settings.css"
import { ChevronLeft, ChevronRight, RefreshCw } from "lucide-react"

import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useI18n } from "@/i18n"
import {
  loadLicenseBody, loadLicenseCatalog,
  type LicenseCatalog, type LicenseDependency, type LicenseGroup,
} from "@/service/licenses-service"

// LICENSE_DEPENDENCY_PAGE_SIZE bounds each dependency table without dropping search results.
// LICENSE_DEPENDENCY_PAGE_SIZE 限制每页依赖表大小，不丢弃搜索结果。
const LICENSE_DEPENDENCY_PAGE_SIZE = 30

// GroupDependency retains all body associations for one exact package/version/source identity.
// GroupDependency 为一个精确包、版本、来源身份保留全部正文关联。
interface GroupDependency extends LicenseDependency {
  // Every original body covering this package identity.
  // 覆盖此包身份的每份原始正文。
  sectionIds: string[]
}

// collectLicenseDependencies deduplicates exact tuples while preserving all original body associations.
// collectLicenseDependencies 对精确元组去重，同时保留全部原始正文关联。
// Parameter groups supplies validated metadata; returns dependencies in deterministic source order.
// 参数 groups 提供已验证元数据；返回确定性来源顺序的依赖。
export function collectLicenseDependencies(groups: LicenseGroup[]): GroupDependency[] {
  const entries = new Map<string, GroupDependency>()
  for (const group of groups) {
    for (const section of group.sections) {
      for (const dependency of section.dependencies) {
        const key = JSON.stringify([dependency.name, dependency.version, dependency.source])
        const existing = entries.get(key)
        if (existing) {
          if (!existing.sectionIds.includes(section.id)) existing.sectionIds.push(section.id)
        } else {
          entries.set(key, { ...dependency, sectionIds: [section.id] })
        }
      }
    }
  }
  return [...entries.values()]
}

// licenseDependencyMatches searches only the structured fields actually emitted by the generator.
// licenseDependencyMatches 仅搜索生成器实际输出的结构化字段。
// Parameters: dependency carries exact generated fields and query is normalized search text; returns whether name, version or source contains the query.
// 参数：dependency 携带精确生成字段，query 是规范搜索文本；返回名称、版本或来源是否包含查询。
function licenseDependencyMatches(dependency: LicenseDependency, query: string): boolean {
  return [dependency.name, dependency.version, dependency.source].some((value) => value.toLocaleLowerCase().includes(query))
}

// AboutLicensesSettingsArea displays generated license groups, exact dependencies and lazy original bodies.
// AboutLicensesSettingsArea 展示生成的许可证分组、精确依赖和懒加载原始正文。
// appVersion is the existing application version; returns a read-only browser/desktop-compatible surface.
// appVersion 是现有应用版本；返回兼容浏览器和桌面的只读界面。
export function AboutLicensesSettingsArea(props: {
  // Existing application version supplied by the workbench.
  // 工作台提供的现有应用版本。
  appVersion: string
}) {
  const { t } = useI18n()
  // Catalog state distinguishes loading from explicit artifact errors.
  // 目录状态区分加载过程与明确的制品错误。
  const [catalog, setCatalog] = React.useState<LicenseCatalog | null>(null)
  const [catalogError, setCatalogError] = React.useState("")
  const [reload, setReload] = React.useState(0)
  // Selection and search are local read-only UI state.
  // 选择与搜索是本地只读界面状态。
  const [query, setQuery] = React.useState("")
  const [groupId, setGroupId] = React.useState("")
  const [sectionId, setSectionId] = React.useState("")
  const [page, setPage] = React.useState(0)
  // viewedDependency scopes the modal to the exact package and its associated original license bodies.
  // viewedDependency 将弹窗限定到精确依赖包及其关联的原始许可证正文。
  const [viewedDependency, setViewedDependency] = React.useState<GroupDependency | null>(null)
  // Body responses carry their request identity so old requests never paint a new selection.
  // 正文响应携带请求身份，防止旧请求覆盖新选择。
  const [body, setBody] = React.useState<{ sectionId: string; revision: string; text: string; error: string } | null>(null)
  const [bodyRetry, setBodyRetry] = React.useState(0)

  React.useEffect(() => {
    let active = true
    setCatalog(null)
    setCatalogError("")
    void loadLicenseCatalog().then((result) => {
      if (active) setCatalog(result)
    }).catch((error: unknown) => {
      if (active) setCatalogError(String(error))
    })
    return () => { active = false }
  }, [reload])

  // Search filters both type names and real package identities; groups retain all original variants.
  // 搜索同时筛选类型名称和真实包身份；分组保留全部原始变体。
  const normalizedQuery = query.trim().toLocaleLowerCase()
  const allGroups = catalog?.licenses ?? []
  const filteredGroups = allGroups.filter((group) =>
    group.title.toLocaleLowerCase().includes(normalizedQuery)
    || group.sections.some((section) => section.dependencies.some((dependency) => licenseDependencyMatches(dependency, normalizedQuery))))
  const selectedGroup = filteredGroups.find((group) => group.id === groupId) ?? filteredGroups[0]
  const selectedSection = selectedGroup?.sections.find((section) => section.id === sectionId) ?? selectedGroup?.sections[0]
  const dependencies = React.useMemo(() => selectedGroup ? collectLicenseDependencies([selectedGroup]) : [], [selectedGroup])
  const matchingDependencies = selectedGroup?.title.toLocaleLowerCase().includes(normalizedQuery)
    ? dependencies
    : dependencies.filter((dependency) => licenseDependencyMatches(dependency, normalizedQuery))
  const currentPage = Math.min(page, Math.max(0, Math.ceil(matchingDependencies.length / LICENSE_DEPENDENCY_PAGE_SIZE) - 1))
  const visibleDependencies = matchingDependencies.slice(currentPage * LICENSE_DEPENDENCY_PAGE_SIZE, (currentPage + 1) * LICENSE_DEPENDENCY_PAGE_SIZE)
  const totalDependencies = React.useMemo(() => collectLicenseDependencies(catalog?.licenses ?? []).length, [catalog])
  const revision = catalog?.source_sha256
  const requestedSectionId = selectedSection?.id

  React.useEffect(() => {
    if (!viewedDependency || !requestedSectionId || !revision) return
    let active = true
    setBody(null)
    void loadLicenseBody(requestedSectionId, revision).then((result) => {
      if (active) setBody({ sectionId: result.section_id, revision, text: result.text, error: "" })
    }).catch((error: unknown) => {
      if (active) setBody({ sectionId: requestedSectionId, revision, text: "", error: String(error) })
    })
    return () => { active = false }
  }, [viewedDependency, requestedSectionId, revision, bodyRetry])

  // selectGroup resets pagination and body selection only on explicit category navigation.
  // selectGroup 仅在明确切换类别时重置分页和正文选择。
  // Parameters: id is a catalog-owned license group; updates local group, body and pagination state and returns no value.
  // 参数：id 是目录持有的许可证分组；更新本地分组、正文及分页状态，不返回值。
  function selectGroup(id: string) {
    setGroupId(id)
    setSectionId("")
    setPage(0)
  }

  return (
    <section className="settings-preferences-page settings-detail-panel settings-license-panel">
      <div className="settings-license-summary">
        <strong>Vulcan Code</strong><Badge variant="outline">{props.appVersion}</Badge>
        {catalog && <span>{allGroups.length} {t("settings.licenses.types")} · {totalDependencies} {t("settings.licenses.dependencies")}</span>}
      </div>
      <p className="settings-hint">{t("settings.licenses.scope")}</p>
      <div className="settings-license-toolbar">
        <Input aria-label={t("settings.licenses.search")} placeholder={t("settings.licenses.search")} value={query}
          onChange={(event) => { setQuery(event.target.value); setPage(0) }} />
        <Button variant="outline" onClick={() => setReload((value) => value + 1)}><RefreshCw size={14} />{t("settings.licenses.reload")}</Button>
      </div>
      {catalogError ? <div role="alert" className="settings-license-error"><strong>{t("settings.licenses.loadError")}</strong><p>{catalogError}</p></div>
        : !catalog ? <StreamPlaceholder fill label={t("settings.licenses.loading")} shape="rows" count={6} />
        : filteredGroups.length === 0 ? <p role="status">{t("settings.licenses.emptySearch")}</p>
        : <div className="settings-license-browser">
          <nav className="settings-license-directory" aria-label={t("settings.licenses.types")}>
            {filteredGroups.map((group) => <Button key={group.id} variant={selectedGroup?.id === group.id ? "secondary" : "ghost"}
              aria-current={selectedGroup?.id === group.id ? "page" : undefined} onClick={() => selectGroup(group.id)}>
              <span className="settings-license-name"><span>{group.title || t("settings.licenses.unknown")}</span></span><Badge variant="outline">{collectLicenseDependencies([group]).length}</Badge>
            </Button>)}
          </nav>
          <div className="settings-license-content">
            <div className="settings-license-mobile-directory">
              <Select value={selectedGroup?.id} onValueChange={selectGroup}>
                <SelectTrigger aria-label={t("settings.licenses.types")}><SelectValue /></SelectTrigger>
                <SelectContent className="settings-license-type-options">{filteredGroups.map((group) => <SelectItem key={group.id} value={group.id}>{group.title || t("settings.licenses.unknown")}</SelectItem>)}</SelectContent>
              </Select>
            </div>
            {selectedGroup && <>
              <div className="settings-license-group-heading"><h3>{selectedGroup.title || t("settings.licenses.unknown")}</h3>
                {selectedGroup.status === "unknown" && <Badge variant="warning">{t("settings.licenses.unknown")}</Badge>}
                <span>{dependencies.length} {t("settings.licenses.dependencies")} · {selectedGroup.sections.length} {t("settings.licenses.variants")}</span>
              </div>
              <div className="settings-license-table-scroll"><table className="settings-license-table">
                <caption className="sr-only">{selectedGroup.title || t("settings.licenses.unknown")} · {t("settings.licenses.dependencies")}</caption>
                <thead><tr><th>{t("settings.licenses.dependency")}</th><th>{t("settings.licenses.version")}</th><th>{t("settings.licenses.source")}</th><th>{t("settings.licenses.view")}</th></tr></thead>
                <tbody>{visibleDependencies.map((dependency) => <tr key={JSON.stringify([dependency.name, dependency.version, dependency.source])}>
                  <td>{dependency.name}</td><td>{dependency.version}</td><td><span>{dependency.source}</span></td>
                  <td><Button variant="ghost" size="sm" onClick={() => {
                    setSectionId(dependency.sectionIds[0])
                    setViewedDependency(dependency)
                  }}>{t("settings.licenses.view")}</Button></td>
                </tr>)}</tbody>
              </table></div>
              {matchingDependencies.length > LICENSE_DEPENDENCY_PAGE_SIZE && <div className="settings-license-pagination">
                <span>{currentPage * LICENSE_DEPENDENCY_PAGE_SIZE + 1}–{Math.min((currentPage + 1) * LICENSE_DEPENDENCY_PAGE_SIZE, matchingDependencies.length)} / {matchingDependencies.length}</span>
                <Button variant="outline" size="sm" aria-label={t("settings.licenses.previous")} disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}><ChevronLeft size={14} /></Button>
                <Button variant="outline" size="sm" aria-label={t("settings.licenses.next")} disabled={(currentPage + 1) * LICENSE_DEPENDENCY_PAGE_SIZE >= matchingDependencies.length} onClick={() => setPage(currentPage + 1)}><ChevronRight size={14} /></Button>
              </div>}
              <Dialog open={viewedDependency !== null} onOpenChange={(open) => { if (!open) setViewedDependency(null) }}>
                <DialogContent size="xl" scrollMode="body-scroll" className="settings-license-dialog">
                  <DialogHeader>
                    <DialogTitle>{viewedDependency?.name} <span className="text-muted-foreground font-normal">{viewedDependency?.version}</span></DialogTitle>
                    <DialogDescription>{selectedGroup.title || t("settings.licenses.unknown")}</DialogDescription>
                  </DialogHeader>
              {viewedDependency && viewedDependency.sectionIds.length > 1 && <div className="settings-license-body-heading"><h3>{t("settings.licenses.body")}</h3>
                <Select value={selectedSection?.id} onValueChange={setSectionId}>
                  <SelectTrigger aria-label={t("settings.licenses.variant")}><SelectValue /></SelectTrigger>
                  <SelectContent>{selectedGroup.sections.filter((section) => viewedDependency.sectionIds.includes(section.id)).map((section, index) => <SelectItem key={section.id} value={section.id}>
                    {t("settings.licenses.versionLabel")} {index + 1} · {section.dependencies.length} {t("settings.licenses.dependencies")}
                  </SelectItem>)}</SelectContent>
                </Select>
              </div>}
              {viewedDependency && viewedDependency.sectionIds.length > 1 && <p className="settings-hint">{t("settings.licenses.variantHint")}</p>}
              {body?.sectionId === requestedSectionId && body.revision === revision
                ? body.error ? <div role="alert" className="settings-license-error"><p>{body.error}</p><Button variant="outline" onClick={() => setBodyRetry((value) => value + 1)}>{t("settings.licenses.retry")}</Button></div>
                  : <pre className="settings-license-body" tabIndex={0}>{body.text}</pre>
                : <StreamPlaceholder className="p-1" label={t("settings.licenses.loadingBody")} count={12} />}
                </DialogContent>
              </Dialog>
            </>}
          </div>
        </div>}
    </section>
  )
}
