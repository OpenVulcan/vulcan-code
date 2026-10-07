// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { MarkdownImageWorkspacePathProvider, MarkdownMessage } from "./conversation-markdown";
import { HtmlAnswerWindowProvider } from "./html-answer-context";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import type { HtmlAnswerDescriptor, HtmlAnswerRange } from "@/service/html-answer-projections";
import { createHtmlAnswerViewLink, revokeHtmlAnswerViewLink, type HtmlAnswerViewLink } from "@/service/html-answers-service";
import { STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS } from "./streaming-markdown-blocks";
import { projectConversationBranchPresentationV1 } from "./conversation-branch-presentation-v1";
import { parseConversationBlocks } from "./conversation-transcript-parser";
import { HTML_ANSWER_CLOSE } from "@/service/html-answer-close";

vi.mock("@/service/html-answers-service", () => ({ createHtmlAnswerViewLink: vi.fn(), revokeHtmlAnswerViewLink: vi.fn(), htmlAnswerRequestErrorLabel: () => "htmlAnswers.cannotOpen" }));
vi.mock("@/i18n", async (original) => ({ ...await original<typeof import("@/i18n")>(), useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));

// Synthetic sources never contact a real Core or execute in jsdom.
// 合成源码绝不访问真实 Core，也不在 jsdom 执行。
const html = '<!doctype html>\r\n<body><script>window.pwned=true</script>![literal](C:/inside.png)</body>\r\n';
const fence = '```vulcan-html {"version":1,"title":"参数 🌏"}\r\n' + html + '```\r\n';
const prefix = '[Reference][saved]\r\n\r\n中文🌏 e\u0301 ![local](C:/image.png)\r\n\r\n';
const body = prefix + fence + '\r\n```diff\r\n-old\r\n+new\r\n```\r\n\r\n[saved]: https://example.com/retained\r\n';
const receipt: HtmlAnswerViewLink = { grantId: "grant", viewerUrl: "http://127.0.0.1:7341/html-answers/session/artifact#ticket=synthetic", expiresInSeconds: 300 };
let root: Root;
let host: HTMLDivElement;
let clipboard: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("localStorage", { getItem: () => null });
  vi.stubGlobal("sessionStorage", { getItem: () => null });
  clipboard = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: clipboard } });
  vi.mocked(createHtmlAnswerViewLink).mockReset().mockResolvedValue(receipt);
  vi.mocked(revokeHtmlAnswerViewLink).mockReset().mockResolvedValue(undefined);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

/** Return byte and UTF-16 ranges over the same original fixture text.
 * 返回相同原始夹具正文中的字节与 UTF-16 区间。 */
function range(start: number, end: number): HtmlAnswerRange {
  return { utf16_start: start, utf16_end: end, byte_start: new TextEncoder().encode(body.slice(0, start)).length,
    byte_end: new TextEncoder().encode(body.slice(0, end)).length };
}

/** Make a synthetic durable descriptor with the exact source coordinates consumed by production UI.
 * 构建合成可靠描述，采用生产界面实际消费的精确源码坐标。 */
function descriptor(): HtmlAnswerDescriptor {
  return { ordinal: 0, title: "参数 🌏", fence: range(prefix.length, prefix.length + fence.length),
    availability: { status: "ready", artifact_id: "a".repeat(64), source: range(body.indexOf(html), body.indexOf(html) + html.length) } };
}

/** Build a canonical assistant event; supplied seq distinguishes independent messages.
 * 构建权威助手事件；传入 seq 区分独立消息。 */
function event(seq = 3, text = body): ConversationEventSummary {
  return { seq, turn_id: 1, created_at_ms: 1, updated_at_ms: 1, event_revision: 7, visibility: "provider",
    event: { kind: "assistant_text", api_call_id: 2, status: "completed", text } };
}

/** Create an atomic synthetic window; annotations are an explicit separate input.
 * 创建原子合成窗口；标注作为显式独立输入。 */
function windowSnapshot(events = [event()], ready = true): ConversationEventWindowControllerSnapshot {
  return { owner: { session_id: "session", window_generation: 4 }, service_epoch: "epoch", memory_revision: 9, durable_revision: 9,
    older_before_seq: null, has_more: false, open_turn_id: null, open_turn_start_seq: null, session_config: null, events,
    html_answer_projections: ready ? events.map((entry) => ({ seq: entry.seq, event_revision: entry.event_revision, answers: [descriptor()] })) : [] };
}

