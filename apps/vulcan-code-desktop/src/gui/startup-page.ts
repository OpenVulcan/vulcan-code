// User-facing startup document shown while the independent Core service becomes ready.
// 独立 Core 服务就绪期间展示的面向用户启动文档。
//
// Every stage and error shown here is a real reported value; the document never fabricates
// progress and never hides a failure behind an indefinite spinner.
// 此处展示的每个阶段与错误都是真实上报值；该文档绝不伪造进度，也不用无限转圈掩盖失败。

import { DESKTOP_TEXT, type DesktopLocale } from './desktop-text.js';

/** Escapes text placed into the startup HTML document. */
function escapeHtml(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

/** Renders the startup document as one self-contained HTML string in the given language. */
export function renderStartupDocument(locale: DesktopLocale): string {
  const text = DESKTOP_TEXT[locale];
  return `<!doctype html>
<html lang="${locale}">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'" />
    <title>${escapeHtml(text.startupTitle)}</title>
    <style>
      body { margin: 0; font-family: "Segoe UI", system-ui, sans-serif; background: #101215; color: #e6e8eb; }
      main { padding: 28px 30px; }
      h1 { margin: 0 0 6px; font-size: 17px; font-weight: 600; letter-spacing: .01em; }
      header { display: flex; align-items: center; justify-content: space-between; -webkit-app-region: drag; }
      button { -webkit-app-region: no-drag; border: 0; background: transparent; color: #e6e8eb; cursor: pointer; font-size: 20px; padding: 0 6px; }
      button:hover { background: #5b2a30; }
      p.stage { margin: 0; font-size: 13px; color: #9aa4b2; }
      p.failure { margin: 16px 0 0; padding: 10px 12px; border-radius: 6px; font-size: 12px; line-height: 1.5;
        background: #2a1518; border: 1px solid #5b2a30; color: #ffb4b4; white-space: pre-wrap; word-break: break-word; }
      ol { margin: 18px 0 0; padding: 0 0 0 18px; font-size: 12px; color: #6f7885; }
      li.current { color: #e6e8eb; }
    </style>
  </head>
  <body>
    <main>
      <header><h1>Vulcan Code</h1><button aria-label="${escapeHtml(text.closeWindow)}" onclick="window.close()">×</button></header>
      <p class="stage" id="stage">${escapeHtml(text.preparing)}</p>
      <p class="failure" id="failure" hidden></p>
      <ol id="history"></ol>
    </main>
    <script>
      var stageElement = document.getElementById("stage");
      var failureElement = document.getElementById("failure");
      var historyElement = document.getElementById("history");
      window.vulcanStartup = function (stage, message) {
        stageElement.textContent = message;
        var entries = historyElement.children;
        for (var index = 0; index < entries.length; index += 1) {
          entries[index].classList.remove("current");
        }
        var entry = document.createElement("li");
        entry.dataset.stage = stage;
        entry.textContent = message;
        entry.className = "current";
        historyElement.appendChild(entry);
      };
      window.vulcanStartupFailed = function (message) {
        stageElement.textContent = ${JSON.stringify(text.startupFailed).replace(/</g, '\u003c')};
        failureElement.hidden = false;
        failureElement.textContent = message;
      };
    </script>
  </body>
</html>`;
}
