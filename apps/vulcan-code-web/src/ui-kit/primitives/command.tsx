import * as React from "react";
import { SearchIcon } from "lucide-react";
import { composeHandlers, cx, useComposedRefs, useControllableState, useStableId } from "../behavior/core";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "./dialog";

// Command lists are searchable option lists: focus stays in the search field while the arrow keys move the
// active row and Enter runs it. The rows keep the `cmdk-*` attributes of the library they replace, so styles
// and tests that address rows by those attributes keep working.
// 命令列表是可搜索的选项列表：焦点停留在搜索框中，方向键移动活动行，Enter 执行该行。各行保留其替代库的 `cmdk-*`
// 属性，使按这些属性定位行的样式与测试继续有效。

// ITEM_SELECTOR matches the enabled rows in document order.
// ITEM_SELECTOR 按文档顺序匹配可用的行。
const ITEM_SELECTOR = '[cmdk-item]:not([data-disabled="true"])';

// SELECT_EVENT is dispatched on a row to run it from the keyboard.
// SELECT_EVENT 在行上派发，用于从键盘执行该行。
const SELECT_EVENT = "vk-command-select";

// matches reports whether every word of the search appears in the row's value or keywords.
// matches 判断搜索中的每个词是否都出现在行的值或关键词中。
function matches(search: string, value: string, keywords: readonly string[] | undefined): boolean {
  const haystack = [value, ...(keywords ?? [])].join(" ").toLocaleLowerCase();
  return search.toLocaleLowerCase().split(/\s+/).filter(Boolean).every((word) => haystack.includes(word));
}

// CommandContextValue is the search and the active row shared by the parts.
// CommandContextValue 是各部件共享的搜索内容与活动行。
interface CommandContextValue {
  search: string;
  setSearch: (search: string) => void;
  filtering: boolean;
  active: string | null;
  setActive: (value: string | null) => void;
  listId: string;
  visibleCount: number;
  report: (key: string, visible: boolean) => void;
}

const CommandContext = React.createContext<CommandContextValue | null>(null);

// useCommand reads the enclosing command list.
// useCommand 读取外层命令列表。
function useCommand(): CommandContextValue {
  const context = React.useContext(CommandContext);
  if (!context) throw new Error("Command parts must be rendered inside a Command.");
  return context;
}

// CommandProps follow the cmdk root.
// CommandProps 沿用 cmdk 根组件。
export interface CommandProps extends Omit<React.ComponentProps<"div">, "onChange"> {
  shouldFilter?: boolean;
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  loop?: boolean;
  label?: string;
}

// Command holds the search and the active row, and handles the list keys for the whole tree.
// Command 保存搜索内容与活动行，并为整棵树处理列表按键。
function Command({ className, shouldFilter = true, value, defaultValue, onValueChange, loop = false, label, onKeyDown, ref, children, ...props }: CommandProps) {
  const [search, setSearch] = React.useState("");
  const [active, setActiveState] = useControllableState<string | null>(value, defaultValue ?? null, onValueChange as ((value: string | null) => void) | undefined);
  const root = React.useRef<HTMLDivElement | null>(null);
  const composed = useComposedRefs(root, ref);
  const listId = useStableId("vk-command-list");
  const visible = React.useRef(new Map<string, boolean>());
  const [visibleCount, setVisibleCount] = React.useState(0);
  const report = React.useCallback((key: string, isVisible: boolean) => {
    if (isVisible) visible.current.set(key, true);
    else visible.current.delete(key);
    setVisibleCount(visible.current.size);
  }, []);
  const hasRows = visibleCount > 0;
  const rows = () => Array.from(root.current?.querySelectorAll<HTMLElement>(ITEM_SELECTOR) ?? []);
  const setActive = React.useCallback((next: string | null) => setActiveState(next), [setActiveState]);
  // On mount and whenever the search changes, the caller's default row becomes active while the search is empty,
  // otherwise the first visible row does.
  // 挂载时以及每次搜索变化时，搜索为空则调用方的默认行成为活动行，否则第一条可见行成为活动行。
  React.useLayoutEffect(() => {
    const list = Array.from(root.current?.querySelectorAll<HTMLElement>(ITEM_SELECTOR) ?? []);
    const preferred = search === "" && defaultValue ? list.find((row) => row.dataset.value === defaultValue) : undefined;
    setActive((preferred ?? list[0])?.dataset.value ?? null);
  }, [search, hasRows, setActive, defaultValue]);
  // The search field names the active row for assistive technology after every render.
  // 每次渲染后，搜索框都会为辅助技术指明活动行。
  React.useLayoutEffect(() => {
    const input = root.current?.querySelector<HTMLInputElement>("[cmdk-input]");
    if (!input) return;
    const row = rows().find((item) => item.dataset.value === active);
    if (row) input.setAttribute("aria-activedescendant", row.id);
    else input.removeAttribute("aria-activedescendant");
  });
  const move = (offset: number | "first" | "last") => {
    const list = rows();
    if (list.length === 0) return;
    const index = list.findIndex((row) => row.dataset.value === active);
    let next: HTMLElement | undefined;
    if (offset === "first") next = list[0];
    else if (offset === "last") next = list[list.length - 1];
    else next = list[index + offset] ?? (loop ? list[offset > 0 ? 0 : list.length - 1] : list[index < 0 ? 0 : index]);
    if (!next) return;
    setActive(next.dataset.value ?? null);
    next.scrollIntoView?.({ block: "nearest" });
  };
  const context = React.useMemo<CommandContextValue>(() => ({
    search,
    setSearch,
    filtering: shouldFilter,
    active,
    setActive,
    listId,
    visibleCount,
    report,
  }), [search, shouldFilter, active, setActive, listId, visibleCount, report]);
  return (
    <CommandContext.Provider value={context}>
      <div
        ref={composed}
        cmdk-root=""
        data-slot="command"
        aria-label={label}
        className={cx("vk-command", className)}
        onKeyDown={composeHandlers(onKeyDown, (event) => {
          if (event.nativeEvent.isComposing) return;
          switch (event.key) {
            case "ArrowDown":
              event.preventDefault();
              move(1);
              break;
            case "ArrowUp":
              event.preventDefault();
              move(-1);
              break;
            case "Home":
              event.preventDefault();
              move("first");
              break;
            case "End":
              event.preventDefault();
              move("last");
              break;
            case "Enter": {
              const row = rows().find((item) => item.dataset.value === active);
              if (!row) return;
              event.preventDefault();
              row.dispatchEvent(new Event(SELECT_EVENT));
              break;
            }
          }
        })}
        {...props}
      >
        {children}
      </div>
    </CommandContext.Provider>
  );
}

