import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { useI18n, type TranslationKey } from "@/i18n";

// ShortcutRow is one action with the key combinations that run it; each combination lists the keys pressed together.
// ShortcutRow 是一项操作及触发它的按键组合；每个组合列出需同时按下的键。
interface ShortcutRow {
  combos: readonly (readonly string[])[];
  label: TranslationKey;
}

// SHORTCUT_GROUPS lists the keyboard shortcuts the workbench answers, grouped by where they work. Key caps are
// written as printed on the keyboard, so they are the same in every language.
// SHORTCUT_GROUPS 按生效位置分组列出工作台响应的键盘快捷键。键帽按键盘上的印字书写，因此在所有语言中相同。
const SHORTCUT_GROUPS: readonly { title: TranslationKey; rows: readonly ShortcutRow[] }[] = [
  {
    title: "palette.shortcuts.anywhere",
    rows: [
      { combos: [["Ctrl", "K"]], label: "palette.title" },
      { combos: [["Ctrl", "Shift", "O"]], label: "palette.newConversation" },
      { combos: [["Ctrl", "M"]], label: "palette.switchModel" },
      { combos: [["Ctrl", "B"]], label: "sidebar.toggle" },
      { combos: [["Alt", "↑"], ["Alt", "↓"]], label: "palette.shortcuts.sessionStep" },
      { combos: [["Ctrl", "F"]], label: "conversation.find.label" },
      { combos: [["Ctrl", "+"], ["Ctrl", "-"], ["Ctrl", "0"]], label: "appChrome.text" },
    ],
  },
  {
    title: "palette.shortcuts.composer",
    rows: [
      { combos: [["⏎"]], label: "palette.shortcuts.send" },
      { combos: [["Ctrl", "⏎"]], label: "palette.shortcuts.sendAlternate" },
      { combos: [["Shift", "⏎"]], label: "palette.shortcuts.newline" },
      { combos: [["⏎"]], label: "palette.shortcuts.stop" },
      { combos: [["↑"], ["↓"]], label: "palette.shortcuts.recall" },
      { combos: [["/"]], label: "palette.shortcuts.slash" },
      { combos: [["@"]], label: "palette.shortcuts.mention" },
    ],
  },
  {
    title: "palette.shortcuts.review",
    rows: [
      { combos: [["Alt", "↑"], ["Alt", "↓"]], label: "palette.shortcuts.reviewFileStep" },
      { combos: [["Alt", "Shift", "↑"], ["Alt", "Shift", "↓"]], label: "palette.shortcuts.reviewChangeStep" },
    ],
  },
  {
    title: "palette.shortcuts.filePreview",
    rows: [
      { combos: [["Ctrl", "F"]], label: "workspaceFiles.find" },
      { combos: [["Ctrl", "G"]], label: "workspaceFiles.gotoLineLabel" },
    ],
  },
  {
    title: "palette.shortcuts.lists",
    rows: [
      { combos: [["↑"], ["↓"], ["Home"], ["End"]], label: "palette.shortcuts.move" },
      { combos: [["←"], ["→"]], label: "palette.shortcuts.tree" },
      { combos: [["←"], ["→"]], label: "palette.shortcuts.tabs" },
      { combos: [["Esc"]], label: "palette.shortcuts.close" },
    ],
  },
];

// KeyboardShortcutsDialog lists every keyboard shortcut, opened from the command palette.
// KeyboardShortcutsDialog 列出全部键盘快捷键，从命令面板打开。
export function KeyboardShortcutsDialog(props: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const { t } = useI18n();
  return (
    <Dialog onOpenChange={props.onOpenChange} open={props.open}>
      <DialogContent className="vk-shortcuts-dialog" size="sm">
        <DialogHeader>
          <DialogTitle>{t("palette.shortcuts")}</DialogTitle>
        </DialogHeader>
        {SHORTCUT_GROUPS.map((group) => (
          <section className="vk-shortcuts-group" key={group.title}>
            <h3>{t(group.title)}</h3>
            <dl className="vk-shortcuts">
              {group.rows.map((row) => (
                <div key={row.label}>
                  <dt>{t(row.label)}</dt>
                  <dd>
                    {row.combos.map((combo) => <kbd className="vk-kbd" key={combo.join("+")}>{combo.join(" ")}</kbd>)}
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </DialogContent>
    </Dialog>
  );
}
