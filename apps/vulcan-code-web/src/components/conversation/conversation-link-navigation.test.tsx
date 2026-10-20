// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { resolveConversationLinkTarget } from "./conversation-link-navigation";
import { readConversationLinkPreference, saveConversationLinkPreference } from "./conversation-link-preference";

beforeEach(() => {
  // values isolates the GUI preference from Node's unavailable native localStorage implementation.
  // values 将 GUI 偏好与 Node 不可用的原生 localStorage 实现隔离。
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
    get length() { return values.size; },
  });
});
afterEach(() => { vi.unstubAllGlobals(); });

it("routes only ordinary transcript website/file clicks and preserves download, modifier and resource behavior", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // host and root dispatch real bubbling DOM clicks into the production capture filter.
  // host 和 root 将真实冒泡 DOM 点击派发到生产捕获过滤器。
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  // routed records only URLs admitted by the actual filter; cancellation prevents jsdom navigation.
  // routed 仅记录实际过滤器接纳的 URL；取消默认动作避免 jsdom 导航。
  const routed: string[] = [];
  try {
    await act(async () => root.render(<section onClickCapture={(event) => {
      // destination is null when the original browser action must remain in charge.
      // 原浏览器动作应继续负责时，destination 为空。
      const destination = resolveConversationLinkTarget(event, window.location.origin);
      if (destination) routed.push(destination.href);
      event.preventDefault();
    }}>
      <div className="vk-transcript-inner">
        <a id="website" href="https://example.com/"><span>Web</span></a>
        <a id="local" href="file:///C:/my%20files/page.html">Local</a>
        <a id="service" href={`${window.location.origin}/private`}>Workbench</a>
        <a id="download" href="https://example.com/download" download>Download</a>
        <a id="unsafe" href="data:text/html,unsafe">Unsafe</a>
        <div className="conversation-v1-resource"><a id="resource" href="https://example.com/resource">Resource</a></div>
      </div>
      <a id="adjacent" href="https://example.com/plugin">Plugin</a>
    </section>));
    for (const selector of ["#website span", "#local", "#service", "#download", "#unsafe", "#resource", "#adjacent"]) {
      host.querySelector(selector)!.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    }
    for (const modifier of ["ctrlKey", "metaKey", "shiftKey", "altKey"]) {
      host.querySelector("#local")!.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, [modifier]: true }));
    }
    expect(routed).toEqual(["https://example.com/", "file:///C:/my%20files/page.html"]);
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});

it("defaults to embedded without persisting and respects both explicit choices", () => {
  expect(readConversationLinkPreference()).toBe(true);
  expect(window.localStorage.length).toBe(0);
  saveConversationLinkPreference(false);
  expect(readConversationLinkPreference()).toBe(false);
  saveConversationLinkPreference(true);
  expect(readConversationLinkPreference()).toBe(true);
});
