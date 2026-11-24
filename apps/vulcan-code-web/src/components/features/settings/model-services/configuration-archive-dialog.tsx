import * as React from "react"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog"
import { Button } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { Input } from "@/ui-kit/primitives/input"
import { useI18n } from "@/i18n"
import { errorMessage } from "@/lib/display-format"
import {
  exportPublicAiConfiguration, importPublicAiConfiguration, previewPublicAiConfigurationImport,
} from "@/service/settings-actions-service"
import type {
  PublicAiConfigurationArchiveAccount, PublicAiConfigurationImportPreview,
  PublicAiConfigurationImportResult, PublicAiSettingsSnapshot,
} from "@/types"

// Defines which operation is open and how imported settings reach the owning page.
// 定义当前操作和将导入设置交给所在页面的方式。
interface ConfigurationArchiveDialogProps {
  mode: "export" | "import"
  settings: Pick<PublicAiSettingsSnapshot, "connections" | "providers">
  onClose: () => void
  onImported: (snapshot: PublicAiSettingsSnapshot) => void
}

// A credential choice uses a local connection ID for export or a file-bound index for import.
// 凭据选项在导出时使用本地连接 ID，导入时使用与文件绑定的索引。
interface ArchiveCredentialChoice extends PublicAiConfigurationArchiveAccount {
  id: string | number
  alreadyPresent: boolean
  requiresSignIn: boolean
}

// Keeps the exact ciphertext reviewed by the user together with its projected account metadata.
// 将用户预览的原始密文与由其投影出的账号元数据放在一起。
interface ReviewedArchive {
  encoded: string
  preview: PublicAiConfigurationImportPreview
}

