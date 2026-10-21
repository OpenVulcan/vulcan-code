// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ConversationRenderItem } from "./conversation-model";
import { ConversationRenderItemView } from "./conversation-render-items";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  // React reports every caught render error to the console; the failure is the point of these tests.
  // React 会把每个被捕获的渲染错误写到控制台；这些测试本就在制造失败。
  vi.spyOn(console, "error").mockImplementation(() => {});
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

// render mounts one transcript row; every callback is inert because only rendering is under test.
// render 挂载一条转录行；只测试渲染，因此所有回调都不做任何事。
async function render(item: ConversationRenderItem): Promise<void> {
  const inert = () => {};
  await act(async () => root.render(
    <I18nProvider initialLocale="en-US">
      <ConversationRenderItemView
        autoExpandThinkingBlockId={null}
        commandSessions={[]}
        expandedToolPresentationIds={new Set()}
        expandedUserPromptIds={new Set()}
        hiddenThinkingBlockIds={new Set()}
        item={item}
        onCopyText={inert}
        onExtractGeneratedMediaAttachments={() => []}
        onLoadToolDetail={async () => null}
        onOpenDetail={inert}
        onOpenMedia={inert}
        onOpenSubagentPanel={async () => false}
        onToolPresentationExpandedChange={inert}
        onUserPromptExpandedChange={inert}
        parentConversationId={null}
        resolveComposerAttachmentMedia={() => null}
        resolveMediaSourceUrl={(path) => path}
        sessionRunning={false}
      />
    </I18nProvider>,
  ));
}

const notice = () => host.querySelector(".vk-msg-fault");

it("shows a notice in place of a row that fails to render and tries again when the row's content changes", async () => {
  // A row whose blocks are missing cannot render; the projection never produces one, so it stands in for any bug.
  // 缺少块的行无法渲染；投影从不产生这种行，因此用它代表任意缺陷。
  const broken = { id: "row", role: "assistant", blocks: null, sourceMessages: [] } as unknown as ConversationRenderItem;
  await render(broken);
  expect(notice()).not.toBeNull();
  expect(notice()!.querySelector("pre")?.textContent).toBeTruthy();
  await render(broken);
  expect(notice()).not.toBeNull();
  await render({ id: "row", role: "assistant", blocks: [], sourceMessages: [] } as unknown as ConversationRenderItem);
  expect(notice()).toBeNull();
});
