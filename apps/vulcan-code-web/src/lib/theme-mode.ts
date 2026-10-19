import { getAppearance, resolvedDark } from "@/ui-kit/appearance";

// ThemeMode is the resolved light or dark rendering that legacy chrome controls still display.
// ThemeMode 是旧版外壳控件仍在显示的已解析明暗呈现。
export type ThemeMode = "light" | "dark";

// THEME_STORAGE_KEY is owned by the appearance store; re-exported for existing imports.
// THEME_STORAGE_KEY 归外观存储所有；为现有导入重新导出。
export { THEME_STORAGE_KEY } from "@/ui-kit/appearance";

// initialThemeMode resolves the stored theme choice (including "auto" and "command") to light or dark.
// initialThemeMode 将已存主题选择（含 "auto" 与 "command"）解析为明或暗。
export function initialThemeMode(): ThemeMode {
  return resolvedDark(getAppearance().theme) ? "dark" : "light";
}
