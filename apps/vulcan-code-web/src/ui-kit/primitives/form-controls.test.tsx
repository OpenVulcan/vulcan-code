import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { Input } from "@/ui-kit/primitives/input"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { RadioGroup, RadioGroupItem } from "@/ui-kit/primitives/radio-group"
import { Select, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { Textarea } from "@/ui-kit/primitives/textarea"

describe("unified form controls", () => {
  // Keeps all three single-line densities available through one Input contract.
  // 通过同一个 Input 契约提供三档单行输入密度。
  it("renders the selected Input density", () => {
    expect(renderToStaticMarkup(<Input density="compact" />)).toContain('data-density="compact"')
    expect(renderToStaticMarkup(<Input />)).toContain('data-density="default"')
    expect(renderToStaticMarkup(<Input density="large" />)).toContain('data-density="large"')
  })

  // Keeps all three multiline densities available through one Textarea contract.
  // 通过同一个 Textarea 契约提供三档多行输入密度。
  it("renders the selected Textarea density", () => {
    expect(renderToStaticMarkup(<Textarea density="compact" />)).toContain('data-density="compact"')
    expect(renderToStaticMarkup(<Textarea />)).toContain('data-density="default"')
    expect(renderToStaticMarkup(<Textarea density="large" />)).toContain('data-density="large"')
  })

  // Exposes the select trigger as a combobox so assistive technology announces the closed list.
  // 将选择触发器暴露为组合框，使辅助技术能播报收起的列表。
  it("renders the select trigger as a collapsed combobox", () => {
    const markup = renderToStaticMarkup(<Select><SelectTrigger><SelectValue /></SelectTrigger></Select>)
    expect(markup).toContain('role="combobox"')
    expect(markup).toContain('aria-expanded="false"')
  })

  // Exposes stable data slots so business forms reuse one visual contract.
  // 暴露稳定的数据槽位，使业务表单复用同一套视觉契约。
  it("renders shared selection-control slots", () => {
    expect(renderToStaticMarkup(<Checkbox checked />)).toContain('data-slot="checkbox"')
    expect(renderToStaticMarkup(<Switch checked />)).toContain('data-slot="switch"')
    expect(renderToStaticMarkup(<RadioGroup><RadioGroupItem value="one" /></RadioGroup>)).toContain('data-slot="radio-group-item"')
  })
})
