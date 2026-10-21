// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  requestComposerCommand,
  requestNewConversationWithText,
  useComposerExternalCommands,
  useNewConversationRequests,
} from "./composer-external-commands";

// Composer stands in for the mounted composer and records inserted text.
// Composer 代替已挂载的输入区并记录插入的文本。
function Composer(props: { inserted: string[] }) {
  useComposerExternalCommands(() => undefined, [], () => undefined, (text) => props.inserted.push(text));
  return null;
}

// Workbench records new-conversation requests.
// Workbench 记录新会话请求。
function Workbench(props: { onOpen: () => void }) {
  useNewConversationRequests(props.onOpen);
  return null;
}

let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

describe("composer external commands", () => {
  it("opens a conversation and hands the text to the composer that mounts next, once", async () => {
    const onOpen = vi.fn();
    const inserted: string[] = [];
    await act(async () => root.render(<Workbench onOpen={onOpen} />));
    await act(async () => requestNewConversationWithText("tidy memory"));
    expect(onOpen).toHaveBeenCalledTimes(1);

    await act(async () => root.render(<><Workbench onOpen={onOpen} /><Composer inserted={inserted} /></>));
    expect(inserted).toEqual(["tidy memory"]);

    await act(async () => root.render(<Workbench onOpen={onOpen} />));
    await act(async () => root.render(<><Workbench onOpen={onOpen} /><Composer inserted={inserted} /></>));
    expect(inserted).toEqual(["tidy memory"]);
  });

  it("inserts into an already mounted composer", async () => {
    const inserted: string[] = [];
    await act(async () => root.render(<Composer inserted={inserted} />));
    await act(async () => requestComposerCommand({ kind: "insert", text: "hello" }));
    expect(inserted).toEqual(["hello"]);
  });
});
