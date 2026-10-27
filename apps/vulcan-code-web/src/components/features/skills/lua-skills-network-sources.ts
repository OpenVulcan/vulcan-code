import type {
  LuaSkillsCustomGithubMirror,
  LuaSkillsGithubMirror,
  LuaSkillsNetworkSettings,
  LuaSkillsRuntimeSettings,
} from "@/types"
import type { OpenVulcanRegion } from "@/service/openvulcan-account-service"

// LuaSkillsHubSiteId names one official Hub site; "custom" covers any other address.
// LuaSkillsHubSiteId 指定一个官方 Hub 站点；"custom" 表示其他任意地址。
export type LuaSkillsHubSiteId = "global" | "china" | "custom"

// LUASKILLS_HUB_SITE_URLS lists the two official Hub sites; the global site is the service default.
// LUASKILLS_HUB_SITE_URLS 列出两个官方 Hub 站点；国际站是服务端默认值。
export const LUASKILLS_HUB_SITE_URLS: Readonly<Record<OpenVulcanRegion, string>> = {
  global: "https://hub.luaskills.com",
  china: "https://hub.luaskills.cn",
}

// LUASKILLS_GITHUB_TOKEN_PAGE opens GitHub's classic token form with a recognizable note; no scope is
// needed because the token only raises the anonymous rate limit for public reads.
// LUASKILLS_GITHUB_TOKEN_PAGE 打开带可识别备注的 GitHub 经典令牌表单；令牌只用于提升公开读取的匿名
// 速率上限，因此无需任何权限范围。
export const LUASKILLS_GITHUB_TOKEN_PAGE = "https://github.com/settings/tokens/new?description=Vulcan%20Code%20LuaSkills"

// trimmedUrl normalizes an optional address for comparison: trimmed, without a trailing slash, null when empty.
// trimmedUrl 规范化可选地址以便比较：去除空白与末尾斜杠，为空时返回 null。
export function trimmedUrl(value: string | null | undefined): string | null {
  const trimmed = value?.trim().replace(/\/+$/, "") ?? ""
  return trimmed ? trimmed : null
}

// effectiveHubBaseUrl returns the Hub the service actually uses for the given settings.
// effectiveHubBaseUrl 返回服务按给定设置实际使用的 Hub。
export function effectiveHubBaseUrl(network: LuaSkillsNetworkSettings): string {
  return trimmedUrl(network.officialSkillHubBaseUrl) ?? LUASKILLS_HUB_SITE_URLS.global
}

// hubSiteOf classifies one Hub address as an official site or a custom one.
// hubSiteOf 将一个 Hub 地址归类为官方站点或自定义地址。
export function hubSiteOf(hubBaseUrl: string): LuaSkillsHubSiteId {
  if (hubBaseUrl === LUASKILLS_HUB_SITE_URLS.global) return "global"
  if (hubBaseUrl === LUASKILLS_HUB_SITE_URLS.china) return "china"
  return "custom"
}

// hubRegionPromptTarget returns the Hub site the signed-in account's region suggests, or null when the
// current Hub already matches, the user dismissed that region, or the user chose a custom Hub.
// hubRegionPromptTarget 返回已登录账号区域建议的 Hub 站点；当前 Hub 已匹配、用户已拒绝该区域或用户
// 选择了自定义 Hub 时返回 null。
export function hubRegionPromptTarget(
  network: LuaSkillsNetworkSettings,
  region: OpenVulcanRegion | null | undefined,
): OpenVulcanRegion | null {
  if (!region) return null
  const current = hubSiteOf(effectiveHubBaseUrl(network))
  if (current === "custom" || current === region) return null
  if (network.dismissedHubRegionPrompts.includes(region)) return null
  return region
}

// withNetwork returns a complete settings document with only the network block replaced.
// withNetwork 返回仅替换网络块的完整设置文档。
export function withNetwork(
  settings: LuaSkillsRuntimeSettings,
  change: Partial<LuaSkillsNetworkSettings>,
): LuaSkillsRuntimeSettings {
  return structuredClone({ ...settings, network: { ...settings.network, ...change } })
}

// LuaSkillsMirrorChoice is one selectable mirror row, built-in or user-added.
// LuaSkillsMirrorChoice 是一个可选择的镜像行，内置或用户添加。
export interface LuaSkillsMirrorChoice {
  // Stable identifier shared with probe targets and results.
  // 与测速目标及结果共用的稳定标识。
  id: string
  // Display label.
  // 显示名称。
  label: string
  // Built-in mirror metadata, null for a user-added mirror.
  // 内置镜像元数据；用户添加的镜像为 null。
  builtin: LuaSkillsGithubMirror | null
  // Download base URL, null for direct GitHub.
  // 下载基址；直连 GitHub 时为 null。
  githubBaseUrl: string | null
}

// mirrorChoices merges the built-in catalog with the user's mirrors in display order.
// mirrorChoices 按显示顺序合并内置目录与用户镜像。
export function mirrorChoices(
  builtins: readonly LuaSkillsGithubMirror[],
  customs: readonly LuaSkillsCustomGithubMirror[],
): LuaSkillsMirrorChoice[] {
  return [
    ...builtins.map((mirror) => ({
      id: mirror.id,
      label: mirror.label,
      builtin: mirror,
      githubBaseUrl: mirror.githubBaseUrl,
    })),
    ...customs.map((mirror) => ({
      id: mirror.id,
      label: mirror.label,
      builtin: null,
      githubBaseUrl: mirror.githubBaseUrl,
    })),
  ]
}

// activeMirrorId finds the mirror whose download address equals the saved one, or null for a hand-typed address.
// activeMirrorId 找出下载地址与已保存值一致的镜像；手工填写的地址返回 null。
export function activeMirrorId(
  choices: readonly LuaSkillsMirrorChoice[],
  network: LuaSkillsNetworkSettings,
): string | null {
  const download = trimmedUrl(network.githubBaseUrl)
  return choices.find((choice) => trimmedUrl(choice.githubBaseUrl) === download)?.id ?? null
}

// customMirrorId derives a fresh identifier that cannot collide with existing mirrors.
// customMirrorId 派生一个不会与现有镜像冲突的新标识。
export function customMirrorId(existing: readonly LuaSkillsMirrorChoice[]): string {
  const taken = new Set(existing.map((choice) => choice.id))
  let index = existing.filter((choice) => choice.builtin === null).length + 1
  while (taken.has(`custom-${index}`)) index += 1
  return `custom-${index}`
}
