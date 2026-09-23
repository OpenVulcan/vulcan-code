import * as React from "react"
import { RefreshCw } from "lucide-react"

import type { ProviderRetrySettingsSaveRequest, SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { PanelTitle } from "@/components/shared/status"
import { Button } from "@/components/ui/button"
import { DisclosureSection } from "@/components/ui/disclosure"
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
      setSaveError(errorValue instanceof Error ? errorValue.message : text("没能保存，你的修改还在，可以稍后再试。", "Couldn't save. Your edits are kept; try again later."))
    } finally {
      savingRef.current = false
    }
  }, [dirty, draft, props.actions, sourceChanged, text])

  return (
    <section className={props.embedded ? "settings-retry-section min-w-0 space-y-4" : "panel full-height settings-detail-panel"}>
      <PanelTitle icon={<RefreshCw size={17} />} title={text("出错重试", "Retries")} />
      <fieldset disabled={saveState === "saving"} className="settings-form-grid min-w-0">
        <article className="space-y-3">
          <strong>{text("网络或服务器出错", "Network or server errors")}</strong>
          <label className="settings-check-row settings-retry-toggle">
            <span><span className="block">{text("自动重试", "Retry automatically")}</span>
              <small>{text("网络中断、超时或服务器临时出错时，自动再试几次。", "Try again a few times after a dropped connection, timeout or temporary server error.")}</small></span>
            <Switch checked={draft.enabled} onCheckedChange={(checked) => setDraft((current) => ({ ...current, enabled: checked }))} />
          </label>
          <label className="settings-check-row settings-retry-toggle">
            <span><span className="block">{text("按服务商建议的时间等待", "Wait as long as the provider asks")}</span>
              <small>{text("服务商说明了需要等多久时，照它说的时间再重试。", "When the provider says how long to wait, wait that long before retrying.")}</small></span>
            <Switch checked={draft.respectRetryAfter} onCheckedChange={(checked) => setDraft((current) => ({ ...current, respectRetryAfter: checked }))} />
          </label>
          <DisclosureSection
            description={text(`最多再试 ${draft.maxRetries} 次，每次间隔 ${draft.initialDelaySeconds}–${draft.maxDelaySeconds} 秒`, `Up to ${draft.maxRetries} retries, ${draft.initialDelaySeconds}–${draft.maxDelaySeconds}s apart`)}
            title={text("重试次数与间隔", "Retry count and spacing")}
          >
          <div className="settings-form-grid two">
            <ProviderRetryNumberField help={text("不含第一次请求，0 到 100", "Not counting the first request; 0 to 100")} label={text("最多再试几次", "Maximum retries")} max={100} min={0} onChange={(value) => setDraft((current) => ({ ...current, maxRetries: value }))} step={1} value={draft.maxRetries} />
            <ProviderRetryNumberField help={text("第一次重试前等多久", "Wait before the first retry")} label={text("第一次等待（秒）", "First wait (seconds)")} max={MAX_SAFE_DELAY_SECONDS} min={0.001} onChange={(value) => setDraft((current) => ({ ...current, initialDelaySeconds: value }))} step={0.001} value={draft.initialDelaySeconds} />
            <ProviderRetryNumberField help={text("间隔再长也不会超过它", "Waits never grow past this")} label={text("最长等待（秒）", "Longest wait (seconds)")} max={MAX_SAFE_DELAY_SECONDS} min={0.001} onChange={(value) => setDraft((current) => ({ ...current, maxDelaySeconds: value }))} step={0.001} value={draft.maxDelaySeconds} />
            <ProviderRetryNumberField help={text("每次间隔乘以它；1 表示间隔不变", "Each wait is multiplied by this; 1 keeps it fixed")} label={text("间隔增长倍数", "Wait growth")} max={8} min={1} onChange={(value) => setDraft((current) => ({ ...current, backoffFactor: value }))} step={0.001} value={draft.backoffFactor} />
            <ProviderRetryNumberField help={text("让间隔略有随机，避免多个请求同时重试；0 到 80", "Adds a little randomness so requests don't retry in lockstep; 0 to 80")} label={text("随机浮动（%）", "Random variation (%)")} max={80} min={0} onChange={(value) => setDraft((current) => ({ ...current, jitterPercent: value }))} step={1} value={draft.jitterPercent} />
          </div>
          </DisclosureSection>
        </article>

        <article className="space-y-3">
          <strong>{text("请求太频繁（限流）", "Too many requests (rate limits)")}</strong>
          <label className="settings-check-row settings-retry-toggle">
            <span><span className="block">{text("被限流时等待后重试", "Wait and retry when rate-limited")}</span>
              <small>{text("服务商提示请求太频繁（429）时，等一会儿再发。每次等待都会显示在对话里。", "When the provider says there are too many requests (429), wait and send again. Each wait shows in the conversation.")}</small></span>
            <Switch checked={draft.retry429} onCheckedChange={(checked) => setDraft((current) => ({ ...current, retry429: checked }))} />
          </label>
          <DisclosureSection
            description={text(`最多再试 ${draft.rateLimitMaxRetries} 次，每次间隔 ${draft.rateLimitInitialDelaySeconds}–${draft.rateLimitMaxDelaySeconds} 秒`, `Up to ${draft.rateLimitMaxRetries} retries, ${draft.rateLimitInitialDelaySeconds}–${draft.rateLimitMaxDelaySeconds}s apart`)}
            title={text("等待次数与间隔", "Wait count and spacing")}
          >
          <div className="settings-form-grid two">
            <ProviderRetryNumberField help={text("1 到 100000", "1 to 100000")} label={text("最多再试几次", "Maximum retries")} max={100000} min={1} onChange={(value) => setDraft((current) => ({ ...current, rateLimitMaxRetries: value }))} step={1} value={draft.rateLimitMaxRetries} />
            <ProviderRetryNumberField help={text("第一次重试前等多久，最多 1800 秒", "Wait before the first retry, up to 1800s")} label={text("第一次等待（秒）", "First wait (seconds)")} max={1800} min={0.001} onChange={(value) => setDraft((current) => ({ ...current, rateLimitInitialDelaySeconds: value }))} step={0.001} value={draft.rateLimitInitialDelaySeconds} />
            <ProviderRetryNumberField help={text("间隔再长也不会超过它，最多 1800 秒", "Waits never grow past this, up to 1800s")} label={text("最长等待（秒）", "Longest wait (seconds)")} max={1800} min={0.001} onChange={(value) => setDraft((current) => ({ ...current, rateLimitMaxDelaySeconds: value }))} step={0.001} value={draft.rateLimitMaxDelaySeconds} />
            <ProviderRetryNumberField help={text("每次间隔乘以它；1 表示间隔不变", "Each wait is multiplied by this; 1 keeps it fixed")} label={text("间隔增长倍数", "Wait growth")} max={8} min={1} onChange={(value) => setDraft((current) => ({ ...current, rateLimitBackoffFactor: value }))} step={0.001} value={draft.rateLimitBackoffFactor} />
          </div>
          </DisclosureSection>
        </article>

        {validationError || saveError ? <p className="settings-validation-error" role="alert">{validationError ?? saveError}</p> : null}
        {sourceChanged && <div className="space-y-2" role="status"><p>{text("这些设置刚在别处被修改过。你的修改还在，重新载入后再保存即可。", "These settings were just changed elsewhere. Your edits are kept; reload, then save.")}</p>
          <Button type="button" variant="outline" onClick={() => { setDraft(sourceDraft); setBaselineDraft(sourceDraft); setSaveState("idle"); setSaveError(null) }}>{text("重新载入", "Reload")}</Button></div>}
        <div className="settings-card-actions">
          <Button disabled={!dirty || sourceChanged || Boolean(validationError) || saveState === "saving"} onClick={() => void saveDraft()} type="button">
            {saveState === "saving" ? text("正在保存…", "Saving…") : saveState === "saved" && !dirty ? text("已保存", "Saved") : text("保存", "Save")}
          </Button>
          {dirty ? <span className="settings-muted-line">{text("有未保存的修改", "Unsaved changes")}</span> : null}
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
  if (ordinaryMaxRetries == null || ordinaryMaxRetries < 0 || ordinaryMaxRetries > 100) return text("出错重试的次数要填 0 到 100 之间的整数。", "Retry count must be a whole number from 0 to 100.")
  if (!validDelayPair(ordinaryInitialSeconds, ordinaryMaxSeconds, MAX_SAFE_DELAY_SECONDS)) return text("出错重试的等待时间要大于 0，且最长等待不能比第一次等待短。", "Retry waits must be above 0, and the longest wait can't be shorter than the first.")
  if (ordinaryFactor == null || ordinaryFactor < 1 || ordinaryFactor > 8) return text("出错重试的间隔增长倍数要在 1 到 8 之间。", "Retry wait growth must be between 1 and 8.")
  if (jitterPercent == null || jitterPercent < 0 || jitterPercent > 80) return text("随机浮动要填 0 到 80 之间的整数。", "Random variation must be a whole number from 0 to 80.")
  if (rateLimitMaxRetries == null || rateLimitMaxRetries < 1 || rateLimitMaxRetries > 100000) return text("限流重试的次数要填 1 到 100000 之间的整数。", "Rate-limit retry count must be a whole number from 1 to 100000.")
  if (!validDelayPair(rateLimitInitialSeconds, rateLimitMaxSeconds, 1800)) return text("限流等待要大于 0、不超过 1800 秒，且最长等待不能比第一次等待短。", "Rate-limit waits must be above 0, at most 1800s, and the longest can't be shorter than the first.")
  if (rateLimitFactor == null || rateLimitFactor < 1 || rateLimitFactor > 8) return text("限流等待的间隔增长倍数要在 1 到 8 之间。", "Rate-limit wait growth must be between 1 and 8.")
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
