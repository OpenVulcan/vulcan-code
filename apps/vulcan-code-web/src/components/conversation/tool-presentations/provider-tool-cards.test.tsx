// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { ProviderToolExtractPresentationCard } from "./provider-tool-cards";

// host and root isolate one rendered extraction card.
// host 和 root 隔离渲染的一张提取卡片。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("parses an extracted page's Markdown only after that page is opened", async () => {
  const pages = ["a", "b"].map((name) => ({
    url: `https://example.test/${name}`, content: `# Page ${name}\n\n${"body ".repeat(200)}`, favicon: null, images: [],
  }));
  await act(async () => root.render(
    <I18nProvider initialLocale="en-US">
      <ProviderToolExtractPresentationCard
        expanded
        presentation={{ id: "extract", kind: "provider_tool_extract", results: pages, failures: [], state: "completed" }}
        requestProgress={null}
        onExpandedChange={() => {}}
      />
    </I18nProvider>,
  ));
  expect(host.querySelectorAll(".markdown-message")).toHaveLength(0);
  expect(host.querySelectorAll(".provider-tool-result-scroll article")).toHaveLength(2);

  const toggles = host.querySelectorAll<HTMLButtonElement>("article button[aria-expanded]");
  await act(async () => { toggles[0]!.click(); });
  expect(host.querySelectorAll(".markdown-message")).toHaveLength(1);
  expect(toggles[0]!.getAttribute("aria-expanded")).toBe("true");
});
