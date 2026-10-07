import React from "react";
import { createRoot } from "react-dom/client";
import { I18nProvider } from "../i18n";
import { HtmlAnswerWindowProvider } from "../components/conversation/html-answer-context";
import { MarkdownMessage } from "../components/conversation/conversation-markdown";
import { projectConversationBranchPresentationV1 } from "../components/conversation/conversation-branch-presentation-v1";
import type { ConversationEventWindowControllerSnapshot } from "../service/conversation-event-window-controller";
import "../boot-styles.css";
import "../styles.css";

/** Render real production components against the explicitly selected synthetic HTTP host.
 * 对显式选中的合成 HTTP 宿主渲染真实生产组件。
 * The fixture window is the sole input; no service discovery or model work is initiated here.
 * 唯一输入为夹具窗口；此处不发起服务发现或模型工作。 */
function Acceptance(props: { initial: ConversationEventWindowControllerSnapshot }) {
  const [snapshot, setSnapshot] = React.useState(props.initial);
  const sessionId = snapshot.owner!.session_id;
  const presentation = projectConversationBranchPresentationV1(sessionId, snapshot.events,
    { sessionId, status: "ready", resources: new Map(), error: null, contentUrl: () => "" }, (key) => key);
  return <I18nProvider initialLocale="zh-CN"><main style={{ margin: "24px auto", maxWidth: 900, padding: 16 }}>
    <h1>会话 HTML 回答验收</h1><p>点击结果卡后，在应用内打开交互页面。</p>
    <button id="append" onClick={() => setSnapshot((current) => ({ ...current, events: [...current.events, {
      seq: current.events.length + 4, event_revision: 1, turn_id: null, created_at_ms: 3, updated_at_ms: 3, visibility: "provider",
      event: { kind: "assistant_text", api_call_id: 1, status: "completed", text: "新增普通回复" },
    }] }))}>Append ordinary message</button>
    <HtmlAnswerWindowProvider snapshot={snapshot} sessionId={snapshot.owner!.session_id} parentSessionId={null}>
      {presentation.renderItems.flatMap((item) => item.blocks.map((block) => block.kind === "markdown" && <MarkdownMessage key={block.id}
        content={block.body} sourceMessageId={block.sourceMessageId} />))}
    </HtmlAnswerWindowProvider>
  </main></I18nProvider>;
}

/** Fetch only the fixture-local canonical window and mount it; surface a generic setup failure.
 * 仅获取夹具本地的权威窗口并挂载；显示不含敏感信息的建立失败。 */
async function mount(): Promise<void> {
  if (new URLSearchParams(location.search).has("performance")) {
    const { measureHtmlAnswerRendering } = await import("./html-answer-performance");
    await measureHtmlAnswerRendering();
    return;
  }
  const response = await fetch("/fixture/window");
  if (!response.ok) throw new Error("Synthetic window unavailable");
  createRoot(document.getElementById("root")!).render(<React.StrictMode><Acceptance initial={await response.json()} /></React.StrictMode>);
}
void mount().catch(() => { document.getElementById("root")!.textContent = "Synthetic setup failed"; });
