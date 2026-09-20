// The GUI-local setting affects conversation website clicks only, not general external-link routing.
// 此 GUI 本地设置仅影响会话网页点击，不参与通用外部链接路由。
const CONVERSATION_LINK_PREFERENCE_KEY = "vulcan.gui.conversation-links-in-browser.v1";

// readConversationLinkPreference reads the explicit GUI choice, defaulting to enabled without writing.
// readConversationLinkPreference 读取 GUI 的明确选择；未设置时默认开启，不回写存储。
// Takes no arguments; returns whether conversation websites should use the embedded browser.
// 无参数；返回会话网页是否使用内置浏览器。
export function readConversationLinkPreference(): boolean {
  try {
    return window.localStorage.getItem(CONVERSATION_LINK_PREFERENCE_KEY) !== "false";
  } catch {
    // An unavailable local store keeps the in-memory product default; saves still report errors.
    // 本地存储不可用时使用产品内存默认值；保存操作仍明确报告错误。
    return true;
  }
}

// saveConversationLinkPreference persists an explicit GUI choice for subsequent conversation clicks.
// saveConversationLinkPreference 持久化明确的 GUI 选择，供后续会话点击使用。
// enabled selects the behavior; returns after storage succeeds, or throws the storage error.
// enabled 选择行为；存储成功后返回，失败则抛出存储错误。
export function saveConversationLinkPreference(enabled: boolean): void {
  window.localStorage.setItem(CONVERSATION_LINK_PREFERENCE_KEY, String(enabled));
}
