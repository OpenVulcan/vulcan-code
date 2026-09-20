// createPluginPanel returns the Core-owned AI browser for one conversation-scoped tab.
// createPluginPanel 为单个会话限定的页签返回 Core 持有的 AI 浏览器。
// sdk supplies the trusted conversation identity and host browser component; returns the panel component.
// sdk 提供受信任的会话身份与宿主浏览器组件；返回面板组件。
export function createPluginPanel(sdk) {
  // CoreBrowserPanel renders only the selected conversation's AI browser projection.
  // CoreBrowserPanel 只渲染当前选中会话的 AI 浏览器投影。
  // Returns the browser surface or a conversation-selection hint.
  // 返回浏览器界面或会话选择提示。
  return function CoreBrowserPanel() {
    const conversationId = sdk.context.conversationId;
    return conversationId
      ? sdk.React.createElement(sdk.components.CoreBrowserArea, { ownerSessionId: conversationId })
      : sdk.React.createElement("div", { className: "p-4 text-sm text-muted-foreground" },
        sdk.t("conversationRequired", "Select a conversation to view its AI browser."));
  };
}
