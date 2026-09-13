import * as React from "react"
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  AudioLines,
  Binary,
  File,
  Image as ImageIcon,
  ListOrdered,
  Type,
  Video,
} from "lucide-react"

import {
  providerModelModalities,
  type ProviderModelModality,
} from "@/components/features/settings/provider-model-modalities"
import {
  ModelBuiltinCapabilityIcons,
  modelBuiltinCapabilities,
} from "@/components/shared/model-builtin-capabilities"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import type { PublicAiModel } from "@/types"

// ProviderModelModalitySummaryProps identifies the exact model whose modalities are summarized.
// ProviderModelModalitySummaryProps 标识要汇总多模态能力的精确模型。
interface ProviderModelModalitySummaryProps {
  // model contains the normalized capability contract.
  // model 包含归一化能力契约。
  model: PublicAiModel
  // additionalModels contributes declared modalities from other executable routes of the same model.
  // additionalModels 补充同一模型其他可执行路由声明的模态。
  additionalModels?: PublicAiModel[]
}

// ProviderModelModalitySummary renders compact tooltip-backed modality icons shared by settings surfaces.
// ProviderModelModalitySummary 渲染设置界面共享的紧凑且带提示的多模态图标。
export function ProviderModelModalitySummary(props: ProviderModelModalitySummaryProps) {
  const { t } = useI18n()
  // models preserves all executable route declarations when the caller renders a grouped model row.
  // models 在调用方渲染分组模型行时保留全部可执行路由声明。
  const models = [props.model, ...(props.additionalModels ?? [])]
  // routeModalities supplies the existing normalized input/output facts for each route.
  // routeModalities 提供每条路由既有的归一化输入输出事实。
  const routeModalities = models.map(providerModelModalities)
  // modalities merges exact declared kinds without repeating icons for equivalent routes.
  // modalities 合并精确声明的类型，避免等价路由重复显示图标。
  const modalities = {
    input: [...new Set(routeModalities.flatMap((route) => route.input))],
    output: [...new Set(routeModalities.flatMap((route) => route.output))],
  }
  // inputResources merges explicit media declarations across the displayed routes.
  // inputResources 合并所展示路由的明确媒体声明。
  const inputResources = [...new Set(models.flatMap((model) => model.capabilities.input_resources ?? []))]
  // genericInputModalities excludes media inputs already represented as explicit understanding capabilities.
  // genericInputModalities 排除已经由明确理解能力表示的媒体输入。
  const genericInputModalities = modalities.input.filter((modality) => (
    modality !== "image" && modality !== "video" && modality !== "audio"
  ))
  // providerNativeTools retains native capabilities from every displayed route.
  // providerNativeTools 保留每条展示路由的原生能力。
  const providerNativeTools = [...new Set(models.flatMap((model) => model.capabilities.provider_native_tools ?? []))]
  // builtinCapabilities prevents media-only routes from being hidden by the generic-modality filter.
  // builtinCapabilities 防止仅媒体路由被通用模态过滤器隐藏。
  const builtinCapabilities = modelBuiltinCapabilities({
    inputResources,
    providerNativeTools,
  })
  if (
    genericInputModalities.length === 0
    && modalities.output.length === 0
    && builtinCapabilities.length === 0
  ) {
    return null
  }
  return (
    <span className="settings-model-modality-summary">
      <ModelBuiltinCapabilityIcons
        className="settings-model-builtin-capabilities"
        inputResources={inputResources}
        providerNativeTools={providerNativeTools}
      />
      {genericInputModalities.length > 0 ? (
        <span className="settings-model-modality-group">
          <ModalityIcon
            icon={<ArrowDownToLine aria-hidden="true" />}
            label={t("settings.models.inputModalities")}
            muted
          />
          {genericInputModalities.map((modality) => (
            <ModalityIcon
              icon={modelModalityIcon(modality)}
              key={`input/${modality}`}
              label={modelModalityLabel(modality, t)}
            />
          ))}
        </span>
      ) : null}
      {modalities.output.length > 0 ? (
        <span className="settings-model-modality-group">
          <ModalityIcon
            icon={<ArrowUpFromLine aria-hidden="true" />}
            label={t("settings.models.outputModalities")}
            muted
          />
          {modalities.output.map((modality) => (
            <ModalityIcon
              icon={modelModalityIcon(modality)}
              key={`output/${modality}`}
              label={modelModalityLabel(modality, t)}
            />
          ))}
        </span>
      ) : null}
    </span>
  )
}

// ModalityIconProps defines one accessible compact modality symbol.
// ModalityIconProps 定义一个可访问的紧凑多模态符号。
interface ModalityIconProps {
  // icon is the visual modality glyph.
  // icon 是多模态视觉图形。
  icon: React.ReactNode
  // label is exposed to assistive technology and the tooltip.
  // label 向辅助技术与提示框公开。
  label: string
  // muted distinguishes direction markers from resource types.
  // muted 用于区分方向标记与资源类型。
  muted?: boolean
}

// ModalityIcon renders one accessible icon with its full meaning available on hover.
// ModalityIcon 渲染一个可访问图标，并在悬停时提供完整含义。
function ModalityIcon(props: ModalityIconProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          aria-label={props.label}
          className={`settings-model-modality-icon ${props.muted ? "muted" : ""}`}
          role="img"
        >
          {props.icon}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top" sideOffset={5}>{props.label}</TooltipContent>
    </Tooltip>
  )
}

// modelModalityIcon maps one closed modality identifier to its compact visual symbol.
// modelModalityIcon 将一个封闭多模态标识映射为紧凑视觉符号。
function modelModalityIcon(modality: ProviderModelModality): React.ReactNode {
  switch (modality) {
    case "text":
      return <Type aria-hidden="true" />
    case "image":
      return <ImageIcon aria-hidden="true" />
    case "audio":
      return <AudioLines aria-hidden="true" />
    case "video":
      return <Video aria-hidden="true" />
    case "file":
      return <File aria-hidden="true" />
    case "embedding":
      return <Binary aria-hidden="true" />
    case "rerank":
      return <ListOrdered aria-hidden="true" />
  }
}

// modelModalityLabel maps one closed modality identifier to its localized label.
// modelModalityLabel 将一个封闭多模态标识映射为本地化标签。
// Parameters: modality is a declared closed identifier and t resolves locale text; returns the corresponding human-readable resource label.
// 参数：modality 是已声明封闭标识，t 解析本地化文案；返回对应的可读资源标签。
export function modelModalityLabel(
  modality: ProviderModelModality,
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (modality) {
    case "text":
      return t("settings.models.modality.text")
    case "image":
      return t("settings.models.modality.image")
    case "audio":
      return t("settings.models.modality.audio")
    case "video":
      return t("settings.models.modality.video")
    case "file":
      return t("settings.models.modality.file")
    case "embedding":
      return t("settings.models.modality.embedding")
    case "rerank":
      return t("settings.models.modality.rerank")
  }
}
