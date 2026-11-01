import * as React from "react"
import { PageBody, PageFacts, PageHint, PageNotice, PageSection } from "@/components/page"
import { formatMessage, useI18n } from "@/i18n"
import { errorMessage, formatBytes } from "@/lib/display-format"
import { luaSkillsDebugInstallStatus, readLuaSkillsDebugStatus, startLuaSkillsDebugInstall } from "@/service/skills-actions-service"
import type { LuaSkillsDebugInstallTask, LuaSkillsDebugStatus } from "@/types"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import { Progress } from "@/ui-kit/primitives/progress"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"

// INSTALL_POLL_INTERVAL_MS paces install-task queries while an install runs.
// INSTALL_POLL_INTERVAL_MS 控制安装运行期间的任务查询节奏。
const INSTALL_POLL_INTERVAL_MS = 1000

// INSTALL_PHASE_LABELS maps the installer phase keys to their catalog labels.
// INSTALL_PHASE_LABELS 将安装阶段键映射到其目录标签。
const INSTALL_PHASE_LABELS = {
  queued: "skills.luaDebug.phase.queued",
  resolving: "skills.luaDebug.phase.resolving",
  downloading: "skills.luaDebug.phase.downloading",
  verifying: "skills.luaDebug.phase.verifying",
  extracting: "skills.luaDebug.phase.extracting",
  completed: "skills.luaDebug.phase.completed",
} as const

