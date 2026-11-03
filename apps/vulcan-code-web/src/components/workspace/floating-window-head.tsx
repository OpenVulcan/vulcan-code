import React from "react";
import { Maximize2, Minimize2, Minus, X } from "lucide-react";
import type { ProjectFloatingLayerControls } from "@/components/workspace/project-floating-layer";

// FloatingWindowHead renders the title row of a floating window: a glyph, the name over a quiet detail line,
// optional window-specific commands, then maximize/restore, minimize and an optional close.
// FloatingWindowHead 渲染浮动窗口的标题行：图形、名称及其下方的次要说明、可选的窗口专属命令，然后是最大化/还原、最小化与可选的关闭。
export function FloatingWindowHead(props: {
  // Leading glyph.
  // 前置图形。
  icon: React.ReactNode;
  // Window name.
  // 窗口名称。
  title: string;
  // Quiet second line under the name.
  // 名称下方的次要说明行。
  detail: React.ReactNode;
  // Full text shown when hovering the title.
  // 悬停标题时显示的完整文本。
  hoverTitle: string;
  // Window commands owned by the floating layer.
  // 浮动层持有的窗口命令。
  controls: ProjectFloatingLayerControls;
  // Accessible names of the shared window commands.
  // 共享窗口命令的无障碍名称。
  labels: { maximize: string; restore: string; minimize: string; close?: string };
  // Window-specific commands placed before maximize.
  // 放在最大化之前的窗口专属命令。
  actions?: React.ReactNode;
  // Minimizes the window into its status-bar entry.
  // 将窗口最小化到其状态栏入口。
  onMinimize: () => void;
  // Closes the window; omitted windows offer no close command.
  // 关闭窗口；省略时窗口不提供关闭命令。
  onClose?: () => void;
  // Keeps workspace input ownership while the pointer is on the title row.
  // 指针位于标题行时保留工作区输入所有权。
  preserveInputOwner?: boolean;
}) {
  const maximizeLabel = props.controls.maximized ? props.labels.restore : props.labels.maximize;
  return (
    <div className="vk-win-head" data-workspace-input-preserve-owner={props.preserveInputOwner ? "true" : undefined}>
      <span className="vk-win-title" title={props.hoverTitle}>
        {props.icon}
        <span>
          <strong>{props.title}</strong>
          <small>{props.detail}</small>
        </span>
      </span>
      <div className="vk-win-actions">
        {props.actions}
        <button
          aria-label={maximizeLabel}
          className="vk-icon-btn"
          data-size="sm"
          onClick={props.controls.toggleMaximized}
          title={maximizeLabel}
          type="button"
        >
          {props.controls.maximized ? <Minimize2 /> : <Maximize2 />}
        </button>
        <button
          aria-label={props.labels.minimize}
          className="vk-icon-btn"
          data-size="sm"
          onClick={props.onMinimize}
          title={props.labels.minimize}
          type="button"
        >
          <Minus />
        </button>
        {props.onClose ? (
          <button aria-label={props.labels.close} className="vk-icon-btn" data-size="sm" onClick={props.onClose} title={props.labels.close} type="button">
            <X />
          </button>
        ) : null}
      </div>
    </div>
  );
}
