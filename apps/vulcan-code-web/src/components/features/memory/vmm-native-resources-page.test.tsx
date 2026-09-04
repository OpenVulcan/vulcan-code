import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { I18nProvider } from "@/i18n"

import { VmmNativeResourcesPage } from "./vmm-native-resources-page"

describe("VMM native resource settings", () => {
  // Verifies each user-facing category mounts its exact editor or validation tools.
  // 验证每个面向用户的分类挂载准确的编辑器或验证工具。
  it("renders the categorized native resource maintenance surface", () => {
    // renderGroup produces one deterministic server-rendered snapshot for a selected category.
    // renderGroup 为所选分类生成一份确定性的服务端渲染快照。
    const renderGroup = (initialGroup: "prompt" | "pii" | "noise") => renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <VmmNativeResourcesPage busy={false} initialGroup={initialGroup} onRunAction={async () => null} />
      </I18nProvider>,
    )

    const promptMarkup = renderGroup("prompt")
    expect(promptMarkup).toContain("原生资源与规则")
    expect(promptMarkup).toContain("提示词设置")
    expect(promptMarkup).toContain("资源文件编辑器")

    const piiMarkup = renderGroup("pii")
    expect(piiMarkup).toContain("PII 规则测试器")
    expect(piiMarkup).toContain("临时正则与 condition 公式测试")
    expect(piiMarkup).toContain("正则 pattern")
    expect(piiMarkup).toContain("条件 condition（可留空）")

    const noiseMarkup = renderGroup("noise")
    expect(noiseMarkup).toContain("噪音规则测试器")
    expect(noiseMarkup).toContain("用户提示词（不接收 system、assistant 或 tool 内容）")
  })
})
