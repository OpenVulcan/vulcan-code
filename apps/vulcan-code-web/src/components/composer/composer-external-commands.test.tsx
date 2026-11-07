// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  type ComposerOwner,
  requestComposerCommand,
  requestNewConversationWithText,
  useComposerExternalCommands,
  useNewConversationRequests,
} from "./composer-external-commands";

// Composer stands in for the mounted composer and records inserted text.
// Composer 代替已挂载的输入区并记录插入的文本。
function Composer(props: { inserted: string[]; owner?: ComposerOwner; skills?: unknown[] }) {
  useComposerExternalCommands(() => undefined, [], () => undefined, (text, _files, skills) => {
    props.inserted.push(text);
    props.skills?.push(skills);
  }, props.owner ?? { scopeKey: "draft", projectId: null, isDraft: true });
  return null;
}

// Workbench records new-conversation requests.
// Workbench 记录新会话请求。
function Workbench(props: { onOpen: (projectId: string | null) => void }) {
  useNewConversationRequests(props.onOpen);
  return null;
}

let root: Root;
let host: HTMLDivElement;

// show renders `node` and lets a pending insert, which lands a tick after the composer settles, arrive.
// show 渲染 `node`，并让在输入区稳定后晚一拍落入的待插入文本到达。
async function show(node: React.ReactNode): Promise<void> {
  await act(async () => root.render(node));
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
}
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
    await show(<Workbench onOpen={onOpen} />);
    await act(async () => requestNewConversationWithText("tidy memory"));
    expect(onOpen).toHaveBeenCalledTimes(1);

    await show(<><Workbench onOpen={onOpen} /><Composer inserted={inserted} /></>);
    expect(inserted).toEqual(["tidy memory"]);

    await show(<Workbench onOpen={onOpen} />);
    await show(<><Workbench onOpen={onOpen} /><Composer inserted={inserted} /></>);
    expect(inserted).toEqual(["tidy memory"]);
  });

  it("keeps a project-targeted draft waiting until a new conversation in that project shows", async () => {
    const onOpen = vi.fn();
    const inserted: string[] = [];
    const skills: unknown[] = [];
    const existing = { scopeKey: "session-1", projectId: "project-b", isDraft: false };
    await show(<><Workbench onOpen={onOpen} /><Composer inserted={inserted} owner={existing} skills={skills} /></>);
    await act(async () => requestNewConversationWithText("fix it", { projectId: "project-b", promptSkillOverrides: { "vulcan-self-fix": "enabled" } }));
    expect(onOpen).toHaveBeenCalledWith("project-b");
    expect(inserted).toEqual([]);

    const otherDraft = { scopeKey: "draft-a", projectId: "project-a", isDraft: true };
    await show(<><Workbench onOpen={onOpen} /><Composer inserted={inserted} owner={otherDraft} skills={skills} /></>);
    expect(inserted).toEqual([]);

    const draft = { scopeKey: "draft-b", projectId: "project-b", isDraft: true };
    await show(<><Workbench onOpen={onOpen} /><Composer inserted={inserted} owner={draft} skills={skills} /></>);
    expect(inserted).toEqual(["fix it"]);
    expect(skills).toEqual([{ "vulcan-self-fix": "enabled" }]);
  });

  it("inserts into an already mounted composer", async () => {
    const inserted: string[] = [];
    await show(<Composer inserted={inserted} />);
    await act(async () => requestComposerCommand({ kind: "insert", text: "hello" }));
    expect(inserted).toEqual(["hello"]);
  });
});
