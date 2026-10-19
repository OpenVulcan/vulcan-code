import * as React from "react"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { useI18n, type TranslationKey } from "@/i18n"

// Units are presentation choices; persisted policy always has a single integer-millisecond owner.
// 单位仅为展示选择；持久策略始终以单一整数毫秒值为权威。
const units = [
  { value: "day", factor: 86400000n, label: "memory.policy.days" },
  { value: "hour", factor: 3600000n, label: "memory.policy.hours" },
  { value: "minute", factor: 60000n, label: "memory.policy.minutes" },
  { value: "second", factor: 1000n, label: "memory.policy.seconds" },
  { value: "millisecond", factor: 1n, label: "memory.policy.milliseconds" },
] as const satisfies ReadonlyArray<{ value: string; factor: bigint; label: TranslationKey }>

// Converts a visible decimal quantity and declared unit to exact milliseconds; invalid input stays invalid.
// 将可见十进制数量及声明单位转换为准确毫秒；无效输入保持无效。
// Returns an integer string, empty input, or an invalid marker rejected by the policy validator.
// 返回整数字符串、空输入，或由策略校验拒绝的无效标记。
export function nativeDurationMilliseconds(quantity: string, unit: typeof units[number]["value"]): string {
  if (quantity === "") return ""
  if (quantity.length > 32 || !/^\d+(?:\.\d*)?$/.test(quantity)) return "invalid"
  const parts = quantity.split(".")
  const fraction = parts.length === 2 ? parts[1] : ""
  const denominator = 10n ** BigInt(fraction.length)
  const factor = units.find((entry) => entry.value === unit)!.factor
  const scaled = BigInt(parts[0] + fraction) * factor
  return scaled % denominator === 0n ? String(scaled / denominator) : "invalid"
}

// Selects the largest exact integral unit for a newly loaded value without rounding or rewriting it.
// 为新加载值选择最大的准确整数单位，不进行舍入或回写。
function displayDuration(value: string): { quantity: string; unit: typeof units[number]["value"] } {
  if (!/^\d+$/.test(value)) return { quantity: value, unit: "millisecond" }
  const milliseconds = BigInt(value)
  const unit = units.find((entry) => milliseconds % entry.factor === 0n)!
  return { quantity: String(milliseconds / unit.factor), unit: unit.value }
}

// DurationInputProps carries one policy field; the parent owns validation and explicit persistence.
// DurationInputProps 携带一个策略字段；父级负责校验及显式持久化。
interface DurationInputProps {
  id: string
  value: string
  disabled: boolean
  // Optional field-specific unit name distinguishes multiple duration controls on one page.
  // 可选的字段专属单位名称用于区分同页的多个时长控件。
  unitLabel?: string
  onChange: (milliseconds: string) => void
}

// Edits amount and unit while preserving intermediate text; changing a unit intentionally changes duration.
// 编辑数量及单位并保留中间文本；修改单位有意改变有效期。
export function VmmNativeDurationInput({ id, value, disabled, unitLabel, onChange }: DurationInputProps) {
  const { t } = useI18n()
  const [display, setDisplay] = React.useState(() => displayDuration(value))
  // Own emitted values must not reformat the user's partially typed decimal quantity.
  // 自己发出的值不能重新格式化用户尚未输入完整的小数数量。
  const emitted = React.useRef(value)
  React.useEffect(() => {
    if (value !== emitted.current) { emitted.current = value; setDisplay(displayDuration(value)) }
  }, [value])
  // Emits exact milliseconds to the policy draft without saving or clamping the user's value.
  // 向策略草稿发送准确毫秒，不保存或截断用户值。
  const update = (next: typeof display) => {
    setDisplay(next)
    emitted.current = nativeDurationMilliseconds(next.quantity, next.unit)
    onChange(emitted.current)
  }
  return <div className="flex gap-2">
    <Input className="min-w-0 flex-1" id={id} inputMode="decimal" maxLength={32} disabled={disabled} value={display.quantity} onChange={(event) => update({ ...display, quantity: event.target.value })} />
    <Select disabled={disabled} value={display.unit} onValueChange={(unit) => update({ ...display, unit: unit as typeof display.unit })}>
      <SelectTrigger className="w-24 shrink-0" aria-label={unitLabel ?? t("memory.policy.durationUnit")}><SelectValue /></SelectTrigger>
      <SelectContent>{units.map((unit) => <SelectItem key={unit.value} value={unit.value}>{t(unit.label)}</SelectItem>)}</SelectContent>
    </Select>
  </div>
}
