import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { Alert } from "@/ui-kit/primitives/alert"
import { Badge } from "@/ui-kit/primitives/badge"
import { Card } from "@/ui-kit/primitives/card"
import { Empty } from "@/ui-kit/primitives/empty"
import { Surface } from "@/ui-kit/primitives/surface"

describe("shared content surfaces", () => {
  it("exposes semantic surface variants", () => {
    const markup = renderToStaticMarkup(<Surface variant="selected">Selected</Surface>)

    expect(markup).toContain('data-slot="surface"')
    expect(markup).toContain('data-variant="selected"')
  })

  it("lets cards and empty states select the same surface vocabulary", () => {
    const cardMarkup = renderToStaticMarkup(<Card variant="interactive">Card</Card>)
    const emptyMarkup = renderToStaticMarkup(<Empty variant="outlined">Empty</Empty>)

    expect(cardMarkup).toContain('data-variant="interactive"')
    expect(emptyMarkup).toContain('data-variant="outlined"')
  })

  it("keeps status and alert meanings explicit", () => {
    const successMarkup = renderToStaticMarkup(<Badge variant="success">Ready</Badge>)
    const warningMarkup = renderToStaticMarkup(<Alert variant="warning">Warning</Alert>)

    expect(successMarkup).toContain('data-variant="success"')
    expect(successMarkup).toContain('data-tone="ok"')
    expect(warningMarkup).toContain('data-tone="warn"')
  })
})
