import React from "react";
import type { ComposerSlashCommandSummary } from "@/types";

// COMPOSER_COMMAND_EVENT carries requests from outside the composer, such as the command palette, to the
// composer that is currently mounted.
// COMPOSER_COMMAND_EVENT 将来自输入区之外（例如命令面板）的请求送到当前挂载的输入区。
const COMPOSER_COMMAND_EVENT = "vulcan:composer-command";

// NEW_CONVERSATION_EVENT asks the workbench to open a new conversation in the active project.
// NEW_CONVERSATION_EVENT 请求工作台在当前项目中打开新会话。
const NEW_CONVERSATION_EVENT = "vulcan:new-conversation";

// ComposerExternalCommand is one thing another surface can ask the composer to do: open its model picker,
// run one of its slash commands by id exactly as if it were chosen in the slash list, or insert text into
// the draft without sending it.
// ComposerExternalCommand 是其他界面可请求输入区执行的一件事：打开模型选择器；按 id 运行某个 slash 指令，效果与在
// slash 列表中选择相同；或向草稿插入文本但不发送。
export type ComposerExternalCommand = { kind: "model" } | { kind: "slash"; id: string } | { kind: "insert"; text: string };

// pendingInsert holds text requested before a composer was mounted; the next mounted composer takes it.
// pendingInsert 保存输入区挂载前请求插入的文本；下一个挂载的输入区会取走它。
let pendingInsert: string | null = null;

// requestComposerCommand asks the mounted composer to run a command; nothing happens when no composer is
// shown, except that inserted text waits for the next composer.
// requestComposerCommand 请求已挂载的输入区执行指令；没有显示输入区时不产生任何效果，但插入的文本会等待下一个输入区。
export function requestComposerCommand(command: ComposerExternalCommand): void {
  if (command.kind === "insert") pendingInsert = command.text;
  window.dispatchEvent(new CustomEvent<ComposerExternalCommand>(COMPOSER_COMMAND_EVENT, { detail: command }));
}

// requestNewConversationWithText opens a new conversation in the active project and leaves `text` in its
// draft for the user to review and send.
// requestNewConversationWithText 在当前项目中打开新会话，并把 `text` 留在其草稿中，由用户检查后发送。
export function requestNewConversationWithText(text: string): void {
  pendingInsert = text;
  window.dispatchEvent(new CustomEvent(NEW_CONVERSATION_EVENT));
}

// useNewConversationRequests calls `open` whenever another surface asks for a new conversation.
// useNewConversationRequests 在其他界面请求新会话时调用 `open`。
export function useNewConversationRequests(open: () => void): void {
  const latest = React.useRef(open);
  latest.current = open;
  React.useEffect(() => {
    const listener = () => latest.current();
    window.addEventListener(NEW_CONVERSATION_EVENT, listener);
    return () => window.removeEventListener(NEW_CONVERSATION_EVENT, listener);
  }, []);
}

// useComposerExternalCommands runs requested commands through the composer's own model picker and slash
// activation while the composer is mounted; a slash id the composer does not offer is ignored.
// useComposerExternalCommands 在输入区挂载期间通过其自身的模型选择器与 slash 激活执行收到的指令；输入区未提供的 slash id 会被忽略。
export function useComposerExternalCommands(
  openModelPicker: () => void,
  slashCommands: readonly ComposerSlashCommandSummary[],
  activateSlashCommand: (command: ComposerSlashCommandSummary) => void,
  insertText: (text: string) => void,
): void {
  const handler = (command: ComposerExternalCommand) => {
    const slash = command.kind === "slash" ? slashCommands.find((entry) => entry.id === command.id) : null;
    if (command.kind === "model") openModelPicker();
    else if (command.kind === "insert") takePendingInsert(insertText);
    else if (slash) activateSlashCommand(slash);
  };
  const latest = React.useRef(handler);
  latest.current = handler;
  const latestInsert = React.useRef(insertText);
  latestInsert.current = insertText;
  React.useEffect(() => {
    const listener = (event: Event) => latest.current((event as CustomEvent<ComposerExternalCommand>).detail);
    window.addEventListener(COMPOSER_COMMAND_EVENT, listener);
    // Text requested while no composer was mounted lands in this one.
    // 没有输入区挂载时请求的文本落入本输入区。
    takePendingInsert((text) => latestInsert.current(text));
    return () => window.removeEventListener(COMPOSER_COMMAND_EVENT, listener);
  }, []);
}

// takePendingInsert hands the waiting text to `insert` once and clears it.
// takePendingInsert 将等待中的文本交给 `insert` 一次并清除。
function takePendingInsert(insert: (text: string) => void): void {
  const text = pendingInsert;
  pendingInsert = null;
  if (text !== null) insert(text);
}
