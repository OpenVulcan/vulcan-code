import type { ReactElement } from "react"

import antigravityIcon from "@/assets/provider-icons/provider-antigravity.svg"
import claudeIcon from "@/assets/provider-icons/provider-claude.svg"
import codexIcon from "@/assets/provider-icons/provider-codex.svg"
import deepSeekIcon from "@/assets/provider-icons/provider-deepseek.svg"
import geminiIcon from "@/assets/provider-icons/provider-gemini.svg"
import grokDarkIcon from "@/assets/provider-icons/provider-grok-dark.svg"
import grokLightIcon from "@/assets/provider-icons/provider-grok-light.svg"
import kimiDarkIcon from "@/assets/provider-icons/provider-kimi-dark.svg"
import kimiLightIcon from "@/assets/provider-icons/provider-kimi-light.svg"
import miniMaxIcon from "@/assets/provider-icons/provider-minimax.svg"
import openAICompatibilityDarkIcon from "@/assets/provider-icons/provider-openai-compat-dark.svg"
import openAICompatibilityLightIcon from "@/assets/provider-icons/provider-openai-compat-light.svg"
import openRouterIcon from "@/assets/provider-icons/provider-openrouter.svg"
import qwenIcon from "@/assets/provider-icons/provider-qwen.svg"
import tavilyIcon from "@/assets/provider-icons/provider-tavily.svg"
import vertexIcon from "@/assets/provider-icons/provider-vertex.svg"
import zhipuIcon from "@/assets/provider-icons/provider-zhipu.svg"
import { cn } from "@/lib/utils"

// ProviderIconKey is the closed frontend-owned provider artwork set.
// ProviderIconKey 是由前端持有的供应商图标闭集。
export type ProviderIconKey =
  | "antigravity"
  | "claude"
  | "codex"
  | "compatibility"
  | "deepseek"
  | "gemini"
  | "grok"
  | "kimi"
  | "minimax"
  | "openrouter"
  | "qwen"
  | "tavily"
  | "vertex"
  | "zhipu"

// ProviderIconAsset stores theme-aware rendering metadata for one artwork entry.
// ProviderIconAsset 保存一个图标项的主题感知渲染元数据。
interface ProviderIconAsset {
  // Light-theme artwork.
  // 亮色主题图标。
  light: string
  // Optional dark-theme artwork.
  // 可选暗色主题图标。
  dark?: string
  // Whether the authored icon requires a contrasting framed surface.
  // 原始图标是否需要对比色外框底板。
  themeSurface?: boolean
  // Whether the standard muted tile must be removed.
  // 是否移除标准弱化底板。
  transparent?: boolean
}

// providerIconLibrary is the immutable frontend artwork library.
// providerIconLibrary 是不可变的前端图标库。
const providerIconLibrary: Readonly<Record<ProviderIconKey, ProviderIconAsset>> = {
  antigravity: { light: antigravityIcon },
  claude: { light: claudeIcon },
  codex: { light: codexIcon },
  compatibility: {
    light: openAICompatibilityLightIcon,
    dark: openAICompatibilityDarkIcon,
    transparent: true,
  },
  deepseek: { light: deepSeekIcon },
  gemini: { light: geminiIcon },
  grok: { light: grokLightIcon, dark: grokDarkIcon, transparent: true },
  kimi: {
    light: kimiLightIcon,
    dark: kimiDarkIcon,
    themeSurface: true,
    transparent: true,
  },
  minimax: { light: miniMaxIcon },
  openrouter: { light: openRouterIcon, transparent: true },
  qwen: { light: qwenIcon },
  tavily: { light: tavilyIcon, transparent: true },
  vertex: { light: vertexIcon },
  zhipu: { light: zhipuIcon },
}

// providerIconMap binds stable provider identifiers to family artwork.
// providerIconMap 将稳定供应商标识绑定到系列图标。
const providerIconMap: Readonly<Record<string, ProviderIconKey>> = {
  alibaba: "qwen",
  anthropic: "claude",
  claude: "claude",
  deepseek: "deepseek",
  google: "gemini",
  kimi: "kimi",
  minimax: "minimax",
  openai: "compatibility",
  openrouter: "openrouter",
  qwen: "qwen",
  tavily: "tavily",
  vertex: "vertex",
  xai: "grok",
  zhipu: "zhipu",
}

// serviceEntryIconMap overrides family artwork for distinct provider products.
// serviceEntryIconMap 为具有独立品牌图标的供应商产品覆盖系列图标。
const serviceEntryIconMap: Readonly<Record<string, ProviderIconKey>> = {
  "google/antigravity": "antigravity",
  "google/vertex": "vertex",
  "openai/codex-subscription": "codex",
}

// ProviderIconIdentity contains stable runtime identifiers used only for frontend artwork lookup.
// ProviderIconIdentity 包含仅用于前端图标查询的稳定运行时标识。
export interface ProviderIconIdentity {
  // Stable provider identifier.
  // 稳定供应商标识。
  providerId?: string
  // Optional exact service-entry identifier.
  // 可选的精确服务入口标识。
  serviceEntryId?: string
}

// resolveProviderIconKey resolves product artwork first, family artwork second, and compatibility artwork last.
// resolveProviderIconKey 依次解析产品图标、系列图标，最后回退到兼容图标。
export function resolveProviderIconKey({
  providerId,
  serviceEntryId,
}: ProviderIconIdentity): ProviderIconKey {
  // productKey is the collision-free provider/service-entry identity.
  // productKey 是无冲突的供应商与服务入口组合标识。
  const productKey = providerId && serviceEntryId ? `${providerId}/${serviceEntryId}` : ""
  if (productKey && serviceEntryIconMap[productKey]) {
    return serviceEntryIconMap[productKey]
  }
  if (providerId && providerIconMap[providerId]) {
    return providerIconMap[providerId]
  }
  return "compatibility"
}

// ProviderIconProps configures one decorative provider logo.
// ProviderIconProps 配置一个装饰性供应商 Logo。
interface ProviderIconProps extends ProviderIconIdentity {
  // Caller-owned square display-box class.
  // 调用方控制的方形展示框样式。
  className?: string
}

// ProviderIcon renders theme-aware provider artwork without exposing it as semantic content.
// ProviderIcon 渲染主题感知供应商图标，且不将其暴露为语义内容。
export function ProviderIcon({
  providerId,
  serviceEntryId,
  className,
}: ProviderIconProps): ReactElement {
  // iconKey is resolved only from stable provider identities.
  // iconKey 仅根据稳定供应商标识解析。
  const iconKey = resolveProviderIconKey({ providerId, serviceEntryId })
  // asset is the complete theme-aware artwork entry.
  // asset 是完整的主题感知图标项。
  const asset = providerIconLibrary[iconKey]

  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex size-[26px] shrink-0 items-center justify-center rounded-md bg-muted p-0.5",
        asset.transparent ? "bg-transparent" : "",
        asset.themeSurface ? "box-border rounded-md bg-black p-[5px] dark:bg-white" : "",
        className,
      )}
      data-provider-icon={iconKey}
    >
      <img
        alt=""
        className={cn("size-full object-contain", asset.dark ? "dark:hidden" : "")}
        draggable={false}
        src={asset.light}
      />
      {asset.dark ? (
        <img
          alt=""
          className="hidden size-full object-contain dark:block"
          draggable={false}
          src={asset.dark}
        />
      ) : null}
    </span>
  )
}