// LuaSkillsDebugPanel installs luaskills-debug in one click and shows where it lives. The install is a
// Core-owned task queried by its stable id, so leaving and reopening the page keeps following it.
// LuaSkillsDebugPanel 一键安装 luaskills-debug 并显示其位置。安装是 Core 自有任务并按稳定标识查询，
// 因此离开后重新打开页面仍会继续跟踪。
export function LuaSkillsDebugPanel() {
  const { t } = useI18n()
  const [status, setStatus] = React.useState<LuaSkillsDebugStatus | null>(null)
  const [task, setTask] = React.useState<LuaSkillsDebugInstallTask | null>(null)
  const [loadError, setLoadError] = React.useState<string | null>(null)
  const [starting, setStarting] = React.useState(false)
  const load = React.useCallback(async () => {
    setLoadError(null)
    try {
      const loaded = await readLuaSkillsDebugStatus()
      setStatus(loaded)
      setTask(loaded.task)
    } catch (error) {
      setLoadError(errorMessage(error))
    }
  }, [])
  React.useEffect(() => { void load() }, [load])
  const running = task?.state === "running"
  const taskId = running ? task.taskId : null
  React.useEffect(() => {
    if (!taskId) return
    let cancelled = false
    const timer = window.setInterval(() => {
      void luaSkillsDebugInstallStatus(taskId).then((next) => {
        if (cancelled) return
        setTask(next)
        // A finished install changes the installed state, so the status is read again.
        // 安装结束会改变已安装状态，因此重新读取状态。
        if (next.state !== "running") void load()
      }).catch((error: unknown) => {
        if (!cancelled) setLoadError(errorMessage(error))
      })
    }, INSTALL_POLL_INTERVAL_MS)
    return () => { cancelled = true; window.clearInterval(timer) }
  }, [taskId, load])
  const install = async () => {
    setStarting(true)
    try {
      setTask(await startLuaSkillsDebugInstall())
    } catch (error) {
      setLoadError(errorMessage(error))
    } finally {
      setStarting(false)
    }
  }
  const installed = status?.version != null
  const supported = status?.platform != null
  const badge = !status ? undefined : !supported ? (
    <Badge data-luaskills-debug-state="unsupported" variant="destructive">{t("skills.luaDebug.state.unsupported")}</Badge>
  ) : !installed ? (
    <Badge data-luaskills-debug-state="missing" variant="outline">{t("skills.luaDebug.state.missing")}</Badge>
  ) : status.compatible ? (
    <Badge data-luaskills-debug-state="ready" variant="success">{t("skills.luaDebug.state.ready")}</Badge>
  ) : (
    <Badge data-luaskills-debug-state="mismatch" variant="warning">{t("skills.luaDebug.state.mismatch")}</Badge>
  )
  const actions = status ? (
    <>
      <Button disabled={running || starting} onClick={() => void load()} size="sm" type="button" variant="outline">{t("knowledge.refresh")}</Button>
      <Button data-luaskills-debug-install="" disabled={!supported || running || starting} onClick={() => void install()} size="sm" type="button">
        {running ? t("skills.luaDebug.installing") : installed && status.compatible ? t("skills.luaDebug.reinstall") : t("skills.luaDebug.install")}
      </Button>
    </>
  ) : undefined
  return (
    <PageBody>
      {loadError ? (
        <PageNotice actions={<Button onClick={() => void load()} size="sm" type="button" variant="outline">{t("composer.capability.retry")}</Button>}
          details={loadError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">
          {t("skills.luaDebug.loadFailed")}
        </PageNotice>
      ) : null}
      <PageSection actions={actions} description={t("skills.luaDebug.toolDescription")} status={badge} title="luaskills-debug">
        {!status ? (loadError ? null : <StreamPlaceholder count={3} label={t("plugins.settings.statusLoading")} shape="rows" />) : (
          <>
            {running ? <InstallProgress task={task} /> : null}
            {task?.state === "failed" ? (
              <PageNotice details={task.error} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">{t("skills.luaDebug.installFailed")}</PageNotice>
            ) : null}
            {!supported ? <PageNotice tone="warning">{t("skills.luaDebug.unsupportedHelp")}</PageNotice> : null}
            {installed && !status.compatible ? (
              <PageNotice tone="warning">{formatMessage(t("skills.luaDebug.mismatchHelp"), { version: status.version ?? "", series: status.series })}</PageNotice>
            ) : null}
            <PageFacts items={statusFacts(status, t)} layout="list" />
          </>
        )}
      </PageSection>
      {status && installed && !status.runtimeReady ? (
        <PageSection description={t("skills.luaDebug.runtimeSetupHelp")} title={t("skills.luaDebug.runtimeSetup")}>
          <pre className="vk-page-hint" data-luaskills-debug-setup="">{runtimeSetupCommand(status)}</pre>
        </PageSection>
      ) : null}
      <PageSection description={t("skills.luaDebug.aiDescription")} title={t("skills.luaDebug.aiTitle")}>
        <PageHint>{t("skills.luaDebug.aiHint")}</PageHint>
      </PageSection>
    </PageBody>
  )
}

// InstallProgress renders the running install's phase, percent and downloaded bytes.
// InstallProgress 渲染运行中安装的阶段、百分比与已下载字节。
function InstallProgress(props: { task: LuaSkillsDebugInstallTask | null }) {
  const { t } = useI18n()
  if (!props.task) return null
  const { phase, overallPercent, completedBytes, totalBytes } = props.task
  const label = Object.hasOwn(INSTALL_PHASE_LABELS, phase) ? t(INSTALL_PHASE_LABELS[phase as keyof typeof INSTALL_PHASE_LABELS]) : phase
  const bytes = completedBytes == null ? null : totalBytes == null ? formatBytes(completedBytes) : `${formatBytes(completedBytes)} / ${formatBytes(totalBytes)}`
  return (
    <div data-luaskills-debug-progress="" role="status">
      <PageHint>{bytes ? `${label} · ${bytes}` : label}</PageHint>
      <Progress aria-label={label} value={overallPercent ?? 0} />
    </div>
  )
}

// statusFacts lists the matching series, the installed version and its paths.
// statusFacts 列出匹配系列、已安装版本及其路径。
function statusFacts(status: LuaSkillsDebugStatus, t: ReturnType<typeof useI18n>["t"]) {
  const missing = t("skills.luaDebug.notInstalled")
  const path = (value: string | null) => value ? <code>{value}</code> : missing
  return [
    { key: "series", label: t("skills.luaDebug.series"), value: `${status.series}.x` },
    { key: "version", label: t("skills.luaDebug.version"), value: status.version ?? missing },
    { key: "platform", label: t("skills.luaDebug.platform"), value: status.platform ?? t("skills.luaDebug.state.unsupported") },
    { key: "executable", label: t("skills.luaDebug.executable"), value: path(status.executable) },
    { key: "runtime", label: t("skills.luaDebug.runtimeRoot"), value: path(status.runtimeRoot) },
    {
      key: "runtimeReady",
      label: t("skills.luaDebug.runtimeReady"),
      value: status.version == null ? missing : t(status.runtimeReady ? "skills.luaDebug.runtimeReady.yes" : "skills.luaDebug.runtimeReady.no"),
      tone: status.version != null && !status.runtimeReady ? "attention" as const : "neutral" as const,
    },
    { key: "installRoot", label: t("skills.luaDebug.installRoot"), value: path(status.installRoot) },
  ]
}

// runtimeSetupCommand returns the one-time Lua runtime setup command for the installed package.
// runtimeSetupCommand 返回已安装包的一次性 Lua 运行时设置命令。
export function runtimeSetupCommand(status: LuaSkillsDebugStatus): string {
  const root = status.packageRoot ?? ""
  if (status.platform?.startsWith("windows")) {
    return `powershell -NoProfile -ExecutionPolicy Bypass -File "${root}\\setup_runtime.ps1" -Target lua`
  }
  return `bash "${root}/setup_runtime.sh" lua`
}
