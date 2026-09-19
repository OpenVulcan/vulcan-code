// createPluginPanel returns the host-owned Core browser projection for one conversation mount.
// createPluginPanel 为单个会话挂载点返回由宿主持有的 Core 浏览器投影。
export function createPluginPanel(sdk) {
  // CoreBrowserPanel keeps the selected conversation identity in the host SDK context.
  // CoreBrowserPanel 从宿主 SDK 上下文读取当前选中的会话身份。
  return function CoreBrowserPanel() {
    // mode switches only this client's panel; it never changes the AI's Core target.
    // mode 只切换本客户端面板，绝不改变 AI 的 Core 目标。
    const [mode, setMode] = sdk.React.useState("ai");
    // conversationId is the only valid owner identity for an AI browser session.
    // conversationId 是 AI 浏览器会话唯一有效的归属身份。
    const conversationId = sdk.context.conversationId;
    // content is either the Core projection or a separate client-owned temporary browser.
    // content 是 Core 投影或独立的客户端临时浏览器。
    const content = mode === "local"
      ? sdk.React.createElement(sdk.components.ClientBrowserArea)
      : conversationId
        ? sdk.React.createElement(sdk.components.CoreBrowserArea, { ownerSessionId: conversationId })
        : sdk.React.createElement("div", { className: "p-4 text-sm text-muted-foreground" },
          sdk.t("conversationRequired", "Select a conversation to view its AI browser."));
    return sdk.React.createElement("div", { className: "flex h-full min-h-0 flex-col" },
      sdk.React.createElement("div", { className: "flex gap-1 border-b p-1" },
        sdk.React.createElement(sdk.components.Button, {
          type: "button", variant: mode === "ai" ? "secondary" : "ghost", size: "sm",
          onClick: () => setMode("ai"),
        }, sdk.t("navigationLabel", "AI Browser")),
        sdk.React.createElement(sdk.components.Button, {
          type: "button", variant: mode === "local" ? "secondary" : "ghost", size: "sm",
          onClick: () => setMode("local"),
        }, sdk.t("localBrowserLabel", "Local Browser"))),
      sdk.React.createElement("div", { className: "min-h-0 flex-1" }, content));
  };
}
