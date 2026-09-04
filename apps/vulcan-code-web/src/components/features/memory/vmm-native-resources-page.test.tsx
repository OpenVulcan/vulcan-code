import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { I18nProvider } from "@/i18n"

import { VmmNativeResourcesPage } from "./vmm-native-resources-page"

describe("VMM native resource settings", () => {
  // Verifies the native-only page exposes composition, ad-hoc PII formulas, and Noise testing.
  // 验证原生专属页面会公开资源组合、临时 PII 公式与噪音测试。
  it("renders the complete native resource maintenance surface", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <VmmNativeResourcesPage busy={false} onRunAction={async () => null} />
      </I18nProvider>,
    )

    expect(markup).toContain("原生资源与规则")
    expect(markup).toContain("PII 规则测试器")
    expect(markup).toContain("临时正则与 condition 公式测试")
    expect(markup).toContain("正则 pattern")
    expect(markup).toContain("条件 condition（可留空）")
    expect(markup).toContain("噪音规则测试器")
  })
})
