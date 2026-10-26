import releaseIndex from "@changelog/releases.json"

// ChangelogRelease is one entry of `changelog/releases.json` in the vulcan-i18n crate.
// ChangelogRelease 是 vulcan-i18n crate 中 `changelog/releases.json` 的一个条目。
export interface ChangelogRelease {
  // Exact `x.y.z` version.
  // 精确的 `x.y.z` 版本。
  version: string
  // Release day as `YYYY-MM-DD`; null while the newest release is unreleased.
  // 以 `YYYY-MM-DD` 表示的发布日；最新发布尚未发布时为 null。
  date: string | null
}

// CHANGELOG_RELEASES lists releases newest first; the crate build script validates order, dates and languages.
// CHANGELOG_RELEASES 按从新到旧列出发布；crate 构建脚本校验顺序、日期与语言。
export const CHANGELOG_RELEASES: readonly ChangelogRelease[] = releaseIndex.releases

// noteLoaders fetches each release and language from its own chunk, so the page downloads only what it shows.
// noteLoaders 从各自分块获取每个发布与语言，使页面只下载其展示的内容。
const noteLoaders = import.meta.glob<string>("@changelog/*/*.md", { query: "?raw", import: "default" })

// loadReleaseNotes returns the Markdown notes of one release in one registered language.
// loadReleaseNotes 返回一个发布在一种注册语言下的 Markdown 说明。
// Parameters: version is a listed release and locale a registered code; rejects when no such file was bundled.
// 参数：version 为已列出的发布，locale 为注册语言代码；未打包对应文件时拒绝。
export function loadReleaseNotes(version: string, locale: string): Promise<string> {
  const suffix = `/${version}/${locale}.md`
  const loader = Object.entries(noteLoaders).find(([path]) => path.endsWith(suffix))?.[1]
  return loader ? loader() : Promise.reject(new Error(`No release notes for ${version} in ${locale}`))
}
