import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { Input } from "@/components/ui/input"
import { Checkbox } from "@/components/ui/checkbox"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Select, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

describe("unified form controls", () => {
  // Keeps all three single-line densities available through one Input contract.
  // 通过同一个 Input 契约提供三档单行输入密度。
  it("renders the selected Input density", () => {
    expect(renderToStaticMarkup(<Input density="compact" />)).toContain('data-density="compact"')
    expect(renderToStaticMarkup(<Input />)).toContain('data-density="default"')
    expect(renderToStaticMarkup(<Input density="large" />)).toContain('data-density="large"')
  })

  // Uses only a one-pixel focus extension instead of the former heavy three-pixel ring.
  // 焦点态只允许一像素外扩，不再使用原先厚重的三像素外框。
  it("keeps the Input focus ring thin", () => {
    const markup = renderToStaticMarkup(<Input />)

    expect(markup).toContain("focus-visible:ring-1")
    expect(markup).not.toContain("focus-visible:ring-3")
  })

  // Keeps all three multiline densities available through one Textarea contract.
  // 通过同一个 Textarea 契约提供三档多行输入密度。
  it("renders the selected Textarea density", () => {
    expect(renderToStaticMarkup(<Textarea density="compact" />)).toContain('data-density="compact"')
    expect(renderToStaticMarkup(<Textarea />)).toContain('data-density="default"')
    expect(renderToStaticMarkup(<Textarea density="large" />)).toContain('data-density="large"')
  })

  // Keeps every shared selection control on the accepted one-pixel focus treatment.
  // 让全部共享选择控件保持已验收的一像素焦点外框。
  it("keeps selection-control focus rings thin", () => {
    const selectMarkup = renderToStaticMarkup(<Select><SelectTrigger><SelectValue /></SelectTrigger></Select>)
    const checkboxMarkup = renderToStaticMarkup(<Checkbox />)
    const switchMarkup = renderToStaticMarkup(<Switch />)
    const radioMarkup = renderToStaticMarkup(<RadioGroup><RadioGroupItem value="one" /></RadioGroup>)

    for (const markup of [selectMarkup, checkboxMarkup, switchMarkup, radioMarkup]) {
      expect(markup).toContain("focus-visible:ring-1")
      expect(markup).not.toContain("focus-visible:ring-3")
    }
  })

  // Exposes stable data slots so business forms reuse one visual contract.
  // 暴露稳定的数据槽位，使业务表单复用同一套视觉契约。
  it("renders shared selection-control slots", () => {
    expect(renderToStaticMarkup(<Checkbox checked />)).toContain('data-slot="checkbox"')
    expect(renderToStaticMarkup(<Switch checked />)).toContain('data-slot="switch"')
    expect(renderToStaticMarkup(<RadioGroup><RadioGroupItem value="one" /></RadioGroup>)).toContain('data-slot="radio-group-item"')
  })
})
