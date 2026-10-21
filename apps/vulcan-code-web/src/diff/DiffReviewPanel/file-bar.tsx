import React from "react";
import { ChevronDown, ChevronUp, ChevronsUpDown, FolderOpen, FolderTree, Info, List, MoreHorizontal, Search } from "lucide-react";
import "@/ui-kit/styles/features/file-bar.css";
import { IconButton } from "@/ui-kit/primitives/button";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui-kit/primitives/popover";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui-kit/primitives/dropdown-menu";
import type { WorkspacePathAction } from "@/components/workspace/workspace-path-actions";
import { formatMessage, useI18n } from "@/i18n";
import type { DiffFile } from "../types";
import { diffRevealRelativePath } from "./search";

// DIFF_FILE_KIND gives each change status the one-letter mark Git users already read.
// DIFF_FILE_KIND 为每种变更状态提供 Git 用户熟悉的单字母标记。
const DIFF_FILE_KIND: Record<DiffFile["status"], string> = { added: "A", deleted: "D", modified: "M", renamed: "R", binary: "B" };

// DiffFileMenuView is how the file menu lays out the changed files.
// DiffFileMenuView 是文件菜单排列变更文件的方式。
type DiffFileMenuView = "tree" | "list";

// DiffFileBar names the one file the review shows and moves between files: its picker opens a menu of every
// changed file grouped by folder or as a flat list, and the step buttons walk the files in order.
// DiffFileBar 标明审核当前展示的单个文件并在文件间移动：选择器打开按目录分组或平铺的全部变更文件菜单，步进按钮按顺序逐个切换。
export function DiffFileBar(props: {
  // Every changed file of the review, in folder order (see diffFilesInFolderOrder).
  // 审核中的全部变更文件，按目录顺序排列（见 diffFilesInFolderOrder）。
  files: readonly DiffFile[];
  // Key of the file being shown.
  // 正在展示的文件键。
  activeKey: string;
  // Shows another file.
  // 展示另一个文件。
  onSelect: (file: DiffFile) => void;
  // Actions for the shown file, offered in a menu after the step buttons; none hides the menu.
  // 当前文件的操作，在步进按钮之后以菜单提供；为空时隐藏菜单。
  actions?: readonly WorkspacePathAction[];
}) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  const [filter, setFilter] = React.useState("");
  const [view, setView] = React.useState<DiffFileMenuView>("tree");
  const listRef = React.useRef<HTMLDivElement | null>(null);
  const index = props.files.findIndex((file) => file.key === props.activeKey);
  const file = props.files[index];
  React.useEffect(() => {
    if (!open) return;
    const frame = window.requestAnimationFrame(() => {
      listRef.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [open]);
  if (!file) return null;
  const [dir, name] = splitDiffFilePath(diffFilePath(file));
  const needle = filter.trim().toLowerCase();
  const shown = needle ? props.files.filter((entry) => diffFilePath(entry).toLowerCase().includes(needle)) : props.files;
  const totals = props.files.reduce((sum, entry) => ({ additions: sum.additions + entry.additions, deletions: sum.deletions + entry.deletions }), { additions: 0, deletions: 0 });
  const choose = (next: DiffFile) => {
    setOpen(false);
    props.onSelect(next);
  };
  // moveFocus walks the menu rows with the arrow keys, starting from the filter field.
  // moveFocus 用方向键在菜单行间移动，从筛选框开始。
  const moveFocus = (event: React.KeyboardEvent, from: Element | null) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const rows = Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>(".vk-filemenu-item") ?? []);
    if (!rows.length) return;
    event.preventDefault();
    const at = from ? rows.indexOf(from as HTMLButtonElement) : -1;
    const next = event.key === "ArrowDown" ? Math.min(rows.length - 1, at + 1) : Math.max(0, at - 1);
    rows[at < 0 && event.key === "ArrowUp" ? rows.length - 1 : next]?.focus();
  };
  return (
    <div className="vk-filebar">
      <Popover onOpenChange={(next) => { setOpen(next); if (!next) setFilter(""); }} open={open}>
        <PopoverTrigger aria-haspopup="listbox" aria-label={`${t("diffFile.pick")} · ${diffFilePath(file)}`} className="vk-filepick" title={diffFilePath(file)} type="button">
          <DiffFileKind file={file} />
          <span className="vk-filepick-name">
            <strong>{name}</strong>
            {dir ? <small>{dir}</small> : null}
          </span>
          <DiffFileStat additions={file.additions} deletions={file.deletions} />
          <ChevronsUpDown aria-hidden="true" className="vk-faint" size={13} />
        </PopoverTrigger>
        <PopoverContent align="start" className="vk-filemenu" sideOffset={2}>
          <div className="vk-filemenu-head">
            <div className="vk-input-wrap">
              <Search aria-hidden="true" size={13} />
              <input
                aria-label={t("diffFile.filter")}
                autoComplete="off"
                className="vk-input"
                data-size="sm"
                onChange={(event) => setFilter(event.target.value)}
                onKeyDown={(event) => moveFocus(event, null)}
                placeholder={t("diffFile.filter")}
                value={filter}
              />
            </div>
            <SegmentedControl aria-label={t("diffFile.tree")} onValueChange={(value) => setView(value === "list" ? "list" : "tree")} value={view}>
              <SegmentedControlItem aria-label={t("diffFile.tree")} title={t("diffFile.tree")} value="tree"><FolderTree size={13} /></SegmentedControlItem>
              <SegmentedControlItem aria-label={t("diffFile.list")} title={t("diffFile.list")} value="list"><List size={13} /></SegmentedControlItem>
            </SegmentedControl>
          </div>
          <div
            className="vk-filemenu-list"
            onKeyDown={(event) => moveFocus(event, document.activeElement)}
            ref={listRef}
            role="listbox"
            aria-label={t("diffFile.pick")}
          >
            {shown.length === 0 ? <p className="vk-filemenu-none">{t("diff.noMatchingFiles")}</p> : null}
            {view === "list"
              ? shown.map((entry) => <DiffFileMenuItem active={entry.key === file.key} file={entry} key={entry.key} label={diffFilePath(entry)} onChoose={choose} />)
              : groupDiffFilesByFolder(shown).map(([folder, entries]) => (
                <React.Fragment key={folder || "."}>
                  {folder ? (
                    <div className="vk-filemenu-dir" title={folder}>
                      <FolderOpen aria-hidden="true" size={13} />
                      <span>{folder}</span>
                    </div>
                  ) : null}
                  {entries.map((entry) => (
                    <DiffFileMenuItem active={entry.key === file.key} file={entry} key={entry.key} label={splitDiffFilePath(diffFilePath(entry))[1]} nested={Boolean(folder)} onChoose={choose} />
                  ))}
                </React.Fragment>
              ))}
          </div>
          <div className="vk-filemenu-foot">
            <Info aria-hidden="true" size={12} />
            <span>{formatMessage(t("diffFile.summary"), { count: props.files.length })}</span>
            <DiffFileStat additions={totals.additions} deletions={totals.deletions} />
          </div>
        </PopoverContent>
      </Popover>
      <span className="vk-filebar-pos">{index + 1}/{props.files.length}</span>
      <IconButton aria-label={t("diffFile.prev")} disabled={index <= 0} onClick={() => props.onSelect(props.files[index - 1])} size="sm" title={`${t("diffFile.prev")}  Alt ↑`}>
        <ChevronUp size={14} />
      </IconButton>
      <IconButton aria-label={t("diffFile.next")} disabled={index >= props.files.length - 1} onClick={() => props.onSelect(props.files[index + 1])} size="sm" title={`${t("diffFile.next")}  Alt ↓`}>
        <ChevronDown size={14} />
      </IconButton>
      {props.actions?.length ? (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <IconButton aria-label={t("workspaceFiles.fileActions")} size="sm" title={t("workspaceFiles.fileActions")}>
              <MoreHorizontal size={14} />
            </IconButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {props.actions.map((action) => (
              <DropdownMenuItem disabled={action.disabled} key={action.key} onClick={action.run}>{action.icon}{action.label}</DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </div>
  );
}

// DiffFileMenuItem is one selectable changed file in the file menu.
// DiffFileMenuItem 是文件菜单中一个可选择的变更文件。
function DiffFileMenuItem(props: { file: DiffFile; label: string; active: boolean; nested?: boolean; onChoose: (file: DiffFile) => void }) {
  return (
    <button
      aria-selected={props.active}
      className="vk-filemenu-item"
      data-nested={props.nested ? "" : undefined}
      onClick={() => props.onChoose(props.file)}
      role="option"
      title={diffFilePath(props.file)}
      type="button"
    >
      <DiffFileKind file={props.file} />
      <span>{props.label}</span>
      <DiffFileStat additions={props.file.additions} deletions={props.file.deletions} />
    </button>
  );
}

// DiffFileKind renders a file's one-letter change mark.
// DiffFileKind 渲染文件的单字母变更标记。
function DiffFileKind(props: { file: DiffFile }) {
  return <span className="vk-file-kind" data-k={DIFF_FILE_KIND[props.file.status]}>{DIFF_FILE_KIND[props.file.status]}</span>;
}

// DiffFileStat renders added and removed line counts.
// DiffFileStat 渲染新增与删除的行数。
function DiffFileStat(props: { additions: number; deletions: number }) {
  return (
    <span className="vk-filestat">
      <span className="vk-add">+{props.additions}</span>
      <span className="vk-del">−{props.deletions}</span>
    </span>
  );
}

// diffFilePath returns the path a file is known by after the change.
// diffFilePath 返回文件变更后的路径。
function diffFilePath(file: DiffFile): string {
  return diffRevealRelativePath(file) ?? file.key;
}

// splitDiffFilePath splits a path into its folder and file name.
// splitDiffFilePath 将路径拆分为目录与文件名。
function splitDiffFilePath(path: string): [string, string] {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? ["", path] : [path.slice(0, slash), path.slice(slash + 1)];
}

// diffFilesInFolderOrder orders files the way the file menu groups them, so stepping and the menu agree.
// diffFilesInFolderOrder 按文件菜单的分组方式排列文件，使逐个切换与菜单顺序一致。
export function diffFilesInFolderOrder(files: readonly DiffFile[]): DiffFile[] {
  return groupDiffFilesByFolder(files).flatMap(([, entries]) => entries);
}

// groupDiffFilesByFolder groups files by folder, keeping the order in which each folder first appears.
// groupDiffFilesByFolder 按目录分组文件，保持每个目录首次出现的顺序。
function groupDiffFilesByFolder(files: readonly DiffFile[]): Array<[string, DiffFile[]]> {
  const groups = new Map<string, DiffFile[]>();
  for (const file of files) {
    const folder = splitDiffFilePath(diffFilePath(file))[0];
    const group = groups.get(folder);
    if (group) group.push(file);
    else groups.set(folder, [file]);
  }
  return [...groups];
}
