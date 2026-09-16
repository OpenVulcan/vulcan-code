import { createContext } from "react";

// This presentation-only slot moves the menu without duplicating its session/draft state or service subscriptions.
// 此纯展示插槽移动菜单，但不复制会话／草稿状态或服务订阅。
export const ComposerSettingsHostContext = createContext<HTMLDivElement | null>(null);
