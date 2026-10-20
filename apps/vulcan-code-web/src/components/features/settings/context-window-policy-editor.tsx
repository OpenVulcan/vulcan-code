import * as React from "react"

import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { useI18n } from "@/i18n"
import type { PublicAiContextWindowPolicy, PublicAiContextWindowPolicyOverrides } from "@/types"
import "@/ui-kit/styles/features/settings-forms.css"

// NumericPolicyField names the sparse policy fields edited as plain integers.
// NumericPolicyField 命名以纯整数编辑的稀疏策略字段。
type NumericPolicyField = "effective_context_window_percent" | "auto_compact_percent" | "auto_compact_token_limit" | "fallback_buffer_tokens"

// ContextWindowPolicyEditor edits the global sparse window policy as label-left, control-right rows; empty fields use
// built-in values, which they show in place so an untouched field never reads as blank.
// ContextWindowPolicyEditor 以标签在左、控件在右的行编辑全局稀疏窗口策略；空字段使用系统内置值，
// 并在原位显示该值，使未改动的字段不会显示为空白。
export function ContextWindowPolicyEditor(props: {
  onChange: (value: PublicAiContextWindowPolicyOverrides | undefined) => void
  value?: PublicAiContextWindowPolicyOverrides
  // Optional source-specific inherited values shown under a field while it is left empty.
  // 可选的来源专属继承值，字段留空时显示在其下方。
  inheritanceHints?: Partial<Record<keyof PublicAiContextWindowPolicyOverrides, string>>
  // Built-in values from the service, shown in every field left empty.
  // 来自服务的内置值，显示在每个留空字段中。
  defaults?: PublicAiContextWindowPolicy
  // Locks the complete editor while its owning section persists an immutable draft.
  // 所属分区持久化不可变草稿时锁定完整编辑器。
  disabled?: boolean
}) {
  const { t } = useI18n()
  // updateNumber preserves the sparse shape while parsing only safe integer input.
  // updateNumber 保持稀疏结构，并且只解析安全整数输入。
  const updateNumber = React.useCallback((field: NumericPolicyField, rawValue: string) => {
    const next: PublicAiContextWindowPolicyOverrides = { ...(props.value ?? {}) }
    const trimmed = rawValue.trim()
    if (!trimmed) {
      delete next[field]
    } else {
      const parsed = Number.parseInt(trimmed, 10)
      if (Number.isSafeInteger(parsed)) {
        next[field] = parsed
      }
    }
    props.onChange(Object.keys(next).length > 0 ? next : undefined)
  }, [props])
  // updateScope changes only the optional usage-scope field.
  // updateScope 只修改可选的用量范围字段。
  const updateScope = React.useCallback((rawValue: string) => {
    const next: PublicAiContextWindowPolicyOverrides = { ...(props.value ?? {}) }
    if (rawValue === "") {
      delete next.auto_compact_scope
    } else {
      next.auto_compact_scope = rawValue as PublicAiContextWindowPolicyOverrides["auto_compact_scope"]
    }
    props.onChange(Object.keys(next).length > 0 ? next : undefined)
  }, [props])
  // hint shows the inherited value only while the field has no explicit override.
  // hint 仅在字段没有显式覆盖时显示继承值。
  const hint = (field: keyof PublicAiContextWindowPolicyOverrides) =>
    props.value?.[field] === undefined && props.inheritanceHints?.[field]
      ? <small className="vk-form-note">{props.inheritanceHints[field]}</small>
      : null
  // numberRow renders one integer field; min and max mirror the backend validation bounds.
  // numberRow 渲染一个整数字段；min 与 max 对应后台校验边界。
  // placeholderOf shows the built-in value; an unset token limit reads as "by percentage" instead of a blank.
  // placeholderOf 显示内置值；未设置的 Token 上限显示为“按比例”而非空白。
  const placeholderOf = (field: NumericPolicyField) => {
    const value = props.defaults?.[field]
    if (value === undefined) return undefined
    return value === null ? t("settings.contextWindowPolicy.byPercent") : String(value)
  }
  const scopeLabel = (scope: PublicAiContextWindowPolicy["auto_compact_scope"]) =>
    t(scope === "total" ? "settings.contextWindowPolicy.scopeTotal" : "settings.contextWindowPolicy.scopeBodyAfterPrefix")
  const numberRow = (field: NumericPolicyField, label: string, min: number, max?: number) => (
    <label className="vk-form-pref" key={field}>
      <span className="vk-form-pref-text"><strong>{label}</strong>{hint(field)}</span>
      <Input
        className="vk-form-w-sm"
        inputMode="numeric"
        max={max}
        min={min}
        onChange={(event) => updateNumber(field, event.target.value)}
        placeholder={placeholderOf(field)}
        type="number"
        value={props.value?.[field] ?? ""}
      />
    </label>
  )
  return (
    <fieldset className="vk-form-prefs context-basics-rows" disabled={props.disabled}>
      {numberRow("effective_context_window_percent", t("settings.contextWindowPolicy.effectivePercent"), 1, 100)}
      {numberRow("auto_compact_percent", t("settings.contextWindowPolicy.autoCompactPercent"), 1, 100)}
      {numberRow("auto_compact_token_limit", t("settings.contextWindowPolicy.autoCompactTokenLimit"), 1)}
      <div className="vk-form-pref">
        <span className="vk-form-pref-text"><strong>{t("settings.contextWindowPolicy.scope")}</strong>{hint("auto_compact_scope")}</span>
        <Select
          disabled={props.disabled}
          onValueChange={(value) => updateScope(value === "inherit" ? "" : value)}
          value={props.value?.auto_compact_scope ?? "inherit"}
        >
          <SelectTrigger aria-label={t("settings.contextWindowPolicy.scope")} className="vk-form-w-lg"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="inherit">{props.defaults
              ? `${t("settings.contextWindowPolicy.scopeInherit")} · ${scopeLabel(props.defaults.auto_compact_scope)}`
              : t("settings.contextWindowPolicy.scopeInherit")}</SelectItem>
            <SelectItem value="total">{t("settings.contextWindowPolicy.scopeTotal")}</SelectItem>
            <SelectItem value="body_after_prefix">{t("settings.contextWindowPolicy.scopeBodyAfterPrefix")}</SelectItem>
          </SelectContent>
        </Select>
      </div>
      {numberRow("fallback_buffer_tokens", t("settings.contextWindowPolicy.fallbackBuffer"), 0)}
    </fieldset>
  )
}
