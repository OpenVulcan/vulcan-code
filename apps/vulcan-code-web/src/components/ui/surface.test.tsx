import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { Alert } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Card } from "@/components/ui/card"
import { Empty } from "@/components/ui/empty"
import { Surface } from "@/components/ui/surface"

describe("shared content surfaces", () => {
  it("exposes semantic surface variants", () => {
    const markup = renderToStaticMarkup(<Surface variant="selected">Selected</Surface>)

    expect(markup).toContain('data-slot="surface"')
    expect(markup).toContain('data-variant="selected"')
    expect(markup).toContain("bg-accent/55")
  })

  it("lets cards and empty states select the same surface vocabulary", () => {
    const cardMarkup = renderToStaticMarkup(<Card variant="interactive">Card</Card>)
    const emptyMarkup = renderToStaticMarkup(<Empty variant="outlined">Empty</Empty>)

    expect(cardMarkup).toContain('data-variant="interactive"')
    expect(cardMarkup).toContain("hover:bg-accent/40")
    expect(emptyMarkup).toContain('data-variant="outlined"')
    expect(emptyMarkup).toContain("border-border")
  })

  it("keeps status and alert meanings explicit", () => {
    const successMarkup = renderToStaticMarkup(<Badge variant="success">Ready</Badge>)
    const warningMarkup = renderToStaticMarkup(<Alert variant="warning">Warning</Alert>)

    expect(successMarkup).toContain('data-variant="success"')
    expect(successMarkup).toContain("--vcn-content-success")
    expect(warningMarkup).toContain("--vcn-content-warning")
  })
})
