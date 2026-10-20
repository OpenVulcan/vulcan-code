import React from "react";
import { ChevronRight, Cpu, MessageSquare, MessagesSquare, Palette, Slash, SquarePen } from "lucide-react";
import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "@/ui-kit/primitives/command";
import { APPEARANCE_THEMES, setAppearance, useAppearance, type AppearanceTheme } from "@/ui-kit/appearance";
import { requestComposerCommand } from "@/components/composer/composer-external-commands";
import { composerSlashCommandDescriptionKey } from "@/components/composer/composer-model";
import { settingsSubAreas } from "@/components/features/settings/settings-navigation";
import type { ActivityRailEntry } from "@/components/shell/activity-rail";
import { useI18n, type TranslationKey } from "@/i18n";
import type { ComposerSlashCommandSummary, SettingsSubAreaId } from "@/types";
import "@/ui-kit/styles/features/command-palette.css";

// THEME_LABEL names each theme the way the appearance menu does.
// THEME_LABEL 以外观菜单的方式命名每个主题。
const THEME_LABEL: Record<AppearanceTheme, TranslationKey> = {
  auto: "appChrome.theme.auto",
  light: "appChrome.theme.light",
  dark: "appChrome.theme.dark",
  command: "appChrome.theme.command",
};

// CommandPalette is one searchable list of everything the workbench can do from anywhere: start a session,
// switch the model or theme, run a slash command in the composer, go to any area or settings page, and, once
// something is typed, open a session by its title or project.
// Ctrl+K (Cmd+K) toggles it, Ctrl+M (Cmd+M) opens the composer's model picker directly and Ctrl+Shift+O
// (Cmd+Shift+O) starts a new session in the current project.
// CommandPalette 是可在任何位置使用的工作台全部操作的可搜索列表：新建会话、切换模型或主题、在输入区运行 slash 指令，
// 前往任意区域或设置页，并在输入内容后按标题或项目打开会话。Ctrl+K（Cmd+K）切换其显示，Ctrl+M（Cmd+M）直接打开输入区的模型选择器，
// Ctrl+Shift+O（Cmd+Shift+O）在当前项目中新建会话。
export function CommandPalette(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  // Areas in rail order, including enabled plugin pages.
  // 按轨道顺序排列的区域，包括已启用的插件页面。
  areas: readonly ActivityRailEntry[];
  // Slash commands the composer offers.
  // 输入区提供的 slash 指令。
  slashCommands: readonly ComposerSlashCommandSummary[];
  onOpenSessions: () => void;
  onOpenSettingsPage: (page: SettingsSubAreaId) => void;
  // Starts a session in the current project; absent while no project is open.
  // 在当前项目中新建会话；没有打开项目时不提供。
  onNewConversation?: () => void;
  // Sessions that can be opened, each with the name of its project when it has one.
  // 可打开的会话，有所属项目时附带项目名称。
  conversations: readonly { id: string; title: string; projectName: string | null }[];
  onOpenConversation: (conversationId: string) => void;
}) {
  const { t } = useI18n();
  const { theme } = useAppearance();
  const { onOpenChange, onNewConversation } = props;
  // query is kept here so sessions are listed only once something is typed, not as a wall of rows on open.
  // query 保存在此，使会话只在输入内容后列出，而不是打开时铺满整列。
  const [query, setQuery] = React.useState("");
  React.useEffect(() => {
    if (!props.open) setQuery("");
  }, [props.open]);
  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      const key = event.key.toLowerCase();
      if (event.shiftKey) {
        if (key === "o" && onNewConversation) {
          event.preventDefault();
          onOpenChange(false);
          onNewConversation();
        }
        return;
      }
      if (key === "k") {
        event.preventDefault();
        onOpenChange(!props.open);
      } else if (key === "m") {
        event.preventDefault();
        requestComposerCommand({ kind: "model" });
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onNewConversation, onOpenChange, props.open]);
  // run closes the palette before the command so focus returns to the page the command acts on.
  // run 在执行指令前关闭面板，使焦点回到指令作用的页面。
  const run = (action: () => void) => () => {
    onOpenChange(false);
    window.requestAnimationFrame(action);
  };
  const nextTheme = APPEARANCE_THEMES[(APPEARANCE_THEMES.indexOf(theme) + 1) % APPEARANCE_THEMES.length];
  return (
    <CommandDialog className="vk-palette" description={t("palette.placeholder")} onOpenChange={onOpenChange} open={props.open} title={t("palette.title")}>
      <Command label={t("palette.title")} loop>
        <CommandInput autoFocus onValueChange={setQuery} placeholder={t("palette.placeholder")} value={query} />
        <CommandList label={t("palette.title")}>
          <CommandEmpty>{t("palette.empty")}</CommandEmpty>
          <CommandGroup heading={t("palette.actions")}>
            {props.onNewConversation ? (
              <CommandItem onSelect={run(props.onNewConversation)} value="new-conversation" keywords={[t("palette.newConversation")]}>
                <SquarePen />
                <span>{t("palette.newConversation")}</span>
                <CommandShortcut>Ctrl Shift O</CommandShortcut>
              </CommandItem>
            ) : null}
            <CommandItem onSelect={run(() => requestComposerCommand({ kind: "model" }))} value="switch-model" keywords={[t("palette.switchModel")]}>
              <Cpu />
              <span>{t("palette.switchModel")}</span>
              <CommandShortcut>Ctrl M</CommandShortcut>
            </CommandItem>
            <CommandItem onSelect={() => setAppearance("theme", nextTheme)} value="cycle-theme" keywords={[t("palette.cycleTheme")]}>
              <Palette />
              <span>{t("palette.cycleTheme")}</span>
              <CommandShortcut>{t(THEME_LABEL[theme])} → {t(THEME_LABEL[nextTheme])}</CommandShortcut>
            </CommandItem>
          </CommandGroup>
          {props.slashCommands.length ? (
            <CommandGroup heading={t("palette.slash")}>
              {props.slashCommands.map((command) => {
                const descriptionKey = composerSlashCommandDescriptionKey(command);
                const description = descriptionKey ? t(descriptionKey) : "";
                return (
                  <CommandItem
                    key={command.id}
                    keywords={[command.command, description]}
                    onSelect={run(() => requestComposerCommand({ kind: "slash", id: command.id }))}
                    value={`slash:${command.id}`}
                  >
                    <Slash />
                    <span className="vk-palette-cmd">/{command.command.replace(/^\//, "")}</span>
                    <span className="vk-palette-desc">{description}</span>
                  </CommandItem>
                );
              })}
            </CommandGroup>
          ) : null}
          {query.trim() && props.conversations.length ? (
            <CommandGroup heading={t("palette.conversations")}>
              {props.conversations.map((conversation) => (
                <CommandItem
                  key={conversation.id}
                  keywords={[conversation.title, conversation.projectName ?? ""]}
                  onSelect={run(() => props.onOpenConversation(conversation.id))}
                  value={`conversation:${conversation.id}`}
                >
                  <MessageSquare />
                  <span className="vk-palette-title">{conversation.title}</span>
                  {conversation.projectName ? <span className="vk-palette-project">{conversation.projectName}</span> : null}
                </CommandItem>
              ))}
            </CommandGroup>
          ) : null}
          <CommandGroup heading={t("palette.go")}>
            <CommandItem onSelect={run(props.onOpenSessions)} value="go:sessions" keywords={[t("area.chat")]}>
              <MessagesSquare />
              <span>{t("area.chat")}</span>
            </CommandItem>
            {props.areas.map((area) => (
              <CommandItem key={area.id} keywords={[area.label]} onSelect={run(area.onSelect)} value={`go:${area.id}`}>
                {area.icon}
                <span>{area.label}</span>
              </CommandItem>
            ))}
          </CommandGroup>
          <CommandGroup heading={t("palette.settings")}>
            {settingsSubAreas(t).map((page) => (
              <CommandItem key={page.id} keywords={[page.label, page.description ?? ""]} onSelect={run(() => props.onOpenSettingsPage(page.id))} value={`settings:${page.id}`}>
                {page.icon}
                <span>{page.label}</span>
                <ChevronRight className="vk-palette-go" />
              </CommandItem>
            ))}
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
