import * as React from "react"
import { RefreshCw } from "lucide-react"

import type { ProviderRetrySettingsSaveRequest, SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { PanelTitle, StatusPill } from "@/components/shared/status"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import type { ProviderRetrySettingsSummary } from "@/types"

// DEFAULT_PROVIDER_RETRY_SETTINGS mirrors the service API compatibility default.
// DEFAULT_PROVIDER_RETRY_SETTINGS 对齐 Service API 的兼容默认值。
const DEFAULT_PROVIDER_RETRY_SETTINGS: ProviderRetrySettingsSummary = {
  enabled: true,
  maxRetries: 5,
  initialDelayMs: 2_000,
  maxDelayMs: 30_000,
  backoffFactorMilli: 2_000,
  jitterPercent: 20,
  respectRetryAfter: true,
  retry429: true,
  rateLimit: {
    maxRetries: 100_000,
    initialDelayMs: 5_000,
    maxDelayMs: 1_800_000,
    backoffFactorMilli: 2_000,
  },
}

// MAX_SAFE_DELAY_SECONDS keeps millisecond conversion within JavaScript's exact integer range.
// MAX_SAFE_DELAY_SECONDS 确保换算为毫秒后仍位于 JavaScript 可精确表示的整数范围内。
const MAX_SAFE_DELAY_SECONDS = Math.floor(Number.MAX_SAFE_INTEGER / 1_000)

// ProviderRetrySettingsDraft stores editable text without collapsing temporarily empty number fields.
// ProviderRetrySettingsDraft 保存可编辑文本，避免临时为空的数字输入框被立即压回零。
interface ProviderRetrySettingsDraft {
  // Whether automatic retry is enabled.
  // 是否启用自动重试。
  enabled: boolean
  // Maximum ordinary retry count.
  // 普通重试最大次数。
  maxRetries: string
  // Initial ordinary wait in seconds.
  // 普通重试首次等待秒数。
  initialDelaySeconds: string
  // Maximum ordinary wait in seconds.
  // 普通重试最大等待秒数。
  maxDelaySeconds: string
  // Ordinary retry multiplier.
  // 普通重试倍数。
  backoffFactor: string
  // Ordinary retry jitter percentage.
  // 普通重试抖动百分比。
  jitterPercent: string
  // Whether Retry-After hints are honored.
  // 是否遵守 Retry-After 提示。
  respectRetryAfter: boolean
  // Whether rate-limit failures may be retried.
  // 是否允许重试限流失败。
  retry429: boolean
  // Maximum finite rate-limit retries.
  // 限流最大有限重试次数。
  rateLimitMaxRetries: string
  // Initial rate-limit wait in seconds.
  // 限流首次等待秒数。
  rateLimitInitialDelaySeconds: string
  // Maximum rate-limit wait in seconds.
  // 限流最大等待秒数。
  rateLimitMaxDelaySeconds: string
  // Rate-limit retry multiplier.
  // 限流重试倍数。
  rateLimitBackoffFactor: string
}

// ProviderRetryNumberFieldProps describes one validated numeric setting input.
// ProviderRetryNumberFieldProps 描述单个受校验的数字设置输入。
interface ProviderRetryNumberFieldProps {
  // Field label.
  // 字段标签。
  label: string
  // Compact help text.
  // 紧凑帮助文本。
  help: string
  // Minimum browser input value.
  // 浏览器输入最小值。
  min: number
  // Maximum browser input value.
  // 浏览器输入最大值。
  max: number
  // Browser input increment.
  // 浏览器输入步长。
  step: number
  // Current editable value.
  // 当前可编辑值。
  value: string
  // Updates the editable value.
  // 更新可编辑值。
  onChange: (value: string) => void
}

// ProviderRetryNumberField renders one number input with its exact backend constraints.
// ProviderRetryNumberField 渲染一个带精确后台约束的数字输入。
function ProviderRetryNumberField(props: ProviderRetryNumberFieldProps) {
  // inputId binds the shared label and description to this exact numeric control.
  // inputId 将统一标签和说明绑定到当前数字控件。
  const inputId = React.useId()

  return (
    <Field>
      <FieldLabel htmlFor={inputId}>{props.label}</FieldLabel>
      <Input
        id={inputId}
        max={props.max}
        min={props.min}
        onChange={(event) => props.onChange(event.currentTarget.value)}
        step={props.step}
        type="number"
        value={props.value}
      />
      <FieldDescription>{props.help}</FieldDescription>
    </Field>
  )
}

// ProviderRetrySettingsArea renders and saves the complete Provider retry configuration.
// ProviderRetrySettingsArea 渲染并保存完整 Provider 重试配置。
// Parameters: props supplies retry policy and save callbacks; returns raw validated drafts, explicit reload and guarded save controls.
// 参数：props 提供重试策略与保存回调；返回原始校验草稿、显式重载及受保护保存控件。
export function ProviderRetrySettingsArea(props: {
  // Service-backed settings actions.
  // 服务端设置操作。
  actions: SettingsActionProps
  // Current retry settings projected by the service.
  // 服务端投影出的当前重试设置。
  settings?: ProviderRetrySettingsSummary
  // Embedded removes the standalone full-height sizing when this panel is grouped with other settings.
  // Embedded 在与其他设置组合时移除独立页面的全高尺寸约束。
  embedded?: boolean
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => locale === "zh-CN" ? zh : en, [locale])
  const settings = props.settings ?? DEFAULT_PROVIDER_RETRY_SETTINGS
  const sourceDraft = React.useMemo(() => retrySettingsDraft(settings), [settings])
  const [draft, setDraft] = React.useState<ProviderRetrySettingsDraft>(sourceDraft)
  const [saveState, setSaveState] = React.useState<"idle" | "saving" | "saved">("idle")
  const [saveError, setSaveError] = React.useState<string | null>(null)
  // baselineDraft tracks the values the user actually began editing.
  // baselineDraft 跟踪用户实际开始编辑的值。
  const [baselineDraft, setBaselineDraft] = React.useState(sourceDraft)
  // observedSource ignores reference-only snapshot refreshes without erasing local input.
  // observedSource 忽略仅引用变化的快照刷新，不擦除本地输入。
  const observedSource = React.useRef(JSON.stringify(sourceDraft))
  // savingRef serializes saves synchronously and protects the pending draft.
  // savingRef 同步串行化保存，并保护待保存草稿。
  const savingRef = React.useRef(false)
  const dirty = JSON.stringify(draft) !== JSON.stringify(baselineDraft)
  const sourceChanged = JSON.stringify(sourceDraft) !== JSON.stringify(baselineDraft)

  React.useEffect(() => {
    const sourceKey = JSON.stringify(sourceDraft)
    if (observedSource.current === sourceKey) return
    observedSource.current = sourceKey
    if (dirty || savingRef.current) return
    setDraft(sourceDraft)
    setBaselineDraft(sourceDraft)
    setSaveState("idle")
    setSaveError(null)
  }, [dirty, sourceDraft])

  const validationError = validateRetrySettingsDraft(draft, text)
  const saveDraft = React.useCallback(async () => {
    if (savingRef.current || !dirty || sourceChanged) return
    const error = validateRetrySettingsDraft(draft, text)
    if (error) {
      setSaveError(error)
      return
    }
    savingRef.current = true
    setSaveState("saving")
    setSaveError(null)
    try {
      await props.actions.onSaveProviderRetrySettings(retrySettingsRequest(draft))
      setBaselineDraft(draft)
      setSaveState("saved")
    } catch (errorValue) {
      setSaveState("idle")
      setSaveError(errorValue instanceof Error ? errorValue.message : text("保存重试设置失败。", "Failed to save retry settings."))
    } finally {
      savingRef.current = false
    }
  }, [dirty, draft, props.actions, sourceChanged, text])

  return (
    <section className={props.embedded ? "settings-retry-section min-w-0 space-y-4" : "panel full-height settings-detail-panel"}>
      <PanelTitle icon={<RefreshCw size={17} />} title={text("请求重试", "Request retry")} />
      <fieldset disabled={saveState === "saving"} className="settings-form-grid min-w-0">
        <article className="space-y-3">
          <StatusPill status={draft.enabled ? "ready" : "warning"} label={draft.enabled ? text("自动重试已启用", "Automatic retry enabled") : text("自动重试已停用", "Automatic retry disabled")} />
          <label className="settings-check-row settings-retry-toggle">
            <span>{text("启用可重试 Provider 错误的自动重试", "Automatically retry eligible Provider failures")}</span>
            <Switch checked={draft.enabled} onCheckedChange={(checked) => setDraft((current) => ({ ...current, enabled: checked }))} />
          </label>
          <label className="settings-check-row settings-retry-toggle">
            <span>{text("遵守 Provider 返回的 Retry-After 等待提示", "Honor Provider Retry-After wait hints")}</span>
            <Switch checked={draft.respectRetryAfter} onCheckedChange={(checked) => setDraft((current) => ({ ...current, respectRetryAfter: checked }))} />
          </label>
        </article>

        <article className="space-y-3 border-t pt-4">
          <strong>{text("普通瞬态错误", "Ordinary transient failures")}</strong>
          <p>{text("用于网络中断、超时和可重试 5xx。最大次数为首次请求之后允许追加的重试次数。", "Used for network interruptions, timeouts, and retryable 5xx responses. Maximum retries are additional attempts after the initial request.")}</p>
          <div className="settings-form-grid two">
            <ProviderRetryNumberField help={text("0 到 100 次", "0 to 100 retries")} label={text("最大重试次数", "Maximum retries")} max={100} min={0} onChange={(value) => setDraft((current) => ({ ...current, maxRetries: value }))} step={1} value={draft.maxRetries} />
            <ProviderRetryNumberField help={text("必须大于 0", "Must be greater than 0")} label={text("首次等待（秒）", "Initial wait (seconds)")} max={MAX_SAFE_DELAY_SECONDS} min={0.001} onChange={(value) => setDraft((current) => ({ ...current, initialDelaySeconds: value }))} step={0.001} value={draft.initialDelaySeconds} />
            <ProviderRetryNumberField help={text("不得小于首次等待", "Must not be less than the initial wait")} label={text("最大等待（秒）", "Maximum wait (seconds)")} max={MAX_SAFE_DELAY_SECONDS} min={0.001} onChange={(value) => setDraft((current) => ({ ...current, maxDelaySeconds: value }))} step={0.001} value={draft.maxDelaySeconds} />
            <ProviderRetryNumberField help={text("1 为固定间隔，最大 8", "1 is fixed; maximum 8")} label={text("递增倍数", "Backoff multiplier")} max={8} min={1} onChange={(value) => setDraft((current) => ({ ...current, backoffFactor: value }))} step={0.001} value={draft.backoffFactor} />
            <ProviderRetryNumberField help={text("0 到 80% 的确定性抖动", "0% to 80% deterministic jitter")} label={text("抖动百分比", "Jitter percentage")} max={80} min={0} onChange={(value) => setDraft((current) => ({ ...current, jitterPercent: value }))} step={1} value={draft.jitterPercent} />
          </div>
        </article>

        <article className="space-y-3 border-t pt-4">
          <strong>{text("429 与限流错误", "429 and rate-limit failures")}</strong>
          <label className="settings-check-row settings-retry-toggle">
            <span>{text("自动重试 HTTP 429 与 Anthropic rate_limit_error", "Retry HTTP 429 and Anthropic rate_limit_error")}</span>
            <Switch checked={draft.retry429} onCheckedChange={(checked) => setDraft((current) => ({ ...current, retry429: checked }))} />
          </label>
          <p>{text("每次限流都会先显示等待记录，且所有自动重试都受宿主硬上限约束。", "Every rate limit first creates a visible wait record, and all automatic retries remain under the host hard ceiling.")}</p>
          <div className="settings-form-grid two">
            <ProviderRetryNumberField help={text("1 到 100000 次", "1 to 100000 retries")} label={text("最大重试次数", "Maximum retries")} max={100000} min={1} onChange={(value) => setDraft((current) => ({ ...current, rateLimitMaxRetries: value }))} step={1} value={draft.rateLimitMaxRetries} />
            <ProviderRetryNumberField help={text("0.001 到 1800 秒", "0.001 to 1800 seconds")} label={text("首次等待（秒）", "Initial wait (seconds)")} max={1800} min={0.001} onChange={(value) => setDraft((current) => ({ ...current, rateLimitInitialDelaySeconds: value }))} step={0.001} value={draft.rateLimitInitialDelaySeconds} />
            <ProviderRetryNumberField help={text("不得小于首次等待，硬上限 1800 秒", "Must not be less than the initial wait; hard cap 1800 seconds")} label={text("最大等待（秒）", "Maximum wait (seconds)")} max={1800} min={0.001} onChange={(value) => setDraft((current) => ({ ...current, rateLimitMaxDelaySeconds: value }))} step={0.001} value={draft.rateLimitMaxDelaySeconds} />
            <ProviderRetryNumberField help={text("1 为固定间隔；大于 1 会逐次递增", "1 is fixed; values above 1 grow each retry")} label={text("递增倍数", "Backoff multiplier")} max={8} min={1} onChange={(value) => setDraft((current) => ({ ...current, rateLimitBackoffFactor: value }))} step={0.001} value={draft.rateLimitBackoffFactor} />
          </div>
        </article>

        {validationError || saveError ? <p className="settings-validation-error" role="alert">{validationError ?? saveError}</p> : null}
        {sourceChanged && <div className="space-y-2" role="status"><p>{text("服务器设置已更新，请重新载入后再保存。当前草稿仍保留。", "Server settings changed. Reload before saving; your current draft is retained.")}</p>
          <Button type="button" variant="outline" onClick={() => { setDraft(sourceDraft); setBaselineDraft(sourceDraft); setSaveState("idle"); setSaveError(null) }}>{text("重新载入当前设置", "Reload current settings")}</Button></div>}
        <div className="settings-card-actions">
          <Button disabled={!dirty || sourceChanged || Boolean(validationError) || saveState === "saving"} onClick={() => void saveDraft()} type="button">
            {saveState === "saving" ? text("保存中", "Saving") : saveState === "saved" && !dirty ? text("已保存", "Saved") : text("保存重试设置", "Save retry settings")}
          </Button>
          {!dirty && saveState === "idle" ? <span className="settings-muted-line">{text("当前设置已同步", "Current settings are synchronized")}</span> : null}
        </div>
      </fieldset>
    </section>
  )
}

// retrySettingsDraft converts one service summary into stable editable form values.
// retrySettingsDraft 将服务摘要转换为稳定的可编辑表单值。
function retrySettingsDraft(settings: ProviderRetrySettingsSummary): ProviderRetrySettingsDraft {
  return {
    enabled: settings.enabled,
    maxRetries: String(settings.maxRetries),
    initialDelaySeconds: millisecondsToSecondsText(settings.initialDelayMs),
    maxDelaySeconds: millisecondsToSecondsText(settings.maxDelayMs),
    backoffFactor: factorMilliToText(settings.backoffFactorMilli),
    jitterPercent: String(settings.jitterPercent),
    respectRetryAfter: settings.respectRetryAfter,
    retry429: settings.retry429,
    rateLimitMaxRetries: String(settings.rateLimit.maxRetries),
    rateLimitInitialDelaySeconds: millisecondsToSecondsText(settings.rateLimit.initialDelayMs),
    rateLimitMaxDelaySeconds: millisecondsToSecondsText(settings.rateLimit.maxDelayMs),
    rateLimitBackoffFactor: factorMilliToText(settings.rateLimit.backoffFactorMilli),
  }
}

// retrySettingsRequest converts one validated form draft into the fixed-point service contract.
// retrySettingsRequest 将已校验表单草稿转换为定点 Service 契约。
function retrySettingsRequest(draft: ProviderRetrySettingsDraft): ProviderRetrySettingsSaveRequest {
  return {
    enabled: draft.enabled,
    maxRetries: Number(draft.maxRetries),
    initialDelayMs: secondsTextToMilliseconds(draft.initialDelaySeconds),
    maxDelayMs: secondsTextToMilliseconds(draft.maxDelaySeconds),
    backoffFactorMilli: factorTextToMilli(draft.backoffFactor),
    jitterPercent: Number(draft.jitterPercent),
    respectRetryAfter: draft.respectRetryAfter,
    retry429: draft.retry429,
    rateLimit: {
      maxRetries: Number(draft.rateLimitMaxRetries),
      initialDelayMs: secondsTextToMilliseconds(draft.rateLimitInitialDelaySeconds),
      maxDelayMs: secondsTextToMilliseconds(draft.rateLimitMaxDelaySeconds),
      backoffFactorMilli: factorTextToMilli(draft.rateLimitBackoffFactor),
    },
  }
}

// validateRetrySettingsDraft mirrors backend numeric and cross-field constraints before saving.
// validateRetrySettingsDraft 在保存前对齐后台数字范围与跨字段约束。
function validateRetrySettingsDraft(draft: ProviderRetrySettingsDraft, text: (zh: string, en: string) => string): string | null {
  const ordinaryMaxRetries = integerValue(draft.maxRetries)
  const ordinaryInitialSeconds = finiteNumber(draft.initialDelaySeconds)
  const ordinaryMaxSeconds = finiteNumber(draft.maxDelaySeconds)
  const ordinaryFactor = finiteNumber(draft.backoffFactor)
  const jitterPercent = integerValue(draft.jitterPercent)
  const rateLimitMaxRetries = integerValue(draft.rateLimitMaxRetries)
  const rateLimitInitialSeconds = finiteNumber(draft.rateLimitInitialDelaySeconds)
  const rateLimitMaxSeconds = finiteNumber(draft.rateLimitMaxDelaySeconds)
  const rateLimitFactor = finiteNumber(draft.rateLimitBackoffFactor)
  if (ordinaryMaxRetries == null || ordinaryMaxRetries < 0 || ordinaryMaxRetries > 100) return text("普通重试最大次数必须是 0 到 100 的整数。", "Ordinary maximum retries must be an integer from 0 to 100.")
  if (!validDelayPair(ordinaryInitialSeconds, ordinaryMaxSeconds, MAX_SAFE_DELAY_SECONDS)) return text("普通重试等待必须大于 0，且最大等待不能小于首次等待。", "Ordinary waits must be positive, and maximum wait cannot be less than initial wait.")
  if (ordinaryFactor == null || ordinaryFactor < 1 || ordinaryFactor > 8) return text("普通重试递增倍数必须位于 1.0 到 8.0。", "Ordinary backoff multiplier must be from 1.0 to 8.0.")
  if (jitterPercent == null || jitterPercent < 0 || jitterPercent > 80) return text("抖动百分比必须是 0 到 80 的整数。", "Jitter percentage must be an integer from 0 to 80.")
  if (rateLimitMaxRetries == null || rateLimitMaxRetries < 1 || rateLimitMaxRetries > 100000) return text("限流最大重试次数必须是 1 到 100000 的整数。", "Rate-limit maximum retries must be an integer from 1 to 100000.")
  if (!validDelayPair(rateLimitInitialSeconds, rateLimitMaxSeconds, 1800)) return text("限流等待必须大于 0、最大 1800 秒，且最大等待不能小于首次等待。", "Rate-limit waits must be positive, at most 1800 seconds, and maximum wait cannot be less than initial wait.")
  if (rateLimitFactor == null || rateLimitFactor < 1 || rateLimitFactor > 8) return text("限流递增倍数必须位于 1.0 到 8.0。", "Rate-limit backoff multiplier must be from 1.0 to 8.0.")
  return null
}

// validDelayPair checks one initial and maximum delay pair expressed in seconds.
// validDelayPair 校验一组以秒表示的首次与最大等待。
function validDelayPair(initialSeconds: number | null, maxSeconds: number | null, ceilingSeconds: number): boolean {
  return initialSeconds != null && maxSeconds != null && initialSeconds > 0 && maxSeconds >= initialSeconds && maxSeconds <= ceilingSeconds
}

// integerValue parses one exact integer input or returns null.
// integerValue 解析一个精确整数输入，失败时返回 null。
function integerValue(value: string): number | null {
  const parsed = finiteNumber(value)
  return parsed != null && Number.isInteger(parsed) ? parsed : null
}

// finiteNumber parses one finite browser number input or returns null.
// finiteNumber 解析一个有限浏览器数字输入，失败时返回 null。
function finiteNumber(value: string): number | null {
  if (!value.trim()) return null
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : null
}

// millisecondsToSecondsText formats exact millisecond settings for editable seconds input.
// millisecondsToSecondsText 将精确毫秒设置格式化为可编辑秒输入。
function millisecondsToSecondsText(milliseconds: number): string {
  return String(milliseconds / 1_000)
}

// secondsTextToMilliseconds converts validated seconds text into integer milliseconds.
// secondsTextToMilliseconds 将已校验秒文本转换为整数毫秒。
function secondsTextToMilliseconds(seconds: string): number {
  return Math.round(Number(seconds) * 1_000)
}

// factorMilliToText formats a fixed-point multiplier for the settings form.
// factorMilliToText 将定点倍数格式化为设置表单文本。
function factorMilliToText(factorMilli: number): string {
  return String(factorMilli / 1_000)
}

// factorTextToMilli converts a validated multiplier into fixed-point thousandths.
// factorTextToMilli 将已校验倍数转换为定点千分数。
function factorTextToMilli(factor: string): number {
  return Math.round(Number(factor) * 1_000)
}
