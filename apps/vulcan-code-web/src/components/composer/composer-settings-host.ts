import { createContext } from "react";

// Opens the application history-window settings from the composer.
// 从输入区打开应用的历史窗口设置。
export const EconomySettingsNavigation = createContext<(() => void) | null>(null);

// This presentation-only slot moves the menu without duplicating its session/draft state or service subscriptions.
// 此纯展示插槽移动菜单，但不复制会话／草稿状态或服务订阅。
export const ComposerSettingsHostContext = createContext<HTMLDivElement | null>(null);

// ComposerAccountHost is the status-bar slot for the active model account and the way to its settings page.
// ComposerAccountHost 是当前模型账号在状态栏中的插槽及前往其设置页的入口。
export interface ComposerAccountHost {
  host: HTMLDivElement | null;
  onOpenSettings: () => void;
}

export const ComposerAccountHostContext = createContext<ComposerAccountHost>({ host: null, onOpenSettings: () => {} });
