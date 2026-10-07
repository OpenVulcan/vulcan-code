import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { I18nProvider } from "../i18n";
import { HtmlAnswerWindowProvider } from "../components/conversation/html-answer-context";
import { MarkdownMessage } from "../components/conversation/conversation-markdown";
import type { ConversationEventWindowControllerSnapshot } from "../service/conversation-event-window-controller";

/** Mount the existing renderer with or without the new ordinary-message context; return a view.
 * 挂载带或不带新增普通消息上下文的既有渲染器；返回展示树。 */
function View({ snapshot, enabled }: { snapshot: ConversationEventWindowControllerSnapshot; enabled: boolean }) {
  const children = snapshot.events.map((event) => event.event.kind === "assistant_text" &&
    <MarkdownMessage key={event.seq} content={event.event.text} sourceMessageId={`v1:${event.seq}`} />);
  return <I18nProvider initialLocale="zh-CN">{enabled
    ? <HtmlAnswerWindowProvider snapshot={snapshot} sessionId={snapshot.owner!.session_id} parentSessionId={null}>{children}</HtmlAnswerWindowProvider>
    : children}</I18nProvider>;
}

/** Measure twenty alternating fresh mounts after three warmups, then retain 100 unopened real cards.
 * 三次预热后交替测量二十次全新挂载，再保留一百个未打开的真实卡片。
 * The only source is the local synthetic Core fixture; results contain timings and counts, never tokens.
 * 唯一来源为本地合成 Core 夹具；结果只含耗时和数量，不含令牌。 */
export async function measureHtmlAnswerRendering(): Promise<void> {
  const response = await fetch("/fixture/performance-window");
  if (!response.ok) throw new Error("Synthetic performance window unavailable");
  const snapshot: ConversationEventWindowControllerSnapshot = await response.json();
  const ordinary = { ...snapshot, html_answer_projections: [], events: snapshot.events.map((event) => ({ ...event,
    event: { kind: "assistant_text" as const, api_call_id: 1, status: "completed" as const, text: "Ordinary answer. **Summary** with a [reference](https://example.com)." } })) };
  const container = document.getElementById("root")!;
  const baseline: number[] = [];
  const enabled: number[] = [];
  for (let sample = -3; sample < 20; sample += 1) {
    for (const active of sample % 2 === 0 ? [false, true] : [true, false]) {
      const root = createRoot(container);
      const start = performance.now();
      flushSync(() => root.render(<View snapshot={ordinary} enabled={active} />));
      const elapsed = performance.now() - start;
      if (sample >= 0) (active ? enabled : baseline).push(elapsed);
      flushSync(() => root.unmount());
    }
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }
  const root = createRoot(container);
  flushSync(() => root.render(<View snapshot={snapshot} enabled />));
  const report = document.createElement("pre");
  report.id = "performance-result";
  report.textContent = JSON.stringify({ baseline, enabled, warmup: 3, samples: 20 });
  document.body.append(report);
}