// Collects the password and credential selections for one export or two-step import.
// 收集一次导出或两步导入所需的密码和凭据选项。
export function ConfigurationArchiveDialog({ mode, settings, onClose, onImported }: ConfigurationArchiveDialogProps) {
  const { t } = useI18n()
  const [password, setPassword] = React.useState("")
  const [confirmation, setConfirmation] = React.useState("")
  const [file, setFile] = React.useState<File | null>(null)
  const [busy, setBusy] = React.useState(false)
  const [failure, setFailure] = React.useState<string | null>(null)
  const [completed, setCompleted] = React.useState<PublicAiConfigurationImportResult | null>(null)
  // A password or file change invalidates every index from the old preview.
  // 密码或文件发生变化时，旧预览中的索引全部失效。
  const [review, setReview] = React.useState<ReviewedArchive | null>(null)
  const [selection, setSelection] = React.useState<Set<string | number>>(
    () => new Set(mode === "export" ? settings.connections.map((connection) => connection.id) : []),
  )
  const [includeOtherProviders, setIncludeOtherProviders] = React.useState(settings.connections.length === 0)
  const exporting = mode === "export"
  // Keep already installed connections visible but exclude them from an incremental import.
  // 保留已安装连接的显示，并从增量导入中排除这些连接。
  const installedIds = new Set(settings.connections.map((connection) => connection.id))
  const choices: ArchiveCredentialChoice[] = exporting
    ? settings.connections.map((connection) => ({
      id: connection.id,
      displayName: connection.display_name,
      providerId: connection.provider_id,
      serviceEntryId: connection.service_entry_id,
      alreadyPresent: false,
      requiresSignIn: connection.provider_id === "openai"
        && (connection.service_entry_id === "codex-subscription" || connection.service_entry_id === "chatgpt-plan"),
    }))
    : (review?.preview.accounts ?? []).map((account) => ({
      ...account,
      id: account.index,
      alreadyPresent: account.connectionId !== null && installedIds.has(account.connectionId),
      requiresSignIn: account.connectionId === null,
    }))
  const customProviderCount = exporting
    ? settings.providers.filter((provider) => provider.kind === "custom").length
    : review?.preview.providerCount ?? 0
  const chosenIds = choices.filter((choice) => !choice.alreadyPresent && selection.has(choice.id)).map((choice) => choice.id)
  const selecting = exporting || review !== null
  const withOtherProviders = customProviderCount > 0 && includeOtherProviders
  const canSubmit = !busy && !completed && password.trim() !== ""
    && (selecting ? chosenIds.length > 0 || withOtherProviders : file !== null)

  // Submits the selected operation while the form owns the only password copy in WebUI.
  // 表单在 WebUI 内持有密码，提交当前选定的迁移操作。
  async function submit(event: React.FormEvent) {
    event.preventDefault()
    if (!canSubmit) return
    if (exporting && password !== confirmation) {
      setFailure(t("settings.configurationArchive.passwordMismatch"))
      return
    }
    setBusy(true)
    setFailure(null)
    try {
      if (exporting) {
        const connectionIds = chosenIds.filter((id) => typeof id === "string")
        const encoded = await exportPublicAiConfiguration(password, connectionIds, withOtherProviders)
        downloadArchive(encoded)
        onClose()
      } else if (review) {
        const indexes = chosenIds.filter((id) => typeof id === "number")
        const result = await importPublicAiConfiguration(review.encoded, password, indexes, withOtherProviders)
        setCompleted(result)
        setPassword("")
        setReview(null)
        onImported(result.snapshot)
      } else if (file) {
        const encoded = await readArchive(file)
        const preview = await previewPublicAiConfigurationImport(encoded, password)
        setReview({ encoded, preview })
        setSelection(new Set(preview.accounts
          .filter((account) => account.connectionId === null || !installedIds.has(account.connectionId))
          .map((account) => account.index)))
        setIncludeOtherProviders(preview.accounts.length === 0)
      }
    } catch (reason) {
      setFailure(errorMessage(reason))
    } finally {
      setBusy(false)
    }
  }

  // Updates one selection using the latest checkbox state.
  // 使用最新的复选框状态更新一项选择。
  function toggleChoice(id: string | number, checked: boolean) {
    setSelection((previous) => {
      const next = new Set(previous)
      if (checked) next.add(id)
      else next.delete(id)
      return next
    })
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open && !busy) onClose() }}>
      <DialogContent size="sm">
        <form className="vk-form-stack" onSubmit={(event) => void submit(event)}>
          <DialogHeader>
            <DialogTitle>{t(exporting ? "settings.configurationArchive.export" : "settings.configurationArchive.import")}</DialogTitle>
            <DialogDescription>{t("settings.configurationArchive.description")}</DialogDescription>
          </DialogHeader>
          {completed ? (
            <div className="vk-form-stack" role="status">
              <p>{t("settings.configurationArchive.imported")
                .replace("{providers}", String(completed.providerCount))
                .replace("{accounts}", String(completed.accountCount))}</p>
              {completed.signInRequired.length > 0 ? (
                <div>
                  <strong>{t("settings.configurationArchive.signInRequired")}</strong>
                  <ul>{completed.signInRequired.map((account, index) => (
                    <li key={index}>{account.displayName} · {account.providerId}</li>
                  ))}</ul>
                </div>
              ) : null}
            </div>
          ) : (
            <>
              {!exporting ? (
                <label className="vk-field">
                  <span>{t("settings.configurationArchive.file")}</span>
                  <Input accept=".vc_mcfg" disabled={busy} type="file" onChange={(event) => {
                    setFile(event.target.files?.[0] ?? null)
                    setReview(null)
                    setFailure(null)
                  }} />
                </label>
              ) : null}
              <label className="vk-field">
                <span>{t("settings.configurationArchive.password")}</span>
                <Input autoComplete={exporting ? "new-password" : "off"} disabled={busy} type="password"
                  value={password} onChange={(event) => {
                    setPassword(event.target.value)
                    setReview(null)
                    setFailure(null)
                  }} />
              </label>
              {exporting ? (
                <label className="vk-field">
                  <span>{t("settings.configurationArchive.confirmPassword")}</span>
                  <Input autoComplete="new-password" disabled={busy} type="password"
                    value={confirmation} onChange={(event) => setConfirmation(event.target.value)} />
                </label>
              ) : null}
              {selecting ? (
                <ArchiveCredentialSelection choices={choices} selection={selection} busy={busy}
                  onToggle={toggleChoice} onSelectAll={(checked) => setSelection(new Set(checked
                    ? choices.filter((choice) => !choice.alreadyPresent).map((choice) => choice.id)
                    : []))} />
              ) : null}
              {selecting && customProviderCount > 0 ? (
                <label className="vk-check-label">
                  <Checkbox checked={withOtherProviders} disabled={busy}
                    onCheckedChange={(checked) => setIncludeOtherProviders(checked === true)} />
                  {t("settings.configurationArchive.includeOtherProviders")}
                </label>
              ) : null}
              {failure ? <p role="alert" className="vk-ms-account-error">{failure}</p> : null}
            </>
          )}
          <DialogFooter>
            <Button disabled={busy} type="button" variant="ghost" onClick={onClose}>
              {t(completed ? "settings.configurationArchive.done" : "settings.configurationArchive.cancel")}
            </Button>
            {!completed ? (
              <Button disabled={!canSubmit} type="submit">
                {t(exporting ? "settings.configurationArchive.export" : review
                  ? "settings.configurationArchive.import" : "settings.configurationArchive.review")}
              </Button>
            ) : null}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

