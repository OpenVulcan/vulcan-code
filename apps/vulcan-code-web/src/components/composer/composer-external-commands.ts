import React from "react";
import type { ComposerSlashCommandSummary } from "@/types";

// COMPOSER_COMMAND_EVENT carries requests from outside the composer, such as the command palette, to the
// composer that is currently mounted.
// COMPOSER_COMMAND_EVENT 将来自输入区之外（例如命令面板）的请求送到当前挂载的输入区。
const COMPOSER_COMMAND_EVENT = "vulcan:composer-command";

// ComposerExternalCommand is one thing another surface can ask the composer to do: open its model picker,
// or run one of its slash commands by id exactly as if it were chosen in the slash list.
// ComposerExternalCommand 是其他界面可请求输入区执行的一件事：打开模型选择器，或按 id 运行某个 slash 指令，效果与在 slash 列表中选择相同。
export type ComposerExternalCommand = { kind: "model" } | { kind: "slash"; id: string };

// requestComposerCommand asks the mounted composer to run a command; nothing happens when no composer is shown.
// requestComposerCommand 请求已挂载的输入区执行指令；没有显示输入区时不产生任何效果。
export function requestComposerCommand(command: ComposerExternalCommand): void {
  window.dispatchEvent(new CustomEvent<ComposerExternalCommand>(COMPOSER_COMMAND_EVENT, { detail: command }));
}

// useComposerExternalCommands runs requested commands through the composer's own model picker and slash
// activation while the composer is mounted; a slash id the composer does not offer is ignored.
// useComposerExternalCommands 在输入区挂载期间通过其自身的模型选择器与 slash 激活执行收到的指令；输入区未提供的 slash id 会被忽略。
export function useComposerExternalCommands(
  openModelPicker: () => void,
  slashCommands: readonly ComposerSlashCommandSummary[],
  activateSlashCommand: (command: ComposerSlashCommandSummary) => void,
): void {
  const handler = (command: ComposerExternalCommand) => {
    const slash = command.kind === "slash" ? slashCommands.find((entry) => entry.id === command.id) : null;
    if (command.kind === "model") openModelPicker();
    else if (slash) activateSlashCommand(slash);
  };
  const latest = React.useRef(handler);
  latest.current = handler;
  React.useEffect(() => {
    const listener = (event: Event) => latest.current((event as CustomEvent<ComposerExternalCommand>).detail);
    window.addEventListener(COMPOSER_COMMAND_EVENT, listener);
    return () => window.removeEventListener(COMPOSER_COMMAND_EVENT, listener);
  }, []);
}
