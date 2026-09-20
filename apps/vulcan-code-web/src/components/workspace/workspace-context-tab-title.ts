import type { TranslationKey } from "@/i18n";
import type { WorkspaceContextTabKind } from "@/components/workspace/WorkspaceContextPanel";

// workspaceContextTabDefaultTitle returns the locale-aware default title for one context tab kind.
// workspaceContextTabDefaultTitle 返回单个上下文页签类型的本地化默认标题。
// Parameters: kind is the registered tab kind, and t optionally resolves a translation key.
// 参数：kind 是已注册的页签类型，t 可选地解析翻译键。
// Returns: the translated title or an English fallback when no translator is supplied.
// 返回值：返回翻译后的标题；未提供翻译器时返回英文默认标题。
export function workspaceContextTabDefaultTitle(kind: WorkspaceContextTabKind, t?: (key: TranslationKey) => string): string {
  if (kind === "local-browser") {
    return t ? t("workspaceContext.localBrowser") : "Local Browser";
  }
  if (kind === "browser") {
    return t ? `AI ${t("area.browser")}` : "AI Browser";
  }
  if (kind === "subagent") {
    return t ? t("conversation.subagent.schedule") : "Subagent";
  }
  if (kind === "files") {
    return t ? t("workspaceFilePreview.openFile") : "Open file";
  }
  if (kind === "memory") {
    return t ? t("contextPanel.memory") : "Memory";
  }
  return t ? t("contextPanel.review") : "Review";
}