// CommandInput is the search field; focus stays here while the active row moves.
// CommandInput 是搜索框；活动行移动时焦点停留在此处。
function CommandInput({ className, value, onValueChange, onChange, ...props }: Omit<React.ComponentProps<"input">, "value" | "type"> & {
  value?: string;
  onValueChange?: (search: string) => void;
}) {
  const command = useCommand();
  const { setSearch } = command;
  React.useLayoutEffect(() => {
    if (value !== undefined) setSearch(value);
  }, [value, setSearch]);
  return (
    <div data-slot="command-input-wrapper" className="vk-command-search">
      <SearchIcon aria-hidden="true" />
      <input
        cmdk-input=""
        type="text"
        role="combobox"
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
        aria-autocomplete="list"
        aria-expanded
        aria-controls={command.listId}
        data-slot="command-input"
        className={cx("vk-command-input", className)}
        value={value ?? command.search}
        onChange={composeHandlers(onChange, (event) => {
          if (value === undefined) setSearch(event.target.value);
          onValueChange?.(event.target.value);
        })}
        {...props}
      />
    </div>
  );
}

// CommandList is the scrollable listbox.
// CommandList 是可滚动的列表框。
function CommandList({ className, children, label, ...props }: React.ComponentProps<"div"> & { label?: string }) {
  const command = useCommand();
  return (
    <div cmdk-list="" role="listbox" id={command.listId} aria-label={label} tabIndex={-1} data-slot="command-list" className={cx("vk-command-list", className)} {...props}>
      <div cmdk-list-sizer="">{children}</div>
    </div>
  );
}

// CommandEmpty renders only while no row matches the search.
// CommandEmpty 仅在没有行匹配搜索时渲染。
function CommandEmpty({ className, ...props }: React.ComponentProps<"div">) {
  const command = useCommand();
  if (command.visibleCount > 0) return null;
  return <div cmdk-empty="" role="presentation" data-slot="command-empty" className={cx("vk-command-empty", className)} {...props} />;
}

// GroupContext lets a group hide itself when none of its rows match.
// GroupContext 让组在其各行均不匹配时隐藏自身。
const GroupContext = React.createContext<((key: string, visible: boolean) => void) | null>(null);

