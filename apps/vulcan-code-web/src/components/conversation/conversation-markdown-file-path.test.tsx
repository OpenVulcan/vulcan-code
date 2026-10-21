// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { WorkspaceFileOpenContext } from "@/components/workspace/workspace-file-open-request";
import { MarkdownMessage } from "./conversation-markdown";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

const content = "See `src/app.ts:4-6` and `obj.method`.\n\n```\nsrc/fenced.ts\n```\n";

it("turns inline file paths into file panel controls only where the workbench offers an opener", async () => {
  const open = vi.fn();
  await act(async () => root.render(
    <I18nProvider initialLocale="en-US">
      <WorkspaceFileOpenContext.Provider value={open}><MarkdownMessage content={content} eager /></WorkspaceFileOpenContext.Provider>
    </I18nProvider>,
  ));
  const controls = Array.from(host.querySelectorAll<HTMLElement>("code[role=button]"));
  expect(controls.map((control) => control.textContent)).toEqual(["src/app.ts:4-6"]);
  await act(async () => controls[0].click());
  expect(open).toHaveBeenCalledWith("src/app.ts", { start: 4, end: 6 });

  await act(async () => root.render(<I18nProvider initialLocale="en-US"><MarkdownMessage content={content} eager /></I18nProvider>));
  expect(host.querySelectorAll("code[role=button]")).toHaveLength(0);
});
