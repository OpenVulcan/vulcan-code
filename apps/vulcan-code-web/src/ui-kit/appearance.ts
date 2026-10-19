import React from "react";

// Appearance preferences are local display choices: theme, text tier, motion and conversation width.
// Each one is a single attribute on the document root that the kit stylesheets read.
// 外观偏好是本地显示选择：主题、字号档位、动效与会话宽度。每项都是文档根节点上的一个属性，由 kit 样式读取。

// AppearanceTheme is the chosen theme; "auto" follows the operating system.
// AppearanceTheme 是所选主题；"auto" 跟随操作系统。
export type AppearanceTheme = "auto" | "light" | "dark" | "command";

// AppearanceText is one of the five UI text tiers; the whole layout scales from it.
// AppearanceText 是五档界面字号之一；整体版面随之等比缩放。
export type AppearanceText = "xs" | "s" | "m" | "l" | "xl";

// AppearanceMotion lets the user reduce motion regardless of the system setting.
// AppearanceMotion 允许用户在系统设置之外单独减少动效。
export type AppearanceMotion = "system" | "reduce";

// AppearanceWidth is the conversation reading width: four limited presets or the full stage width.
// AppearanceWidth 是会话阅读宽度：四档限宽或占满舞台宽度。
export type AppearanceWidth = "compact" | "standard" | "wide" | "xwide" | "full";

// AppearancePreferences is the complete local appearance state.
// AppearancePreferences 是完整的本地外观状态。
export interface AppearancePreferences {
  theme: AppearanceTheme;
  text: AppearanceText;
  motion: AppearanceMotion;
  width: AppearanceWidth;
}

// APPEARANCE_THEMES, APPEARANCE_TEXT_TIERS and APPEARANCE_WIDTHS list the accepted values in display order.
// APPEARANCE_THEMES、APPEARANCE_TEXT_TIERS 与 APPEARANCE_WIDTHS 按显示顺序列出可接受的取值。
export const APPEARANCE_THEMES: readonly AppearanceTheme[] = ["auto", "light", "dark", "command"];
export const APPEARANCE_TEXT_TIERS: readonly AppearanceText[] = ["xs", "s", "m", "l", "xl"];
export const APPEARANCE_WIDTHS: readonly AppearanceWidth[] = ["compact", "standard", "wide", "xwide", "full"];

// THEME_STORAGE_KEY keeps the key the earlier light/dark toggle used, so an existing choice still applies.
// THEME_STORAGE_KEY 沿用早期明暗切换的键名，使已有选择继续生效。
export const THEME_STORAGE_KEY = "vulcan-webui-theme";

// APPEARANCE_STORAGE_KEYS maps the remaining preferences to their own local storage keys.
// APPEARANCE_STORAGE_KEYS 将其余偏好映射到各自的本地存储键。
const APPEARANCE_STORAGE_KEYS = {
  text: "vulcan-webui-text-tier",
  motion: "vulcan-webui-motion",
  width: "vulcan-webui-conversation-width",
} as const;

// DEFAULT_APPEARANCE is used for any preference that was never chosen.
// DEFAULT_APPEARANCE 用于从未选择过的偏好。
export const DEFAULT_APPEARANCE: AppearancePreferences = {
  theme: "auto",
  text: "s",
  motion: "system",
  width: "standard",
};

// readStored returns one stored value when it is one of the accepted values.
// readStored 在存储值属于可接受取值时返回该值。
function readStored<T extends string>(key: string, accepted: readonly T[], fallback: T): T {
  try {
    const stored = window.localStorage.getItem(key);
    return stored !== null && (accepted as readonly string[]).includes(stored) ? (stored as T) : fallback;
  } catch {
    return fallback;
  }
}

// readAppearance loads every preference from local storage.
// readAppearance 从本地存储读取全部偏好。
function readAppearance(): AppearancePreferences {
  return {
    theme: readStored(THEME_STORAGE_KEY, APPEARANCE_THEMES, DEFAULT_APPEARANCE.theme),
    text: readStored(APPEARANCE_STORAGE_KEYS.text, APPEARANCE_TEXT_TIERS, DEFAULT_APPEARANCE.text),
    motion: readStored(APPEARANCE_STORAGE_KEYS.motion, ["system", "reduce"] as const, DEFAULT_APPEARANCE.motion),
    width: readStored(APPEARANCE_STORAGE_KEYS.width, APPEARANCE_WIDTHS, DEFAULT_APPEARANCE.width),
  };
}

// darkQuery reports the operating-system color scheme for the "auto" theme.
// darkQuery 为 "auto" 主题报告操作系统配色。
function darkQuery(): MediaQueryList | null {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;
}

// resolvedDark reports whether the chosen theme currently renders dark.
// resolvedDark 报告所选主题当前是否以深色呈现。
export function resolvedDark(theme: AppearanceTheme): boolean {
  if (theme === "auto") return darkQuery()?.matches ?? false;
  return theme !== "light";
}

// applyAppearance writes the root attributes and the native color-scheme the resolved theme implies.
// applyAppearance 写入根节点属性及解析后主题对应的原生 color-scheme。
export function applyAppearance(preferences: AppearancePreferences): void {
  const root = document.documentElement;
  if (preferences.theme === "auto") delete root.dataset.vkTheme;
  else root.dataset.vkTheme = preferences.theme;
  root.dataset.vkText = preferences.text;
  root.dataset.vkWidth = preferences.width;
  if (preferences.motion === "reduce") root.dataset.vkMotion = "reduce";
  else delete root.dataset.vkMotion;
  const dark = resolvedDark(preferences.theme);
  root.dataset.vkScheme = dark ? "dark" : "light";
  root.style.colorScheme = dark ? "dark" : "light";
}

// current is the in-memory authority after the first read; listeners re-render on change.
// current 在首次读取后作为内存权威；变更时通知监听者重新渲染。
let current: AppearancePreferences | null = null;
const listeners = new Set<() => void>();

// getAppearance returns the current preferences, reading storage once.
// getAppearance 返回当前偏好，仅读取一次存储。
export function getAppearance(): AppearancePreferences {
  if (current === null) current = readAppearance();
  return current;
}

// initializeAppearance applies the stored preferences at boot and follows system theme changes for "auto".
// initializeAppearance 在启动时套用已存偏好，并在 "auto" 时跟随系统主题变化。
export function initializeAppearance(): void {
  applyAppearance(getAppearance());
  darkQuery()?.addEventListener("change", () => {
    if (getAppearance().theme === "auto") applyAppearance(getAppearance());
  });
}

// setAppearance applies one change immediately and persists only that explicit choice.
// Returns false when storage refused the write; the change still applies to this window.
// setAppearance 立即套用一项变更，并只持久化这项明确选择。存储拒绝写入时返回 false，变更仍在当前窗口生效。
export function setAppearance<K extends keyof AppearancePreferences>(key: K, value: AppearancePreferences[K]): boolean {
  current = { ...getAppearance(), [key]: value };
  applyAppearance(current);
  listeners.forEach((listener) => listener());
  try {
    window.localStorage.setItem(key === "theme" ? THEME_STORAGE_KEY : APPEARANCE_STORAGE_KEYS[key as Exclude<K, "theme">], value);
    return true;
  } catch {
    return false;
  }
}

// subscribe registers one re-render listener for useAppearance.
// subscribe 为 useAppearance 注册一个重新渲染监听者。
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

// useAppearance returns the live preferences for components that render appearance controls.
// useAppearance 为渲染外观控件的组件返回实时偏好。
export function useAppearance(): AppearancePreferences {
  return React.useSyncExternalStore(subscribe, getAppearance, getAppearance);
}
