import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { FilterChip, FilterChipGroup, SegmentedControl, SegmentedControlItem } from "@/components/ui/choice-controls"
import { NavItem, TreeRow } from "@/components/ui/navigation-item"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"

describe("shared navigation and choice controls", () => {
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
