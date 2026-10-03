// ThemeMode describes the two shadcn default theme modes.
// ThemeMode 描述 shadcn 默认的两种主题模式。
export type ThemeMode = "light" | "dark";

// THEME_STORAGE_KEY is the only local-storage key holding the user's explicit theme choice.
// THEME_STORAGE_KEY 是保存用户明确主题选择的唯一本地存储键。
export const THEME_STORAGE_KEY = "vulcan-webui-theme";

// initialThemeMode returns the persisted shadcn theme, defaulting to light.
// initialThemeMode 返回持久化的 shadcn 主题，默认使用亮色。
export function initialThemeMode(): ThemeMode {
  try {
    // Storage may be unavailable in restricted browser contexts; rendering must remain usable.
    // 受限浏览器环境可能不提供存储权限，界面仍须保持可用。
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    return stored === "light" || stored === "dark" ? stored : "light";
  } catch {
    return "light";
  }
}

// applyThemeMode publishes the theme on the document root. The boot entry calls it before the first paint so the
// startup screen already wears the persisted theme; the workbench shell calls it again whenever the theme changes.
// applyThemeMode 将主题发布到文档根节点。启动入口在首帧前调用，使启动界面即以持久化主题呈现；
// 工作台外壳在主题变化时再次调用。
export function applyThemeMode(theme: ThemeMode): void {
  document.documentElement.classList.toggle("dark", theme === "dark");
  document.documentElement.style.colorScheme = theme;
}
