import React from "react";
import { createRoot } from "react-dom/client";
import { ContextEngineArea } from "../../src/components/features/context-engine/context-engine-area";
import { I18nProvider } from "../../src/i18n";
import { setDiagnosticScenario } from "./transport";
import "./styles.css";

// Render real production components and styles inside an explicitly isolated, resizable content pane.
// 在明确隔离、可调整大小的内容面板中渲染真实生产组件与样式。
// Takes no arguments; controls page-only locale, width and diagnostic fixtures without user-service access.
// 无参数；控制仅当前页的语言、宽度与诊断夹具，不访问用户服务。
function BrowserAcceptance() {
  // Content width is independent of the real browser viewport; returns a local sizing control.
  // 内容宽度独立于真实浏览器视口；提供本地尺寸控制。
  const [width, setWidth] = React.useState(1000);
  // Revision reloads fixture diagnostics without publishing user settings.
  // 修订号重载夹具诊断，不发布用户设置。
  const [revision, setRevision] = React.useState(1);
  // Locale controls only this isolated provider instance.
  // 语言仅控制此隔离上下文实例。
  const [locale, setLocale] = React.useState("zh-CN");
  return <I18nProvider initialLocale={locale}>
    <div className="p-4 text-sm">
      <p>隔离布局验收：使用生产组件与样式；所有保存和模型测试均被拒绝，不连接用户服务。</p>
      <div className="my-3 flex flex-wrap gap-3">
        {[360, 640, 1000].map((value) => <button key={value} className="rounded border px-3 py-1"
          onClick={() => setWidth(value)}>{value} px</button>)}
        <button className="rounded border px-3 py-1" onClick={() => { setDiagnosticScenario(true); setRevision((value) => value + 1); }}>长诊断</button>
        <button className="rounded border px-3 py-1" onClick={() => { setDiagnosticScenario(false); setRevision((value) => value + 1); }}>正常状态</button>
        <button className="rounded border px-3 py-1" onClick={() => setLocale((value) => value === "zh-CN" ? "en-US" : "zh-CN")}>中文 / English</button>
      </div>
      <main id="vce-browser-pane" style={{ width, maxWidth: "100%" }}><ContextEngineArea workbenchRevision={revision} /></main>
    </div>
  </I18nProvider>;
}

// Mount only the isolated acceptance root; missing markup is an explicit harness error.
// 仅挂载隔离验收根节点；缺少标记时明确报告夹具错误。
const root = document.getElementById("root");
if (!root) throw new Error("Missing isolated browser acceptance root");
createRoot(root).render(<BrowserAcceptance />);