/** Render through the same context, image normalization, sanitizer and Markdown AST as the branch.
 * 沿分支相同的上下文、图片规范化、清理器和 Markdown AST 渲染。 */
async function renderWindow(snapshot: ConversationEventWindowControllerSnapshot, parent: string | null = null) {
  const sessionId = snapshot.owner!.session_id;
  const presentation = projectConversationBranchPresentationV1(sessionId, snapshot.events,
    { sessionId, status: "ready", resources: new Map(), error: null, contentUrl: () => "" }, (key) => key);
  const messages = new Map(presentation.messages.map((message) => [message.id, message]));
  await act(async () => root.render(<HtmlAnswerWindowProvider snapshot={snapshot} sessionId={snapshot.owner!.session_id} parentSessionId={parent}>
    <MarkdownImageWorkspacePathProvider value="C:/project">
      {snapshot.events.flatMap((entry) => {
        const message = messages.get(`v1:${entry.seq}`);
        if (!message || entry.event.kind !== "assistant_text") return [];
        const streaming = entry.event.status === "streaming";
        return parseConversationBlocks(message).map((block) => block.kind === "markdown" && <MarkdownMessage key={block.id} content={block.body}
          sourceMessageId={block.sourceMessageId} streaming={streaming} />);
      })}
    </MarkdownImageWorkspacePathProvider>
  </HtmlAnswerWindowProvider>));
  // Await the real lazy module boundary rather than sleeping or bypassing deferred loading.
  // 等待真实延迟模块边界，不休眠也不绕过按需加载。
  await act(async () => vi.dynamicImportSettled());
}

/** Click a localized action by semantic label; report missing action clearly.
 * 按语义标签点击本地化操作；清楚报告缺失操作。 */
async function click(label: string) {
  const button = [...document.querySelectorAll("button")].find((entry) => entry.textContent === label || entry.getAttribute("aria-label") === label);
  expect(button, `Expected action ${label} may have changed`).toBeDefined();
  await act(async () => button!.click());
}

it("preserves Unicode/CRLF coordinates, image normalization, reference context and exact source without executing", async () => {
  await renderWindow(windowSnapshot());
  expect(host.querySelectorAll(".html-answer-card")).toHaveLength(1);
  expect(host.querySelector('a[href="https://example.com/retained"]')).not.toBeNull();
  expect(host.querySelectorAll("img")).toHaveLength(1);
  expect(host.querySelector("iframe,script")).toBeNull();
  expect(createHtmlAnswerViewLink).not.toHaveBeenCalled();
  await click("htmlAnswers.copySource");
  expect(clipboard).toHaveBeenLastCalledWith(html);
  await click("htmlAnswers.source");
  expect(host.querySelector(".html-answer-source")?.textContent).toBe(html);
  expect(host.querySelector("script")).toBeNull();
});

it("issues only after click, keeps exact parent/revision and releases the viewer on close", async () => {
  await renderWindow(windowSnapshot(), "parent");
  await click("htmlAnswers.open");
  expect(createHtmlAnswerViewLink).toHaveBeenCalledWith({ sessionId: "session", parentSessionId: "parent", windowGeneration: 4,
    eventSeq: 3, eventRevision: 7, artifactId: "a".repeat(64) }, "en-US");
  const frame = document.querySelector("iframe");
  expect(frame?.src).toBe(receipt.viewerUrl);
  await click("htmlAnswers.maximize");
  expect(document.querySelector("iframe")).toBe(frame);
  await click("Close");
  expect(document.querySelector("iframe")).toBeNull();
  expect(revokeHtmlAnswerViewLink).toHaveBeenCalledWith(receipt.grantId);
});

it("keeps an opened viewer mounted across unrelated event appends and revokes it on generation change", async () => {
  const first = windowSnapshot();
  await renderWindow(first);
  await click("htmlAnswers.open");
  const frame = document.querySelector("iframe");
  await renderWindow({ ...first, events: [...first.events, event(5, "ordinary text")] });
  expect(document.querySelector("iframe")).toBe(frame);
  expect(revokeHtmlAnswerViewLink).not.toHaveBeenCalled();
  await renderWindow({ ...first, owner: { session_id: "session", window_generation: 5 } });
  expect(document.querySelector("iframe")).toBeNull();
  expect(revokeHtmlAnswerViewLink).toHaveBeenCalledWith(receipt.grantId);
});