// CommandGroup groups rows under an optional heading.
// CommandGroup 将各行归入可选标题下。
function CommandGroup({ className, heading, children, ...props }: Omit<React.ComponentProps<"div">, "title"> & { heading?: React.ReactNode; value?: string; forceMount?: boolean }) {
  const headingId = useStableId("vk-command-group");
  const visible = React.useRef(new Set<string>());
  const [count, setCount] = React.useState(-1);
  const report = React.useCallback((key: string, isVisible: boolean) => {
    if (isVisible) visible.current.add(key);
    else visible.current.delete(key);
    setCount(visible.current.size);
  }, []);
  return (
    <GroupContext.Provider value={report}>
      <div cmdk-group="" role="presentation" hidden={count === 0 || undefined} data-slot="command-group" className={cx("vk-command-group", className)} {...props}>
        {heading ? <div cmdk-group-heading="" aria-hidden="true" id={headingId}>{heading}</div> : null}
        <div cmdk-group-items="" role="group" aria-labelledby={heading ? headingId : undefined}>{children}</div>
      </div>
    </GroupContext.Provider>
  );
}

// CommandSeparator divides groups.
// CommandSeparator 分隔各组。
function CommandSeparator({ className, alwaysRender: _alwaysRender, ...props }: React.ComponentProps<"div"> & { alwaysRender?: boolean }) {
  return <div cmdk-separator="" role="separator" data-slot="command-separator" className={cx("vk-menu-sep", className)} {...props} />;
}

// CommandItemProps follow the cmdk item.
// CommandItemProps 沿用 cmdk 列表项。
export interface CommandItemProps extends Omit<React.ComponentProps<"div">, "onSelect" | "value"> {
  value?: string;
  keywords?: string[];
  disabled?: boolean;
  onSelect?: (value: string) => void;
  forceMount?: boolean;
}

// CommandItem is one row; it hides itself while the search excludes it.
// CommandItem 是一行；被搜索排除时隐藏自身。
function CommandItem({ className, value: givenValue, keywords, disabled = false, onSelect, forceMount, children, ref, onPointerMove, onClick, ...props }: CommandItemProps) {
  const command = useCommand();
  const group = React.useContext(GroupContext);
  const key = useStableId("vk-command-item");
  const element = React.useRef<HTMLDivElement | null>(null);
  const composed = useComposedRefs(element, ref);
  const value = (givenValue ?? (typeof children === "string" ? children : key)).trim();
  const visible = forceMount || !command.filtering || command.search === "" || matches(command.search, value, keywords);
  const selected = command.active === value && !disabled;
  const { report } = command;
  React.useLayoutEffect(() => {
    report(key, visible);
    group?.(key, visible);
    return () => {
      report(key, false);
      group?.(key, false);
    };
  }, [key, visible, report, group]);
  // Keyboard selection reaches the row through SELECT_EVENT so the latest onSelect runs.
  // 键盘选择通过 SELECT_EVENT 到达该行，以便执行最新的 onSelect。
  const latestSelect = React.useRef(onSelect);
  latestSelect.current = onSelect;
  React.useEffect(() => {
    const node = element.current;
    if (!node) return;
    const run = () => latestSelect.current?.(value);
    node.addEventListener(SELECT_EVENT, run);
    return () => node.removeEventListener(SELECT_EVENT, run);
  }, [value, visible]);
  if (!visible) return null;
  return (
    <div
      ref={composed}
      cmdk-item=""
      id={key}
      role="option"
      aria-selected={selected}
      aria-disabled={disabled}
      data-selected={selected || undefined}
      data-disabled={disabled}
      data-value={value}
      data-slot="command-item"
      className={cx("vk-command-item", className)}
      onPointerMove={composeHandlers(onPointerMove, () => {
        if (!disabled && command.active !== value) command.setActive(value);
      })}
      onClick={composeHandlers(onClick, () => {
        if (!disabled) onSelect?.(value);
      })}
      {...props}
    >
      {children}
    </div>
  );
}

// CommandShortcut shows a row's keyboard shortcut at its trailing edge.
// CommandShortcut 在行尾显示该行的键盘快捷键。
function CommandShortcut({ className, ...props }: React.ComponentProps<"span">) {
  return <span data-slot="command-shortcut" className={cx("vk-menu-meta", className)} {...props} />;
}

// CommandDialog shows a command list in a modal with a screen-reader title.
// CommandDialog 在带有读屏标题的模态框中显示命令列表。
function CommandDialog({ title = "Command Palette", description = "Search for a command to run...", children, className, showCloseButton = false, ...props }: React.ComponentProps<typeof Dialog> & {
  title?: string;
  description?: string;
  className?: string;
  showCloseButton?: boolean;
}) {
  return (
    <Dialog {...props}>
      <DialogContent className={cx("vk-command-dialog", className)} scrollMode="content-scroll" layout="viewer" showCloseButton={showCloseButton}>
        <DialogHeader className="vk-sr-only">
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  );
}

export {
  Command,
  CommandDialog,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandShortcut,
  CommandSeparator,
};