// Renders available credential choices and a tri-state control for their selection.
// 渲染可选凭据，以及用于批量选择的三态复选框。
function ArchiveCredentialSelection({
  choices, selection, busy, onToggle, onSelectAll,
}: {
  choices: ArchiveCredentialChoice[]
  selection: ReadonlySet<string | number>
  busy: boolean
  onToggle: (id: string | number, checked: boolean) => void
  onSelectAll: (checked: boolean) => void
}) {
  const { t } = useI18n()
  const selectable = choices.filter((choice) => !choice.alreadyPresent)
  const selectedCount = selectable.filter((choice) => selection.has(choice.id)).length
  const allChecked = selectedCount > 0 && selectedCount === selectable.length
  return (
    <fieldset className="vk-fieldset">
      <legend className="vk-field-legend" data-variant="label">{t("settings.configurationArchive.credentials")}</legend>
      {selectable.length > 0 ? (
        <label className="vk-check-label">
          <Checkbox checked={allChecked || (selectedCount > 0 ? "indeterminate" : false)} disabled={busy}
            onCheckedChange={(checked) => onSelectAll(checked === true)} />
          {t("settings.misc.selectAll")}
        </label>
      ) : null}
      {choices.length === 0 ? <span className="vk-field-hint">{t("settings.configurationArchive.emptyCredentials")}</span> : null}
      <div className="vk-form-scroll-list vk-form-stack">
        {choices.map((choice) => (
          <label className="vk-check-label" key={choice.id}>
            <Checkbox checked={!choice.alreadyPresent && selection.has(choice.id)}
              disabled={busy || choice.alreadyPresent} onCheckedChange={(checked) => onToggle(choice.id, checked === true)} />
            <span className="vk-field-content">
              <span>{choice.displayName}</span>
              <span className="vk-field-hint">
                {choice.providerId} · {choice.serviceEntryId}
                {choice.alreadyPresent ? ` · ${t("settings.configurationArchive.alreadyPresent")}` : ""}
                {choice.requiresSignIn ? ` · ${t("settings.configurationArchive.reauthorize")}` : ""}
              </span>
            </span>
          </label>
        ))}
      </div>
    </fieldset>
  )
}

// Encodes the selected ciphertext file without transforming its bytes.
// 编码选择的密文文件，保留原始字节。
function readArchive(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      if (typeof reader.result !== "string") {
        reject(new Error("Could not read configuration archive"))
        return
      }
      resolve(reader.result.slice(reader.result.indexOf(",") + 1))
    }
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(file)
  })
}

// Downloads the encrypted ZIP under the Vulcan configuration extension.
// 使用 Vulcan 配置专用后缀下载加密 ZIP。
function downloadArchive(encoded: string) {
  const bytes = Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0))
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/zip" }))
  const link = document.createElement("a")
  link.href = url
  link.download = "vulcan-configuration.vc_mcfg"
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(url), 1000)
}