it("accepts a close relay only from its exact viewer window and origin", async () => {
  await renderWindow(windowSnapshot());
  await click("htmlAnswers.open");
  const frame = document.querySelector("iframe")!;
  const origin = new URL(receipt.viewerUrl).origin;
  for (const message of [
    { data: HTML_ANSWER_CLOSE, origin, source: window },
    { data: HTML_ANSWER_CLOSE, origin: "null", source: frame.contentWindow },
    { data: "open-host-api", origin, source: frame.contentWindow },
  ]) {
    await act(async () => window.dispatchEvent(new MessageEvent("message", message)));
    expect(document.querySelector("iframe")).toBe(frame);
  }
  await act(async () => window.dispatchEvent(new MessageEvent("message", {
    data: HTML_ANSWER_CLOSE, origin, source: frame.contentWindow,
  })));
  expect(document.querySelector("iframe")).toBeNull();
  expect(revokeHtmlAnswerViewLink).toHaveBeenCalledWith(receipt.grantId);
});

it("revokes a late grant when its pending modal closes without mounting a frame", async () => {
  let resolve!: (value: HtmlAnswerViewLink) => void;
  vi.mocked(createHtmlAnswerViewLink).mockImplementation(() => new Promise((done) => { resolve = done; }));
  await renderWindow(windowSnapshot());
  await click("htmlAnswers.open");
  await click("Close");
  await act(async () => resolve(receipt));
  expect(document.querySelector("iframe")).toBeNull();
  expect(revokeHtmlAnswerViewLink).toHaveBeenCalledWith(receipt.grantId);
});

it("keeps missing authority and streaming inert and excludes non-assistant and nested samples", async () => {
  const pending = windowSnapshot([event()], false);
  await renderWindow(pending);
  expect(host.querySelector('.html-answer-card button')?.hasAttribute("disabled")).toBe(true);
  const growing = event();
  if (growing.event.kind === "assistant_text") growing.event.status = "streaming";
  await renderWindow(windowSnapshot([growing], false));
  expect(host.querySelector('.html-answer-card[data-status="generating"]')).not.toBeNull();
  const sample = event(3, fence.split("\r\n").map((line) => `> ${line}`).join("\r\n"));
  await renderWindow(windowSnapshot([sample], false));
  expect(host.querySelector(".html-answer-card")).toBeNull();
  await renderWindow(windowSnapshot([{ ...event(), visibility: "ui_only" }], false));
  expect(host.querySelector(".html-answer-card")).toBeNull();
  expect(createHtmlAnswerViewLink).not.toHaveBeenCalled();
});

it("keeps one hundred unopened cards free of frames, requests and injected HTML", async () => {
  await renderWindow(windowSnapshot(Array.from({ length: 100 }, (_, index) => event(index + 1))));
  expect(host.querySelectorAll(".html-answer-card")).toHaveLength(100);
  expect(host.querySelector("iframe,script")).toBeNull();
  expect(createHtmlAnswerViewLink).not.toHaveBeenCalled();
});

it("collapses a large growing HTML tail without parsing or exposing an executable frame", async () => {
  const growing = event(3, fence.replace(html, "x".repeat(STREAMING_MARKDOWN_RAW_TAIL_MAX_CHARS * 2)));
  if (growing.event.kind === "assistant_text") growing.event.status = "streaming";
  await renderWindow(windowSnapshot([growing], false));
  expect(host.querySelectorAll('.html-answer-card[data-status="generating"]')).toHaveLength(1);
  expect(host.querySelector("iframe,pre")).toBeNull();
  expect(createHtmlAnswerViewLink).not.toHaveBeenCalled();
});

it("copies a new temporary link and revokes it if clipboard delivery fails", async () => {
  await renderWindow(windowSnapshot());
  await click("htmlAnswers.copyLink");
  expect(clipboard).toHaveBeenLastCalledWith(receipt.viewerUrl);
  expect(document.querySelector("iframe")).toBeNull();
  expect(revokeHtmlAnswerViewLink).not.toHaveBeenCalled();
  clipboard.mockRejectedValue(new Error("denied"));
  // The production clipboard adapter falls back to selection when the modern API refuses access.
  // 正式剪贴板适配器在现代接口拒绝访问时回退到选区复制。
  Object.defineProperty(document, "execCommand", { configurable: true, value: () => false });
  await click("htmlAnswers.copyLink");
  expect(revokeHtmlAnswerViewLink).toHaveBeenCalledWith(receipt.grantId);
  expect(host.querySelector('[role="status"]')?.textContent).toBe("htmlAnswers.copyFailed");
});
