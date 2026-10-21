// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { WorkspaceContextTabBoundary } from "./workspace-context-panel";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.spyOn(console, "error").mockImplementation(() => undefined);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

let failing = true;
function TabContent() {
  if (failing) throw new Error("diff renderer broke");
  return <p data-testid="content">ok</p>;
}

const render = (resetKey: string) => act(async () => root.render(
  <I18nProvider initialLocale="en-US"><WorkspaceContextTabBoundary resetKey={resetKey}><TabContent /></WorkspaceContextTabBoundary></I18nProvider>,
));

it("shows a failing tab's error in place and renders it again on retry", async () => {
  failing = true;
  await render("review-1:s1");
  expect(host.querySelector("[data-testid=content]")).toBeNull();
  expect(host.textContent).toContain("diff renderer broke");
  failing = false;
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-page-actions button")!.click());
  expect(host.querySelector("[data-testid=content]")).not.toBeNull();
});

it("clears a failure when another tab or session is shown", async () => {
  failing = true;
  await render("review-1:s1");
  expect(host.querySelector("[data-testid=content]")).toBeNull();
  failing = false;
  await render("files-2:s1");
  expect(host.querySelector("[data-testid=content]")).not.toBeNull();
});
