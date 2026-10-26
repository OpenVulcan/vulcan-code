import * as React from "react"
import "@/ui-kit/styles/features/changelog-settings.css"
import { ChevronLeft, ChevronRight } from "lucide-react"

import { MarkdownMessage } from "@/components/conversation/conversation-markdown"
import { PageNotice } from "@/components/page"
import { useI18n } from "@/i18n"
import { CHANGELOG_RELEASES, loadReleaseNotes } from "@/service/changelog"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"

// ChangelogSettingsArea pages through the release notes one release at a time in the interface language.
// ChangelogSettingsArea 以界面语言逐个发布翻阅更新记录。
// appVersion is the running application version, opened first and marked as current; returns a read-only surface.
// appVersion 是正在运行的应用版本，首先打开并标为当前版本；返回只读界面。
export function ChangelogSettingsArea(props: {
  // Running application version supplied by the workbench.
  // 工作台提供的正在运行的应用版本。
  appVersion: string
}) {
  const { t, locale } = useI18n()
  // page indexes CHANGELOG_RELEASES, newest first; it opens on the running version when listed.
  // page 是 CHANGELOG_RELEASES 的下标，从新到旧；运行版本已列出时首先打开它。
  const [page, setPage] = React.useState(() => Math.max(0, CHANGELOG_RELEASES.findIndex((release) => release.version === props.appVersion)))
  const [retry, setRetry] = React.useState(0)
  // Notes carry their request identity so a slow earlier page never paints over the current one.
  // 说明携带请求身份，较慢的旧页面不会覆盖当前页面。
  const [notes, setNotes] = React.useState<{ key: string; text: string; error: string } | null>(null)
  const release = CHANGELOG_RELEASES[page]
  const key = `${release.version}/${locale}`
  const top = React.useRef<HTMLDivElement>(null)

  React.useEffect(() => {
    let active = true
    void loadReleaseNotes(release.version, locale).then((text) => {
      if (active) setNotes({ key, text, error: "" })
    }).catch((error: unknown) => {
      if (active) setNotes({ key, text: "", error: String(error) })
    })
    return () => { active = false }
  }, [key, release.version, locale, retry])

  // turnTo opens another release and brings its heading into view, so the bottom pager starts reading at the top.
  // turnTo 打开另一个发布并让其标题进入视野，使底部翻页后从顶部开始阅读。
  // Parameters: next is a valid CHANGELOG_RELEASES index; updates page state and returns no value.
  // 参数：next 为有效的 CHANGELOG_RELEASES 下标；更新页面状态，不返回值。
  function turnTo(next: number) {
    setPage(next)
    top.current?.scrollIntoView({ block: "start" })
  }

  const date = release.date === null
    ? t("settings.changelog.unreleased")
    : new Intl.DateTimeFormat(locale, { dateStyle: "long", timeZone: "UTC" }).format(new Date(`${release.date}T00:00:00Z`))
  const pager = (
    <div className="settings-changelog-pager">
      <Button disabled={page === 0} onClick={() => turnTo(page - 1)} size="sm" variant="ghost"><ChevronLeft />{t("settings.changelog.newer")}</Button>
      <span className="settings-changelog-position">{page + 1} / {CHANGELOG_RELEASES.length}</span>
      <Button disabled={page === CHANGELOG_RELEASES.length - 1} onClick={() => turnTo(page + 1)} size="sm" variant="ghost">{t("settings.changelog.older")}<ChevronRight /></Button>
    </div>
  )

  return (
    <section className="settings-changelog">
      <header className="settings-changelog-head" ref={top}>
        <div className="settings-changelog-title">
          <strong className="vk-mono">{release.version}</strong>
          {release.version === props.appVersion ? <Badge variant="success">{t("settings.changelog.current")}</Badge> : null}
          <span>{date}</span>
        </div>
        <div className="settings-changelog-controls">
          <Select value={release.version} onValueChange={(version) => turnTo(CHANGELOG_RELEASES.findIndex((candidate) => candidate.version === version))}>
            <SelectTrigger aria-label={t("settings.changelog.version")}><SelectValue /></SelectTrigger>
            <SelectContent>{CHANGELOG_RELEASES.map((candidate) => <SelectItem key={candidate.version} value={candidate.version}>{candidate.version}</SelectItem>)}</SelectContent>
          </Select>
          {pager}
        </div>
      </header>
      {notes?.key !== key ? <StreamPlaceholder label={t("settings.changelog.loading")} count={8} />
        : notes.error ? <PageNotice actions={<Button onClick={() => setRetry((value) => value + 1)} size="sm" variant="outline">{t("settings.licenses.retry")}</Button>} details={notes.error} tone="error">{t("settings.changelog.loadError")}</PageNotice>
        : <>
          <MarkdownMessage content={notes.text} eager />
          {CHANGELOG_RELEASES.length > 1 ? <footer className="settings-changelog-foot">{pager}</footer> : null}
        </>}
    </section>
  )
}
