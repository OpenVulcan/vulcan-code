import * as React from "react"
import {
  FolderTree,
  History,
} from "lucide-react"

import { RuntimeModulesPanel } from "@/components/features/settings/settings-runtime-panels"
import { PanelTitle, StatusIcon, StatusPill } from "@/components/shared/status"
import { useI18n } from "@/i18n"
import type {
  LogEntrySummary,
  McpServerSummary,
  RuntimeModuleSnapshot,
  StartupSnapshot,
} from "@/types"
import { McpServerDialog, type McpSettingsActionProps } from "./mcp-server-dialog"

// McpSettingsArea renders MCP runtime status using startup and diagnostic snapshots.
// McpSettingsArea 使用启动与诊断快照渲染 MCP 运行态。
export function McpSettingsArea(props: {
  // Service-backed MCP settings actions.
  // 服务端支持的 MCP 设置动作。
  actions: McpSettingsActionProps
  // Diagnostic log entries.
  // 诊断日志条目。
  logs: LogEntrySummary[]
  // Runtime modules used to locate MCP status.
  // 用于定位 MCP 状态的运行时模块。
  modules: RuntimeModuleSnapshot[]
  // Persisted MCP servers.
  // 持久化 MCP 服务器。
  servers: McpServerSummary[]
  // Startup snapshot used to locate MCP startup steps.
  // 用于定位 MCP 启动步骤的启动快照。
  startup: StartupSnapshot
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [editingServer, setEditingServer] = React.useState<McpServerSummary | null>(null)
  const [creatingServer, setCreatingServer] = React.useState(false)
  const mcpModules = props.modules.filter((module) => module.id.toLowerCase().includes("mcp"))
  const mcpSteps = props.startup.steps.filter((step) => step.id.toLowerCase().includes("mcp") || step.label.toLowerCase().includes("mcp"))
  const mcpLogs = props.logs.filter((log) => log.source.toLowerCase().includes("mcp") || log.message.toLowerCase().includes("mcp"))
  return (
    <div className="area-grid two-columns">
      <section className="panel settings-detail-panel">
        <div className="settings-panel-title-row">
          <PanelTitle icon={<FolderTree size={17} />} title={text("MCP 服务器", "MCP servers")} />
          <button className="skill-action-button" onClick={() => setCreatingServer(true)} type="button">{text("添加 MCP", "Add MCP")}</button>
        </div>
        <div className="settings-provider-list">
          {props.servers.length === 0 ? (
            <p className="settings-muted-line">{text("当前没有 MCP 服务器配置。", "No MCP servers are configured.")}</p>
          ) : props.servers.map((server) => (
            <article className="settings-provider-row" key={server.id}>
              <header>
                <div>
                  <strong>{server.name}</strong>
                  <span>{server.id} · {server.transport}</span>
                </div>
                <div className="chip-row">
                  <StatusPill status={server.enabled ? "ready" : "pending"} label={server.enabled ? text("启用", "Enabled") : text("停用", "Disabled")} />
                  <button className="skill-action-button secondary" onClick={() => setEditingServer(server)} type="button">{text("编辑", "Edit")}</button>
                  <button className="skill-action-button secondary danger" onClick={() => void props.actions.onRemoveMcpServer(server.id)} type="button">{text("删除", "Delete")}</button>
                </div>
              </header>
              <span className="settings-muted-line">{server.transport === "stdio" ? server.command : server.url}</span>
            </article>
          ))}
        </div>
        <McpServerDialog
          actions={props.actions}
          onOpenChange={(open) => {
            if (!open) {
              setCreatingServer(false)
              setEditingServer(null)
            }
          }}
          open={creatingServer || editingServer !== null}
          server={editingServer}
        />
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<History size={17} />} title={text("MCP 状态", "MCP status")} />
        <RuntimeModulesPanel modules={mcpModules.length ? mcpModules : props.modules.filter((module) => module.id === "core")} />
        <div className="step-list">
          {mcpSteps.map((step) => (
            <div className="step-item" key={step.id}>
              <StatusIcon status={step.status} />
              <div>
                <strong>{step.label}</strong>
                <span>{step.message ?? step.id}</span>
              </div>
            </div>
          ))}
        </div>
        <div className="log-list">
          {mcpLogs.length === 0 ? (
            <p className="settings-muted-line">{text("当前没有 MCP 相关诊断日志。", "No MCP diagnostic logs are available.")}</p>
          ) : mcpLogs.map((log) => (
            <article className="log-row" key={log.id}>
              <StatusPill status={log.level === "info" ? "ready" : "warning"} label={log.level} />
              <strong>{log.source}</strong>
              <span>{log.message}</span>
            </article>
          ))}
        </div>
      </section>
    </div>
  )
}

// localizedLiteral picks the display literal that matches the active locale.
// localizedLiteral 按当前语言选择展示文本。
function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}
