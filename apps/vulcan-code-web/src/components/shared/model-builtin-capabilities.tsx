import * as React from "react"
import { AudioLines, Image as ImageIcon, Search, Video } from "lucide-react"

import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip"
import { useI18n, type TranslationKey } from "@/i18n"
import { cn } from "@/lib/utils"
import type { PublicAiNativeToolKind, PublicAiResourceKind } from "@/types"

import "@/components/shared/model-builtin-capabilities.css"

// ModelBuiltinCapability identifies one compact model-native capability shown in selectors.
// ModelBuiltinCapability 标识在选择器中展示的一项紧凑模型原生能力。
export type ModelBuiltinCapability =
  | "image_understanding"
  | "video_understanding"
  | "audio_understanding"
  | "web_search"

// ModelBuiltinCapabilitySource carries the exact route declarations used to derive visible icons.
// ModelBuiltinCapabilitySource 承载用于派生可见图标的精确路由声明。
export interface ModelBuiltinCapabilitySource {
  // inputResources lists non-text resources accepted by the exact model route.
  // inputResources 列出精确模型路由接受的非文本资源。
  inputResources?: readonly PublicAiResourceKind[]
  // providerNativeTools lists provider-executed tools available on the exact model route.
  // providerNativeTools 列出精确模型路由可用的供应商执行工具。
  providerNativeTools?: readonly PublicAiNativeToolKind[]
}

// MODEL_BUILTIN_CAPABILITY_ORDER keeps both selector surfaces visually stable.
// MODEL_BUILTIN_CAPABILITY_ORDER 保持两个选择器界面的视觉顺序稳定。
const MODEL_BUILTIN_CAPABILITY_ORDER: readonly ModelBuiltinCapability[] = [
  "image_understanding",
  "video_understanding",
  "audio_understanding",
  "web_search",
]

// ModelBuiltinCapabilityIconsProps configures one shared tooltip-backed capability strip.
// ModelBuiltinCapabilityIconsProps 配置一条共享且带提示框的能力图标带。
interface ModelBuiltinCapabilityIconsProps extends ModelBuiltinCapabilitySource {
  // className supplies surface-specific sizing without changing capability semantics.
  // className 提供界面特定尺寸且不改变能力语义。
  className?: string
}

// modelBuiltinCapabilities derives the four user-visible built-in capabilities from exact route data.
// modelBuiltinCapabilities 从精确路由数据派生四项用户可见的内置能力。
// Parameters: source contains input resources and provider-native tools for one exact route.
// 参数：source 包含一条精确路由的输入资源与供应商原生工具。
// Returns: a stable ordered list containing only explicitly declared capabilities.
// 返回值：仅包含明确声明能力的稳定有序列表。
export function modelBuiltinCapabilities(
  source: ModelBuiltinCapabilitySource,
): ModelBuiltinCapability[] {
  const inputResources = new Set(source.inputResources ?? [])
  const providerNativeTools = new Set(source.providerNativeTools ?? [])
  return MODEL_BUILTIN_CAPABILITY_ORDER.filter((capability) => {
    switch (capability) {
      case "image_understanding":
        return inputResources.has("image")
      case "video_understanding":
        return inputResources.has("video")
      case "audio_understanding":
        return inputResources.has("audio")
      case "web_search":
        return providerNativeTools.has("web_search")
    }
  })
}

// ModelBuiltinCapabilityIcons renders accessible capability icons shared by model-selection surfaces.
// ModelBuiltinCapabilityIcons 渲染由模型选择界面共享的可访问能力图标。
// Parameters: props contains exact route declarations and an optional surface class.
// 参数：props 包含精确路由声明与可选界面样式类。
// Returns: a compact tooltip-backed icon strip, or null when no capability is declared.
// 返回值：紧凑且带提示框的图标带；没有声明能力时返回 null。
export function ModelBuiltinCapabilityIcons(props: ModelBuiltinCapabilityIconsProps) {
  const { t } = useI18n()
  const capabilities = modelBuiltinCapabilities(props)
  if (capabilities.length === 0) {
    return null
  }
  return (
    <span className={cn("model-builtin-capabilities", props.className)}>
      {capabilities.map((capability) => {
        const label = t(modelBuiltinCapabilityLabelKey(capability))
        return (
          <Tooltip key={capability}>
            <TooltipTrigger asChild>
              <span
                aria-label={label}
                className="model-builtin-capability-icon"
                data-capability={capability}
                role="img"
              >
                {modelBuiltinCapabilityIcon(capability)}
              </span>
            </TooltipTrigger>
            <TooltipContent side="top" sideOffset={5}>{label}</TooltipContent>
          </Tooltip>
        )
      })}
    </span>
  )
}

// modelBuiltinCapabilityLabelKey maps one closed capability to an existing localized label.
// modelBuiltinCapabilityLabelKey 将一项封闭能力映射到已有的本地化标签。
// Parameters: capability is the exact icon semantic.
// 参数：capability 是精确的图标语义。
// Returns: the stable translation key used by settings and the composer.
// 返回值：设置界面与输入区共享的稳定翻译键。
function modelBuiltinCapabilityLabelKey(capability: ModelBuiltinCapability): TranslationKey {
  switch (capability) {
    case "image_understanding":
      return "settings.aiUnderstandImage"
    case "video_understanding":
      return "settings.aiUnderstandVideo"
    case "audio_understanding":
      return "settings.aiUnderstandAudio"
    case "web_search":
      return "settings.aiWebSearch"
  }
}

// modelBuiltinCapabilityIcon maps one closed capability to its compact visual symbol.
// modelBuiltinCapabilityIcon 将一项封闭能力映射为紧凑视觉符号。
// Parameters: capability is the exact icon semantic.
// 参数：capability 是精确的图标语义。
// Returns: a decorative SVG whose accessible name is owned by its wrapper.
// 返回值：由外层包装元素持有可访问名称的装饰性 SVG。
function modelBuiltinCapabilityIcon(capability: ModelBuiltinCapability): React.ReactNode {
  switch (capability) {
    case "image_understanding":
      return <ImageIcon aria-hidden="true" />
    case "video_understanding":
      return <Video aria-hidden="true" />
    case "audio_understanding":
      return <AudioLines aria-hidden="true" />
    case "web_search":
      return <Search aria-hidden="true" />
  }
}
