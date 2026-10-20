import * as React from "react"
import { CheckCircle2, CircleAlert, Clock3 } from "lucide-react"

import { Badge } from "@/ui-kit/primitives/badge"
import { cx } from "@/ui-kit/behavior/core";
import type { StartupStepStatus } from "@/types"

// PanelTitle renders a section title with an icon.
// PanelTitle 渲染带图标的区域标题。
function PanelTitle(props: {
  // Icon rendered before the title.
  // 渲染在标题前的图标。
  icon: React.ReactNode
  // Section title text.
  // 区域标题文本。
  title: string
}) {
  return (
    <div className="panel-title">
      {props.icon}
      <h2>{props.title}</h2>
    </div>
  )
}

// StatusIcon renders an icon for a startup status.
// StatusIcon 渲染启动状态图标。
function StatusIcon(props: {
  // Startup status that selects the icon style.
  // 用于选择图标样式的启动状态。
  status: StartupStepStatus
}) {
  if (props.status === "ready") {
    return <CheckCircle2 className="status-icon ready" size={18} />
  }
  if (props.status === "warning" || props.status === "error") {
    return <CircleAlert className={`status-icon ${props.status}`} size={18} />
  }
  return <Clock3 className="status-icon pending" size={18} />
}

// StatusPill renders a compact text status badge.
// StatusPill 渲染紧凑文本状态徽标。
function StatusPill(props: {
  // Status kind used by CSS tokens.
  // CSS token 使用的状态类型。
  status: StartupStepStatus | string
  // Visible status label.
  // 可见状态标签。
  label: string
}) {
  // variant maps runtime status meanings onto the shared badge vocabulary.
  // variant 将运行时状态含义映射到共享徽标语汇。
  const variant = props.status === "ready"
    ? "success"
    : props.status === "error"
      ? "destructive"
      : props.status === "warning"
        ? "warning"
        : "outline"

  return <Badge className={cx("status-pill", props.status)} variant={variant}>{props.label}</Badge>
}

export { PanelTitle, StatusIcon, StatusPill }
