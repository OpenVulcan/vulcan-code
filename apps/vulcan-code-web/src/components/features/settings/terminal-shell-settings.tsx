import * as React from "react"
import { Check, LoaderCircle, SquareTerminal } from "lucide-react"

import { toast } from "@/ui-kit/primitives/toast"
import { formatMessage, useI18n } from "@/i18n"
import type { CommandShellSettingsSummary } from "@/types"

// AUTO_CHOICE is the radio value that clears the preference so the host picks the shell.
// AUTO_CHOICE 是清除偏好、由宿主选择 Shell 的单选值。
const AUTO_CHOICE = "auto"

// TerminalShellSettingsCard picks the shell the integrated terminal opens; the command tools share the same default.
// Each choice saves at once and only affects terminals opened afterwards.
// TerminalShellSettingsCard 选择集成终端打开的 Shell；命令工具共用同一默认值。
// 每次选择立即保存，只影响之后新开的终端。
// Parameters: settings is the workbench projection (absent before the service is ready); onSave persists one choice.
// 参数：settings 为工作台投影（服务就绪前缺省）；onSave 持久化一次选择。
export function TerminalShellSettingsCard(props: {
  settings?: CommandShellSettingsSummary
  onSave: (shellId: string | null) => Promise<void>
}) {
  const { t } = useI18n()
  // pending is the choice being saved; error keeps the last failure next to the list.
  // pending 为正在保存的选择；error 在列表旁保留最近一次失败。
  const [pending, setPending] = React.useState<string | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const settings = props.settings
  const options = settings?.options ?? []
  const configured = settings?.configuredShellId ?? null
  const selected = pending ?? configured ?? AUTO_CHOICE
  const labelOf = (id: string | null | undefined) => options.find((option) => option.id === id)?.label ?? id ?? ""
  // A persisted shell that this host no longer detects falls back to the automatic pick.
  // 本机已检测不到的持久化 Shell 会回退到自动选择。
  const configuredMissing = configured !== null && !options.some((option) => option.id === configured)

  const choose = async (choice: string) => {
    if (pending !== null || choice === (configured ?? AUTO_CHOICE)) return
    setPending(choice)
    setError(null)
    try {
      await props.onSave(choice === AUTO_CHOICE ? null : choice)
      toast.success(t("settings.terminalShell.saved"))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(null)
    }
  }

  const choices = [
    { id: AUTO_CHOICE, label: t("settings.terminalShell.auto"), detail: options[0] ? formatMessage(t("settings.terminalShell.autoDetail"), { shell: options[0].label }) : null },
    ...options.map((option) => ({ id: option.id, label: option.label, detail: option.id })),
  ]

  return (
    <section className="context-basics-section">
      <header className="context-basics-head">
        <div>
          <h3>{t("settings.terminalShell.title")}</h3>
          <p>{t("settings.terminalShell.description")}</p>
          <p className="vk-form-note">{t("settings.terminalShell.newTerminals")}</p>
        </div>
      </header>
      {configuredMissing ? (
        <p className="vk-field-error" role="status">{formatMessage(t("settings.terminalShell.missing"), { shell: configured, fallback: labelOf(settings?.resolvedShellId) })}</p>
      ) : null}
      {options.length === 0 ? <p className="vk-form-note">{t("settings.terminalShell.none")}</p> : (
        <div className="terminal-shell-choices" role="radiogroup" aria-label={t("settings.terminalShell.title")}>
          {choices.map((choice) => {
            const checked = selected === choice.id
            return (
              <button
                aria-checked={checked}
                className="terminal-shell-choice"
                disabled={pending !== null}
                key={choice.id}
                onClick={() => void choose(choice.id)}
                role="radio"
                type="button"
              >
                <SquareTerminal aria-hidden="true" />
                <span>
                  <strong>{choice.label}</strong>
                  {choice.detail ? <small>{choice.detail}</small> : null}
                </span>
                {pending === choice.id ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : checked ? <Check aria-hidden="true" /> : null}
              </button>
            )
          })}
        </div>
      )}
      {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
    </section>
  )
}
