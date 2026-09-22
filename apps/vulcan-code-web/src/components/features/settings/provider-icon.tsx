import type { ReactElement } from "react"

import antigravityIcon from "@/assets/provider-icons/provider-antigravity.svg"
import agentSpaceIcon from "@/assets/provider-icons/provider-agentspace.png"
import apiKeyFunIcon from "@/assets/provider-icons/provider-apikeyfan.svg"
import claudeIcon from "@/assets/provider-icons/provider-claude.svg"
import claudeApiIcon from "@/assets/provider-icons/provider-claudeapi.svg"
import code0Icon from "@/assets/provider-icons/provider-code0.svg"
import codexIcon from "@/assets/provider-icons/provider-codex.svg"
import commandCodeIcon from "@/assets/provider-icons/provider-commandcode.svg"
import deepSeekIcon from "@/assets/provider-icons/provider-deepseek.svg"
import fennoAiIcon from "@/assets/provider-icons/provider-fennoai.svg"
import geminiIcon from "@/assets/provider-icons/provider-gemini.svg"
import grokDarkIcon from "@/assets/provider-icons/provider-grok-dark.svg"
import grokLightIcon from "@/assets/provider-icons/provider-grok-light.svg"
import kimiDarkIcon from "@/assets/provider-icons/provider-kimi-dark.svg"
import kimiLightIcon from "@/assets/provider-icons/provider-kimi-light.svg"
import miniMaxIcon from "@/assets/provider-icons/provider-minimax.svg"
import openAICompatibilityDarkIcon from "@/assets/provider-icons/provider-openai-compat-dark.svg"
import openAICompatibilityLightIcon from "@/assets/provider-icons/provider-openai-compat-light.svg"
import openCodeGoIcon from "@/assets/provider-icons/provider-opencode-go.svg"
import openRouterIcon from "@/assets/provider-icons/provider-openrouter.svg"
import qiniuIcon from "@/assets/provider-icons/provider-qiniu.svg"
import qwenIcon from "@/assets/provider-icons/provider-qwen.svg"
import stepFunIcon from "@/assets/provider-icons/provider-stepfun.svg"
import tavilyIcon from "@/assets/provider-icons/provider-tavily.svg"
import tokenRhythmIcon from "@/assets/provider-icons/provider-tokenrhythm.svg"
import vertexIcon from "@/assets/provider-icons/provider-vertex.svg"
import zhipuIcon from "@/assets/provider-icons/provider-zhipu.svg"
import { cn } from "@/lib/utils"
import type { PublicAiProviderCategory } from "@/types"

// ProviderIconKey is the closed frontend-owned provider artwork set.
// ProviderIconKey 是由前端持有的供应商图标闭集。
export type ProviderIconKey =
  | "antigravity"
  | "agentspace"
  | "apikeyfan"
  | "claude"
  | "claudeapi"
  | "code0"
  | "codex"
  | "commandcode"
  | "compatibility"
  | "deepseek"
  | "fennoai"
  | "gemini"
  | "grok"
  | "kimi"
  | "minimax"
  | "opencode-go"
  | "openrouter"
  | "qwen"
  | "qiniu"
  | "stepfun"
  | "tavily"
  | "tokenrhythm"
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
  // Whether dark theme must invert one externally loaded monochrome SVG.
  // 暗色主题是否需要反转一个通过外部资源加载的单色 SVG。
  monochrome?: boolean
}

