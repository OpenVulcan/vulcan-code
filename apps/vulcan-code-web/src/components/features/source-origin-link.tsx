import * as React from "react"
import { Bot, ExternalLink, Github } from "lucide-react"
import { toast } from "sonner"

import { openExternalUrl } from "@/service/project-actions-service"
import { useI18n } from "@/i18n"

// SourceOriginLinkMetadata defines the shared provenance fields required by source links.
// SourceOriginLinkMetadata 定义来源链接所需的共享溯源字段。
export interface SourceOriginLinkMetadata {
  // Optional display icon key supplied by the source descriptor.
  // 来源描述符提供的可选展示图标键。
  icon?: string | null
  // Stable source kind used to select the icon and label.
  // 用于选择图标与标签的稳定来源类型。
  kind?: string | null
  // Optional external source URL opened by the host shell.
  // 由宿主壳打开的可选外部来源地址。
  url?: string | null
}

// SourceOriginLinkProps defines the provenance link presentation inputs.
// SourceOriginLinkProps 定义来源链接的展示输入。
interface SourceOriginLinkProps {
  // Whether the link is rendered inside a detail dialog.
  // 链接是否渲染在详情弹窗内。
  detail?: boolean
  // Display-only source metadata.
  // 仅用于展示的来源元数据。
  origin?: SourceOriginLinkMetadata | null
}

// SourceOriginLink renders a shared provenance link and opens its URL through the desktop host.
// SourceOriginLink 渲染共享来源链接，并通过桌面宿主打开其地址。
export function SourceOriginLink(props: SourceOriginLinkProps) {
  // t keeps shell failure feedback in the currently selected application language.
  // t 让宿主失败反馈使用当前选中的应用语言。
  const { t } = useI18n()
  // origin stores the source descriptor selected by the caller.
  // origin 保存调用方选择的来源描述符。
  const origin = props.origin
  // url stores the normalized external address declared by the source descriptor.
  // url 保存来源描述符声明的规范化外部地址。
  const url = origin?.url?.trim()
  if (!origin || !url) {
    return null
  }
  // iconKey stores the normalized icon discriminator.
  // iconKey 保存规范化后的图标判别键。
  const iconKey = sourceOriginIconKey(origin)
  // label stores the compact source text shown beside the icon.
  // label 保存显示在图标旁的紧凑来源文本。
  const label = iconKey === "github" ? "GitHub" : origin.kind?.trim() || (t("features.sourceOrigin.source"))
  // className selects the compact row style or expanded detail style.
  // className 选择紧凑行样式或展开详情样式。
  const className = props.detail ? "agent-source-origin-detail-link" : "agent-source-origin-link"
  // handleClick opens the source URL through the same host shell path used by repository links.
  // handleClick 通过仓库链接共用的宿主壳路径打开来源地址。
  const handleClick = (event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault()
    event.stopPropagation()
    void openExternalUrl(url).catch((error) => {
      toast.error(`${t("features.sourceOrigin.couldNotOpenSource")}${error instanceof Error ? error.message : String(error)}`)
    })
  }
  return (
    <a aria-label={url} className={className} href={url} onClick={handleClick} rel="noreferrer" target="_blank" title={url}>
      {sourceOriginIcon(origin)}
      <span>{label}</span>
    </a>
  )
}

// hasLinkedGitHubSourceOrigin reports whether source metadata declares a usable GitHub repository URL.
// hasLinkedGitHubSourceOrigin 判断来源元数据是否声明了可用的 GitHub 仓库地址。
export function hasLinkedGitHubSourceOrigin(origin: SourceOriginLinkMetadata): boolean {
  // kind stores the normalized provenance kind.
  // kind 保存规范化后的来源类型。
  const kind = origin.kind?.trim().toLowerCase() ?? ""
  // url stores the normalized optional provenance URL.
  // url 保存规范化后的可选来源地址。
  const url = origin.url?.trim() ?? ""
  return kind === "github" && url.length > 0
}

// sourceOriginIconKey normalizes the source icon key for deterministic rendering.
// sourceOriginIconKey 归一化来源图标键，以便确定性渲染。
function sourceOriginIconKey(origin: SourceOriginLinkMetadata): string {
  // declaredIconKey stores the optional explicit icon selected by the source descriptor.
  // declaredIconKey 保存来源描述符选择的可选显式图标。
  const declaredIconKey = origin.icon?.trim().toLowerCase()
  if (declaredIconKey) {
    return declaredIconKey
  }
  return origin.kind?.trim().toLowerCase() ?? ""
}

// sourceOriginIcon returns the compact icon associated with one provenance descriptor.
// sourceOriginIcon 返回与单个来源描述符关联的紧凑图标。
function sourceOriginIcon(origin: SourceOriginLinkMetadata): React.ReactNode {
  // iconKey stores the normalized icon discriminator used by this render.
  // iconKey 保存本次渲染使用的规范化图标判别键。
  const iconKey = sourceOriginIconKey(origin)
  if (iconKey === "github") {
    return <Github size={13} />
  }
  if (iconKey === "system") {
    return <Bot size={13} />
  }
  return <ExternalLink size={13} />
}
