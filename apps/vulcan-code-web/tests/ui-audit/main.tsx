import React from "react";
import { createRoot } from "react-dom/client";
import type { AuditCall } from "./runtime";
import "./styles.css";

// AuditWorkbench keeps fixture controls outside the application's own viewport.
// AuditWorkbench 将夹具控件放在应用自身视口之外。
// Returns: a same-origin real-app frame and inspectable RPC history.
// 返回值：同源真实应用框架及可检查的 RPC 历史。
export function AuditWorkbench() {
  // params carry only explicit synthetic scenario and theme choices.
  // params 仅携带明确的合成场景与主题选择。
  const params = new URLSearchParams(window.location.search);
  const scenario = params.get("scenario") ?? "empty";
  const [theme, setTheme] = React.useState(params.get("theme") === "dark" ? "dark" : "light");
  const [calls, setCalls] = React.useState<AuditCall[]>([]);
  const [showLog, setShowLog] = React.useState(false);
  const frame = React.useRef<HTMLIFrameElement>(null);
  React.useEffect(() => {
    // update accepts only this same-origin frame's declared log messages.
    // update 仅接纳此同源框架声明的日志消息。
    const update = (event: MessageEvent) => {
      if (event.origin !== window.location.origin || event.source !== frame.current?.contentWindow) return;
      if (event.data?.type === "ui-audit:rpc") setCalls(event.data.calls as AuditCall[]);
    };
    window.addEventListener("message", update);
    return () => window.removeEventListener("message", update);
  }, []);
  // appUrl also provides a direct full-viewport entry for browser automation.
  // appUrl 同时提供供浏览器自动化使用的全视口直达入口。
  const appUrl = `/app.html?scenario=${encodeURIComponent(scenario)}&theme=${theme}`;
  return <><div className="ui-audit-toolbar"><strong>隔离验收 · 合成数据</strong>
    <label>场景 <select value={scenario} onChange={(event) => { const url = new URL(window.location.href); url.searchParams.set("scenario", event.target.value); window.location.assign(url); }}>
      <option value="empty">空库</option><option value="invalid-default">已配置 / 失效默认</option><option value="long-list">长列表</option>
    </select></label>
    <button onClick={() => setTheme("light")}>浅色</button>
    <button onClick={() => setTheme("dark")}>深色</button>
    <a href={appUrl}>全屏应用</a>
    <button onClick={() => setShowLog((value) => !value)}>RPC 记录（拒绝 {calls.filter((call) => call.status === "rejected").length}）</button>
    <span>保存、模型调用与原生动作均未接入真实服务</span>
  </div><iframe ref={frame} title="真实 WebUI 隔离应用" className="ui-audit-workbench" src={appUrl} />
    {showLog ? <aside className="ui-audit-log"><button onClick={() => setShowLog(false)}>关闭记录</button>{calls.map((call, index) => <div key={index}>{call.status} · {call.method}</div>)}</aside> : null}
  </>;
}
// Mount only the explicit audit root and fail visibly if the harness markup is missing.
// 仅挂载明确的验收根节点，缺少夹具标记时明确失败。
const root = document.getElementById("root");
if (!root) throw new Error("UI_AUDIT_ROOT_MISSING");
createRoot(root).render(<AuditWorkbench />);
