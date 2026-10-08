// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { HTML_ANSWER_CLOSE_MESSAGE, HTML_ANSWER_HEIGHT_MESSAGE } from "@/generated/protocol-limits";
import { HtmlAnswerViewer } from "./viewer";
import { exchangeHtmlAnswerTicket, parseHtmlAnswerTicket, releaseHtmlAnswerLease, type HtmlAnswerLease } from "./transport";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
// The synthetic lease never reaches a real Core or native browser.
// 合成租约不会访问真实 Core 或原生浏览器。
const lease: HtmlAnswerLease = { contentUrl: "http://127.0.0.1:7202/content/synthetic", leaseToken: "synthetic",
  expiresInSeconds: 60, source: "<script>window.test=true</script>", title: "<img src=x>", locale: "en-US" };
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("creates no frame before a lease, escapes its title/source, and destroys the frame on expiry", async () => {
  await act(async () => root.render(<HtmlAnswerViewer state={{ status: "loading" }} />));
  expect(host.querySelector("iframe")).toBeNull();
  await act(async () => root.render(<HtmlAnswerViewer state={{ status: "ready", lease }} />));
  expect(host.querySelector("img")).toBeNull();
  expect(host.querySelector("iframe")?.getAttribute("sandbox")).toBe("allow-scripts");
  const sourceButton = [...host.querySelectorAll("button")].find((button) => button.textContent === "htmlAnswers.source")!;
  await act(async () => sourceButton.click());
  expect(host.querySelector("iframe")).toBeNull();
  expect(host.querySelector("script")).toBeNull();
  expect(host.querySelector("code")?.textContent).toBe(lease.source);
  await act(async () => root.render(<HtmlAnswerViewer state={{ status: "error", label: "htmlAnswers.expired" }} />));
  expect(host.querySelector("iframe")).toBeNull();
  expect(host.querySelector("code")).toBeNull();
});

it("uses a single exact fragment ticket and sends no cookies or general service authorization", async () => {
  const valid = new URL("http://127.0.0.1:7201/html-answers/session/artifact#ticket=synthetic");
  const ticket = parseHtmlAnswerTicket(valid);
  for (const fragment of ["", "#ticket=a&ticket=b", "#ticket=a&token=core", "#token=core"]) {
    valid.hash = fragment;
    expect(() => parseHtmlAnswerTicket(valid)).toThrow("htmlAnswers.invalid");
  }
  const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify(lease), { status: 200 }));
  vi.stubGlobal("fetch", fetch);
  await exchangeHtmlAnswerTicket(ticket, new AbortController().signal);
  expect(fetch).toHaveBeenCalledWith("/v1/html-answers/exchange", expect.objectContaining({
    method: "POST", credentials: "omit", redirect: "error", referrerPolicy: "no-referrer",
    headers: { "Content-Type": "application/json" }, body: JSON.stringify(ticket),
  }));
  releaseHtmlAnswerLease(lease.leaseToken);
  expect(fetch).toHaveBeenLastCalledWith("/v1/html-answers/release", expect.objectContaining({ keepalive: true,
    credentials: "omit", body: JSON.stringify({ leaseToken: lease.leaseToken }) }));
});

it("exposes stable error categories without echoing response diagnostics or secret input", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: "expired", detail: "secret" }), { status: 410 })));
  await expect(exchangeHtmlAnswerTicket({ sessionId: "session", artifactId: "artifact", ticket: "secret" }, new AbortController().signal))
    .rejects.toThrow("htmlAnswers.expired");
});

it("embeds only the page frame and relays nothing but a finite height or a close signal", async () => {
  const parent = { postMessage: vi.fn() };
  vi.stubGlobal("parent", parent);
  await act(async () => root.render(<HtmlAnswerViewer state={{ status: "ready", lease }} />));
  expect(host.querySelector("button")).toBeNull();
  const frame = host.querySelector("iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
  const send = (data: unknown, origin = "null", source: MessageEventSource | null = frame.contentWindow) =>
    act(async () => window.dispatchEvent(new MessageEvent("message", { data, origin, source })));
  await send({ type: HTML_ANSWER_HEIGHT_MESSAGE, height: 420, secret: "page data" });
  expect(parent.postMessage).toHaveBeenLastCalledWith({ type: HTML_ANSWER_HEIGHT_MESSAGE, height: 420 }, "*");
  await send({ type: HTML_ANSWER_HEIGHT_MESSAGE, height: 10 }, "http://127.0.0.1:7202");
  await send({ type: HTML_ANSWER_HEIGHT_MESSAGE, height: 10 }, "null", window);
  await send({ type: HTML_ANSWER_HEIGHT_MESSAGE, height: Number.NaN });
  await send("open-host-api");
  expect(parent.postMessage).toHaveBeenCalledTimes(1);
  await send(HTML_ANSWER_CLOSE_MESSAGE);
  expect(parent.postMessage).toHaveBeenLastCalledWith(HTML_ANSWER_CLOSE_MESSAGE, "*");
});
