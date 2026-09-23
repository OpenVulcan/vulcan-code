import type { ReactNode } from "react"

import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"

// SettingsEmptyState renders the one settings empty-state pattern: icon, one-line message, optional next action.
// SettingsEmptyState 渲染设置页统一空状态：图标、一句说明与可选的下一步操作。
// Parameters: icon marks the missing content, title states it, description explains how to leave it, action offers the next step.
// 参数：icon 标识缺失内容，title 说明现状，description 解释如何离开空状态，action 提供下一步操作。
export function SettingsEmptyState(props: {
  // Lucide icon shown in the muted media tile.
  // 显示在弱化媒体底块中的 Lucide 图标。
  icon: ReactNode
  // Short statement of what is missing.
  // 对缺失内容的简短陈述。
  title: ReactNode
  // Optional guidance below the title.
  // 标题下方的可选引导说明。
  description?: ReactNode
  // Optional primary or recovery action.
  // 可选的主操作或恢复操作。
  action?: ReactNode
}) {
  return (
    <Empty className="settings-empty-state" role="status" variant="outlined">
      <EmptyHeader>
        <EmptyMedia variant="icon">{props.icon}</EmptyMedia>
        <EmptyTitle>{props.title}</EmptyTitle>
        {props.description ? <EmptyDescription>{props.description}</EmptyDescription> : null}
      </EmptyHeader>
      {props.action ? <EmptyContent>{props.action}</EmptyContent> : null}
    </Empty>
  )
}
