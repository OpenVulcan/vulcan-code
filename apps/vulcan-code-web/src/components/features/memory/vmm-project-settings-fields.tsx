import { useId } from "react"

import { Field, FieldLabel } from "@/ui-kit/primitives/field"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui-kit/primitives/select"
import type { TranslationKey } from "@/i18n"

// VmmSelectOption is one stable value and user-facing label.
// VmmSelectOption 是一个稳定值与面向用户的标签。
interface VmmSelectOption {
  value: string
  label: string
}

// VmmSelectFieldProps defines the compact select used by project-owned VMM overrides.
// VmmSelectFieldProps 定义项目级 VMM 覆盖使用的紧凑选择器。
interface VmmSelectFieldProps {
  label: string
  value: string
  disabled?: boolean
  options: VmmSelectOption[]
  onChange: (value: string) => void
}

// VmmSelectField renders one accessible shadcn select without importing obsolete management panels.
// VmmSelectField 渲染一个可访问的 shadcn 选择器，且不再引入已废弃的管理面板。
function VmmSelectField({
  disabled,
  label,
  onChange,
  options,
  value,
}: VmmSelectFieldProps) {
  // fieldId connects the shared field label to the exact shadcn trigger.
  // fieldId 将公共字段标签与准确的 shadcn 触发器关联。
  const fieldId = useId()

  return (
    <Field>
      <FieldLabel htmlFor={fieldId}>{label}</FieldLabel>
      <Select disabled={disabled} onValueChange={onChange} value={value}>
        <SelectTrigger className="w-full" id={fieldId}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Field>
  )
}

// vmmPolicyModeOptions returns the three exact project policy modes.
// vmmPolicyModeOptions 返回三个准确的项目策略模式。
function vmmPolicyModeOptions(
  t: (key: TranslationKey) => string,
): VmmSelectOption[] {
  return [
    { value: "inherit", label: t("vmm.settings.policy.inherit") },
    { value: "enabled", label: t("vmm.settings.policy.enabled") },
    { value: "disabled", label: t("vmm.settings.policy.disabled") },
  ]
}

export { VmmSelectField, vmmPolicyModeOptions }
export type { VmmSelectFieldProps, VmmSelectOption }
