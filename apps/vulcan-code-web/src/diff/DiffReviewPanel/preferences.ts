import type { DiffRenderPreferences, DiffViewMode } from "../types";
// DIFF_RENDER_PREFERENCES_STORAGE_KEY stores WebUI diff display preference state.
// DIFF_RENDER_PREFERENCES_STORAGE_KEY 保存 WebUI diff 显示偏好状态。
const DIFF_RENDER_PREFERENCES_STORAGE_KEY = "vulcan-webui-diff-render-preferences";

// DEFAULT_DIFF_RENDER_PREFERENCES provides target-compatible first-run rendering switches.
// DEFAULT_DIFF_RENDER_PREFERENCES 提供首次运行时的目标兼容渲染开关。
const DEFAULT_DIFF_RENDER_PREFERENCES: DiffRenderPreferences = {
  overflowMode: "scroll",
  inlineDiff: true,
  hunkSeparatorStyle: "line-info",
  indicatorStyle: "bars",
};

// DEFAULT_DIFF_VIEW_MODE provides the first-run diff layout before persistence exists.
// DEFAULT_DIFF_VIEW_MODE 提供持久化数据不存在时的首次 diff 布局。
const DEFAULT_DIFF_VIEW_MODE: DiffViewMode = "unified";

// DiffRenderPreferenceStorage stores persisted display switches including the visual layout.
// DiffRenderPreferenceStorage 保存包含视觉布局在内的持久化显示开关。
type DiffRenderPreferenceStorage = Partial<DiffRenderPreferences> & {
  // Persisted visual layout mode.
  // 持久化的视觉布局模式。
  viewMode?: DiffViewMode;
};

// initialDiffRenderPreferences returns persisted diff render preferences with schema validation.
// initialDiffRenderPreferences 返回经过结构校验的持久化 diff 渲染偏好。
export function initialDiffRenderPreferences(): DiffRenderPreferences {
  if (typeof window === "undefined") {
    return DEFAULT_DIFF_RENDER_PREFERENCES;
  }
  try {
    const stored = window.localStorage.getItem(DIFF_RENDER_PREFERENCES_STORAGE_KEY);
    return normalizeDiffRenderPreferences(stored ? JSON.parse(stored) : null);
  } catch {
    return DEFAULT_DIFF_RENDER_PREFERENCES;
  }
}

// initialDiffViewMode returns the persisted visual diff layout mode with schema validation.
// initialDiffViewMode 返回经过结构校验的持久化 diff 视觉布局模式。
export function initialDiffViewMode(): DiffViewMode {
  if (typeof window === "undefined") {
    return DEFAULT_DIFF_VIEW_MODE;
  }
  try {
    const stored = window.localStorage.getItem(DIFF_RENDER_PREFERENCES_STORAGE_KEY);
    const value = stored ? JSON.parse(stored) as DiffRenderPreferenceStorage : null;
    return normalizeDiffViewMode(value?.viewMode);
  } catch {
    return DEFAULT_DIFF_VIEW_MODE;
  }
}

// persistDiffDisplayPreferences stores the latest render preferences and visual layout together.
// persistDiffDisplayPreferences 将最新渲染偏好与视觉布局一起保存。
export function persistDiffDisplayPreferences(viewMode: DiffViewMode, preferences: DiffRenderPreferences): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(DIFF_RENDER_PREFERENCES_STORAGE_KEY, JSON.stringify({ ...preferences, viewMode }));
  } catch {
    // Ignore quota or privacy-mode failures because rendering can continue with in-memory state.
    // 忽略容量或隐私模式失败，因为渲染仍可使用内存状态继续。
  }
}

// normalizeDiffRenderPreferences upgrades unknown storage data into a complete supported preference object.
// normalizeDiffRenderPreferences 将未知存储数据升级为完整且受支持的偏好对象。
function normalizeDiffRenderPreferences(candidate: unknown): DiffRenderPreferences {
  const value = typeof candidate === "object" && candidate !== null
    ? candidate as Partial<Record<keyof DiffRenderPreferences, unknown>>
    : {};
  return {
    overflowMode: value.overflowMode === "wrap" || value.overflowMode === "scroll"
      ? value.overflowMode
      : DEFAULT_DIFF_RENDER_PREFERENCES.overflowMode,
    inlineDiff: typeof value.inlineDiff === "boolean"
      ? value.inlineDiff
      : DEFAULT_DIFF_RENDER_PREFERENCES.inlineDiff,
    hunkSeparatorStyle: value.hunkSeparatorStyle === "simple" || value.hunkSeparatorStyle === "line-info"
      ? value.hunkSeparatorStyle
      : DEFAULT_DIFF_RENDER_PREFERENCES.hunkSeparatorStyle,
    indicatorStyle: value.indicatorStyle === "classic" || value.indicatorStyle === "bars"
      ? value.indicatorStyle
      : DEFAULT_DIFF_RENDER_PREFERENCES.indicatorStyle,
  };
}

// normalizeDiffViewMode upgrades unknown storage data into a supported visual layout mode.
// normalizeDiffViewMode 将未知存储数据升级为受支持的视觉布局模式。
function normalizeDiffViewMode(candidate: unknown): DiffViewMode {
  return candidate === "split" || candidate === "unified"
    ? candidate
    : DEFAULT_DIFF_VIEW_MODE;
}

