import { useI18n } from "@/i18n"

// useMcpText follows the application's current locale for MCP-only display literals.
// useMcpText 按应用当前语言选择 MCP 专属展示文案。
export function useMcpText(): (zh: string, en: string) => string {
  // locale is owned by the shared application internationalization provider.
  // locale 由共享应用国际化提供器拥有。
  const { locale } = useI18n()
  return (zh, en) => locale === "zh-CN" ? zh : en
}
