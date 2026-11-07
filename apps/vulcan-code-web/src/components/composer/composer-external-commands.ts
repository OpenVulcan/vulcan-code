import React from "react";
import type { ComposerPromptSkillOverrides } from "@/components/composer/composer-model";
import type { ComposerSlashCommandSummary } from "@/types";

// COMPOSER_COMMAND_EVENT carries requests from outside the composer, such as the command palette, to the
// composer that is currently mounted.
// COMPOSER_COMMAND_EVENT 将来自输入区之外（例如命令面板）的请求送到当前挂载的输入区。
const COMPOSER_COMMAND_EVENT = "vulcan:composer-command";

// NEW_CONVERSATION_EVENT asks the workbench to open a new conversation, in the named project or else the active one.
// NEW_CONVERSATION_EVENT 请求工作台打开新会话：有指定项目时在该项目中，否则在当前项目中。
const NEW_CONVERSATION_EVENT = "vulcan:new-conversation";

// ComposerExternalCommand is one thing another surface can ask the composer to do: open its model picker,
// run one of its slash commands by id exactly as if it were chosen in the slash list, or insert text, and
// optionally attach files, into the draft without sending it.
// ComposerExternalCommand 是其他界面可请求输入区执行的一件事：打开模型选择器；按 id 运行某个 slash 指令，效果与在
// slash 列表中选择相同；或向草稿插入文本并可附带文件，但不发送。
export type ComposerExternalCommand = { kind: "model" } | { kind: "slash"; id: string } | { kind: "insert"; text: string; files?: readonly File[] };

// NewConversationTarget names the project a new conversation opens in and the skills it switches.
// NewConversationTarget 指定新会话打开的项目及其切换的技能。
export interface NewConversationTarget { projectId: string; promptSkillOverrides: ComposerPromptSkillOverrides }

// ComposerOwner identifies the draft a mounted composer shows.
// ComposerOwner 标识已挂载输入区显示的草稿。
export interface ComposerOwner { scopeKey: string; projectId: string | null; isDraft: boolean }

// InsertText receives inserted text, its files and, for a targeted new conversation, its skill switches.
// InsertText 接收插入的文本、其文件，以及指定新会话时的技能开关。
type InsertText = (text: string, files: readonly File[], skills: ComposerPromptSkillOverrides | null) => void;

// PendingInsert is text, with any files, waiting for the next composer to take it. With `draft`, only a new
// conversation in that project takes it, and its skill switches become the draft's session settings.
// PendingInsert 是等待下一个输入区取走的文本及其附带文件。带 `draft` 时只有该项目中的新会话会取走它，其技能开关成为
// 草稿的会话设置。
type PendingInsert = { text: string; files: readonly File[]; draft?: NewConversationTarget };

// pendingInsert holds an insert requested before a composer was mounted; the next mounted composer takes it.
// pendingInsert 保存输入区挂载前请求的插入；下一个挂载的输入区会取走它。
let pendingInsert: PendingInsert | null = null;

// requestComposerCommand asks the mounted composer to run a command; nothing happens when no composer is
// shown, except that inserted text waits for the next composer.
// requestComposerCommand 请求已挂载的输入区执行指令；没有显示输入区时不产生任何效果，但插入的文本会等待下一个输入区。
export function requestComposerCommand(command: ComposerExternalCommand): void {
  if (command.kind === "insert") pendingInsert = { text: command.text, files: command.files ?? [] };
  window.dispatchEvent(new CustomEvent<ComposerExternalCommand>(COMPOSER_COMMAND_EVENT, { detail: command }));
}

// requestNewConversationWithText opens a new conversation, in `target`'s project or else the active one, and
// leaves `text` in its draft for the user to review and send.
// requestNewConversationWithText 打开新会话（有 `target` 时在其项目中，否则在当前项目中），并把 `text` 留在其草稿中，
// 由用户检查后发送。
export function requestNewConversationWithText(text: string, target?: NewConversationTarget): void {
  pendingInsert = { text, files: [], draft: target };
  window.dispatchEvent(new CustomEvent<string | null>(NEW_CONVERSATION_EVENT, { detail: target?.projectId ?? null }));
}

// useNewConversationRequests calls `open` with the requested project id, or null, whenever another surface asks
// for a new conversation.
// useNewConversationRequests 在其他界面请求新会话时以所请求的项目 id（或 null）调用 `open`。
export function useNewConversationRequests(open: (projectId: string | null) => void): void {
  const latest = React.useRef(open);
  latest.current = open;
  React.useEffect(() => {
    const listener = (event: Event) => latest.current((event as CustomEvent<string | null>).detail ?? null);
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
  insertText: InsertText,
  owner: ComposerOwner,
): void {
  const latestInsert = React.useRef(insertText);
  latestInsert.current = insertText;
  const latestOwner = React.useRef(owner);
  latestOwner.current = owner;
  const take = () => takePendingInsert(latestOwner.current, (...args) => latestInsert.current(...args));
  const handler = (command: ComposerExternalCommand) => {
    const slash = command.kind === "slash" ? slashCommands.find((entry) => entry.id === command.id) : null;
    if (command.kind === "model") openModelPicker();
    else if (command.kind === "insert") take();
    else if (slash) activateSlashCommand(slash);
  };
  const latest = React.useRef(handler);
  latest.current = handler;
  React.useEffect(() => {
    const listener = (event: Event) => latest.current((event as CustomEvent<ComposerExternalCommand>).detail);
    window.addEventListener(COMPOSER_COMMAND_EVENT, listener);
    return () => window.removeEventListener(COMPOSER_COMMAND_EVENT, listener);
  }, []);
  // Text requested while no composer was mounted, or for the draft this composer has just switched to, lands here.
  // It lands a tick later, as a paste would: the switch commit first applies the draft's restored content, and an
  // insert inside that commit would never let the draft finish applying, so it could not be saved or sent.
  // 没有输入区挂载时请求的文本，或为本输入区刚切换到的草稿请求的文本，落入本输入区。它像粘贴一样晚一拍落入：切换
  // 所在的提交先应用草稿恢复的内容，在该提交内插入会使草稿永远无法完成应用，从而既不能保存也不能发送。
  React.useEffect(() => {
    const timer = window.setTimeout(take, 0);
    return () => window.clearTimeout(timer);
  }, [owner.scopeKey, owner.projectId, owner.isDraft]);
}

// takePendingInsert hands the waiting text to `insert` once and clears it; text meant for a new conversation in
// another project keeps waiting.
// takePendingInsert 将等待中的文本交给 `insert` 一次并清除；发给其他项目新会话的文本继续等待。
function takePendingInsert(owner: ComposerOwner, insert: InsertText): void {
  const pending = pendingInsert;
  if (pending === null || (pending.draft && (!owner.isDraft || owner.projectId !== pending.draft.projectId))) return;
  pendingInsert = null;
  insert(pending.text, pending.files, pending.draft?.promptSkillOverrides ?? null);
}
