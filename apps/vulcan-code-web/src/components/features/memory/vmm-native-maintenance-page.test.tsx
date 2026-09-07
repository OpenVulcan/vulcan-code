import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { I18nProvider } from "@/i18n"

import { VmmNativeMaintenancePage } from "./vmm-native-maintenance-page"

describe("VMM native scheduled maintenance", () => {
  // Verifies the native-only page exposes periodic cleanup, vector generation, manual execution, and recycle-bin controls.
  // 验证原生专属页面公开定期清洗、向量代次、手工执行与回收站控制。
  it("renders the native retention maintenance surface", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <VmmNativeMaintenancePage busy={false} onRunAction={async () => null} />
      </I18nProvider>,
    )

    expect(markup).toContain("定期清洗")
    expect(markup).toContain("预览清洗")
    expect(markup).toContain("定期清洗状态")
    expect(markup).toContain("向量代次与重建")
    expect(markup).toContain("全文检索索引")
    expect(markup).toContain("重建全文索引")
    expect(markup).toContain("回收站")
    expect(markup).toContain("当前筛选范围没有回收批次")
  })
})
