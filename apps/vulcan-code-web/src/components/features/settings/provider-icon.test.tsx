import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"

import { ProviderIcon, resolveProviderIconKey } from "@/components/features/settings/provider-icon"

// Provider artwork resolution preserves family, product, theme, and fallback behavior.
// 供应商图标解析保留系列、产品、主题与回退行为。
describe("provider icon library", () => {
  // Built-in providers resolve their exact family artwork.
  // 内置供应商解析到各自精确的系列图标。
  it("resolves every implemented provider family", () => {
    expect(resolveProviderIconKey({ providerId: "minimax" })).toBe("minimax")
    expect(resolveProviderIconKey({ providerId: "agentspace" })).toBe("agentspace")
    expect(resolveProviderIconKey({ providerId: "openrouter" })).toBe("openrouter")
    expect(resolveProviderIconKey({ providerId: "openai" })).toBe("compatibility")
    expect(resolveProviderIconKey({ providerId: "anthropic" })).toBe("claude")
    expect(resolveProviderIconKey({ providerId: "google" })).toBe("gemini")
    expect(resolveProviderIconKey({ providerId: "apikeyfan" })).toBe("apikeyfan")
    expect(resolveProviderIconKey({ providerId: "claudeapi" })).toBe("claudeapi")
    expect(resolveProviderIconKey({ providerId: "qiniu" })).toBe("qiniu")
    expect(resolveProviderIconKey({ providerId: "stepfun" })).toBe("stepfun")
    expect(resolveProviderIconKey({ providerId: "fennoai" })).toBe("fennoai")
    expect(resolveProviderIconKey({ providerId: "code0" })).toBe("code0")
    expect(resolveProviderIconKey({ providerId: "commandcode" })).toBe("commandcode")
    expect(resolveProviderIconKey({ providerId: "opencode-go" })).toBe("opencode-go")
    expect(resolveProviderIconKey({ providerId: "tokenrhythm" })).toBe("tokenrhythm")
    expect(resolveProviderIconKey({ providerId: "openvulcan" })).toBe("openvulcan")
  })

  // Product artwork overrides its provider-family default.
  // 产品图标覆盖其供应商系列默认图标。
  it("prefers exact service-entry artwork", () => {
    expect(resolveProviderIconKey({
      providerId: "openai",
      serviceEntryId: "codex-subscription",
    })).toBe("codex")
  })

  // Unknown custom providers use the reviewed compatibility artwork.
  // 未知自定义供应商使用已审核的兼容图标。
  it("falls back to compatibility artwork", () => {
    expect(resolveProviderIconKey({ providerId: "custom:example" })).toBe("compatibility")
  })

  // OpenCode Go keeps its supplied monochrome artwork visible in dark theme.
  // OpenCode Go 在暗色主题中保持用户提供的单色图标可见。
  it("renders OpenCode Go with dark-theme monochrome inversion", () => {
    // markup is the exact server-rendered provider icon element.
    // markup 是精确的服务端渲染供应商图标元素。
    const markup = renderToStaticMarkup(<ProviderIcon providerId="opencode-go" />)

    expect(markup).toContain('data-provider-icon="opencode-go"')
    expect(markup).toContain("data-mono=\"true\"")
  })

  // Token Rhythm uses the supplied monochrome artwork and remains visible in dark theme.
  // Token Rhythm 使用用户提供的单色图标，并在暗色主题中保持可见。
  it("renders Token Rhythm with dark-theme monochrome inversion", () => {
    const markup = renderToStaticMarkup(<ProviderIcon providerId="tokenrhythm" />)

    expect(markup).toContain('data-provider-icon="tokenrhythm"')
    expect(markup).toContain("data-mono=\"true\"")
  })

  // StepFun's white supplied artwork uses a contrasting tile and dark-theme inversion.
  // 阶跃星辰提供的白色图标使用对比底板，并在暗色主题下反色保持可见。
  it("renders StepFun artwork on contrasting theme surfaces", () => {
    // markup is the exact server-rendered StepFun icon element.
    // markup 是服务端渲染得到的阶跃星辰图标元素。
    const markup = renderToStaticMarkup(<ProviderIcon providerId="stepfun" />)

    expect(markup).toContain('data-provider-icon="stepfun"')
    expect(markup).toContain("data-surface=\"theme\"")
    expect(markup).toContain("data-mono=\"true\"")
  })

  // Relay artwork owns one visible and accessible marker independent of the provider name.
  // 中转站图标拥有一个独立于供应商名称的可见且可访问标识。
  it("renders the independent relay marker only for relay providers", () => {
    const relayMarkup = renderToStaticMarkup(<ProviderIcon category="relay" providerId="claudeapi" />)
    const cloudMarkup = renderToStaticMarkup(<ProviderIcon category="cloud_maas" providerId="qiniu" />)

    expect(relayMarkup).toContain('data-provider-category="relay"')
    expect(relayMarkup).toContain('aria-label="Relay provider / 中转站"')
    expect(relayMarkup).toContain('role="img"')
    expect(relayMarkup).toContain(">R</span>")
    expect(cloudMarkup).not.toContain("Relay provider / 中转站")
    expect(cloudMarkup).not.toContain(">R</span>")
  })
})
