import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { FilterChip, FilterChipGroup, SegmentedControl, SegmentedControlItem } from "@/components/ui/choice-controls"
import { NavItem, TreeRow } from "@/components/ui/navigation-item"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"

describe("shared navigation and choice controls", () => {
  // Vertical appearance must also select the primitive's matching accessibility and keyboard orientation.
  // 垂直外观必须同步选择底层组件对应的无障碍与键盘操作方向。
  it("forwards vertical orientation into the tab-list interaction contract", () => {
    // markup exposes the primitive-owned orientation, rather than only our root's decorative data attribute.
    // markup 暴露底层组件持有的方向，而非仅验证外层装饰性数据属性。
    const markup = renderToStaticMarkup(<Tabs orientation="vertical" defaultValue="one"><TabsList><TabsTrigger value="one">One</TabsTrigger></TabsList></Tabs>);
    expect(markup).toContain('aria-orientation="vertical"');
    expect(markup).not.toContain('aria-orientation="horizontal"');
  });
  it("renders exclusive segmented choices with a selected value", () => {
    const markup = renderToStaticMarkup(
      <SegmentedControl defaultValue="balanced">
        <SegmentedControlItem value="compact">Compact</SegmentedControlItem>
        <SegmentedControlItem value="balanced">Balanced</SegmentedControlItem>
      </SegmentedControl>,
    )

    expect(markup).toContain('data-slot="segmented-control"')
    expect(markup).toContain('data-state="on"')
  })

  it("exposes explicit filter and navigation selection states", () => {
    const filterMarkup = renderToStaticMarkup(
      <FilterChipGroup aria-label="Status"><FilterChip selected>Active</FilterChip></FilterChipGroup>,
    )
    const navMarkup = renderToStaticMarkup(<NavItem selected>Current</NavItem>)
    const treeMarkup = renderToStaticMarkup(<TreeRow depth={2}>Nested</TreeRow>)

    expect(filterMarkup).toContain('aria-pressed="true"')
    expect(navMarkup).toContain('aria-current="page"')
    expect(treeMarkup).toContain('role="treeitem"')
    expect(treeMarkup).toContain('padding-inline-start:40px')
  })

  it("keeps content tabs on the shared one-pixel focus treatment", () => {
    const markup = renderToStaticMarkup(
      <Tabs defaultValue="one"><TabsList><TabsTrigger value="one">One</TabsTrigger></TabsList></Tabs>,
    )

    expect(markup).toContain('data-slot="tabs-trigger"')
    expect(markup).toContain("focus-visible:ring-1")
    expect(markup).not.toContain("focus-visible:ring-[3px]")
  })
})