// providerIconLibrary is the immutable frontend artwork library.
// providerIconLibrary 是不可变的前端图标库。
const providerIconLibrary: Readonly<Record<ProviderIconKey, ProviderIconAsset>> = {
  antigravity: { light: antigravityIcon },
  agentspace: { light: agentSpaceIcon, transparent: true },
  apikeyfan: { light: apiKeyFunIcon, transparent: true },
  claude: { light: claudeIcon },
  claudeapi: { light: claudeApiIcon, transparent: true },
  code0: { light: code0Icon, transparent: true },
  codex: { light: codexIcon },
  commandcode: { light: commandCodeIcon, monochrome: true, transparent: true },
  compatibility: {
    light: openAICompatibilityLightIcon,
    dark: openAICompatibilityDarkIcon,
    transparent: true,
  },
  deepseek: { light: deepSeekIcon },
  fennoai: { light: fennoAiIcon, transparent: true },
  gemini: { light: geminiIcon },
  grok: { light: grokLightIcon, dark: grokDarkIcon, transparent: true },
  kimi: {
    light: kimiLightIcon,
    dark: kimiDarkIcon,
    themeSurface: true,
    transparent: true,
  },
  minimax: { light: miniMaxIcon },
  "opencode-go": { light: openCodeGoIcon, monochrome: true, transparent: true },
  openrouter: { light: openRouterIcon, transparent: true },
  qwen: { light: qwenIcon },
  qiniu: { light: qiniuIcon, transparent: true },
  stepfun: { light: stepFunIcon, themeSurface: true, transparent: true, monochrome: true },
  tavily: { light: tavilyIcon, monochrome: true, transparent: true },
  tokenrhythm: { light: tokenRhythmIcon, monochrome: true, transparent: true },
  vertex: { light: vertexIcon },
  zhipu: { light: zhipuIcon },
}

// providerIconMap binds stable provider identifiers to family artwork.
// providerIconMap 将稳定供应商标识绑定到系列图标。
const providerIconMap: Readonly<Record<string, ProviderIconKey>> = {
  alibaba: "qwen",
  apikeyfan: "apikeyfan",
  anthropic: "claude",
  agentspace: "agentspace",
  claude: "claude",
  claudeapi: "claudeapi",
  code0: "code0",
  commandcode: "commandcode",
  deepseek: "deepseek",
  fennoai: "fennoai",
  google: "gemini",
  kimi: "kimi",
  minimax: "minimax",
  openai: "compatibility",
  "opencode-go": "opencode-go",
  openrouter: "openrouter",
  qwen: "qwen",
  qiniu: "qiniu",
  stepfun: "stepfun",
  tavily: "tavily",
  tokenrhythm: "tokenrhythm",
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
  // Provider category controls the independent relay marker overlay.
  // 供应商分类控制独立的中转站标识叠层。
  category?: PublicAiProviderCategory
}

// ProviderIcon renders theme-aware provider artwork without exposing it as semantic content.
// ProviderIcon 渲染主题感知供应商图标，且不将其暴露为语义内容。
export function ProviderIcon({
  providerId,
  serviceEntryId,
  className,
  category,
}: ProviderIconProps): ReactElement {
  // iconKey is resolved only from stable provider identities.
  // iconKey 仅根据稳定供应商标识解析。
  const iconKey = resolveProviderIconKey({ providerId, serviceEntryId })
  // asset is the complete theme-aware artwork entry.
  // asset 是完整的主题感知图标项。
  const asset = providerIconLibrary[iconKey]

  return (
    <span
      aria-hidden={category === "relay" ? undefined : "true"}
      aria-label={category === "relay" ? "Relay provider / 中转站" : undefined}
      className={cn(
        "relative inline-flex size-[26px] shrink-0 items-center justify-center rounded-md bg-muted p-0.5",
        asset.transparent ? "bg-transparent" : "",
        asset.themeSurface ? "box-border rounded-md bg-black p-[5px] dark:bg-white" : "",
        className,
      )}
      data-provider-category={category}
      data-provider-icon={iconKey}
      role={category === "relay" ? "img" : undefined}
      title={category === "relay" ? "Relay provider / 中转站" : undefined}
    >
      <img
        alt=""
        className={cn(
          "size-full object-contain",
          asset.dark ? "dark:hidden" : "",
          asset.monochrome ? "dark:invert" : "",
        )}
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
      {category === "relay" ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute -bottom-1 -right-1 inline-flex size-3.5 items-center justify-center rounded-full border border-background bg-[var(--vcn-status-warning-background)] text-[8px] font-bold leading-none text-[var(--vcn-content-warning)] shadow-sm"
        >
          R
        </span>
      ) : null}
    </span>
  )
}
