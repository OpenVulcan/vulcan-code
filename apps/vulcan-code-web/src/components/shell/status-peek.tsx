import React from "react";
import { Settings2 } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui-kit/primitives/popover";

// STATUS_PEEK_OPEN_DELAY_MS and STATUS_PEEK_CLOSE_DELAY_MS pace the hover peek, so crossing the bar does not
// flash cards and the pointer can travel from the item into its card.
// STATUS_PEEK_OPEN_DELAY_MS 与 STATUS_PEEK_CLOSE_DELAY_MS 控制悬停预览的节奏：划过状态栏不会闪出卡片，指针也能从状态项移入卡片。
const STATUS_PEEK_OPEN_DELAY_MS = 160;
const STATUS_PEEK_CLOSE_DELAY_MS = 220;

// StatusPeekTone is the tone of a status item and of the facts inside its card.
// StatusPeekTone 是状态项及其卡片内事实的色调。
export type StatusPeekTone = "ok" | "warn" | "bad" | "ember" | undefined;

// StatusPeek is a status-bar item whose state is read, not navigated: hovering shows its card, pressing pins the
// card open, and only the card's settings button leaves for the page that owns the state.
// StatusPeek 是只供查看、不直接跳转的状态栏项：悬停显示其卡片，按下将卡片固定打开，只有卡片内的设置按钮才会前往持有该状态的页面。
export function StatusPeek(props: {
  // Item content shown in the bar.
  // 状态栏中显示的项内容。
  children: React.ReactNode;
  // Card body.
  // 卡片正文。
  card: React.ReactNode;
  // Card heading and its one-line summary.
  // 卡片标题及其一行摘要。
  title: string;
  summary?: React.ReactNode;
  // Accessible name of the item.
  // 状态项的无障碍名称。
  label: string;
  tone?: StatusPeekTone;
  // Hides the item first when the bar runs out of room.
  // 状态栏空间不足时优先隐藏该项。
  optional?: boolean;
  // Label and action of the card's settings button.
  // 卡片设置按钮的标签与动作。
  settingsLabel: string;
  onOpenSettings: () => void;
  // Called whenever the card opens, so its owner can read fresh state.
  // 每次卡片打开时调用，使其持有者读取最新状态。
  onOpen?: () => void;
}) {
  const [open, setOpen] = React.useState(false);
  // pinned marks a card opened by pressing the item or the card; hover leaving never closes a pinned card.
  // pinned 标记由按下状态项或卡片打开的卡片；悬停离开不会关闭已固定的卡片。
  const pinned = React.useRef(false);
  const timer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const onOpen = props.onOpen;
  const cancel = React.useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }, []);
  React.useEffect(() => cancel, [cancel]);
  const change = React.useCallback((next: boolean) => {
    cancel();
    if (next) onOpen?.();
    else pinned.current = false;
    setOpen(next);
  }, [cancel, onOpen]);
  const schedule = (next: boolean) => {
    cancel();
    if (!next && pinned.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      change(next);
    }, next ? STATUS_PEEK_OPEN_DELAY_MS : STATUS_PEEK_CLOSE_DELAY_MS);
  };
  return (
    <Popover onOpenChange={change} open={open}>
      <PopoverTrigger
        aria-label={props.label}
        className="vk-status-item"
        data-optional={props.optional ? "" : undefined}
        data-tone={props.tone}
        // Pressing pins the card instead of toggling it away, so a click after a hover peek keeps it open.
        // 按下会固定卡片而不是将其切走，因此悬停预览后的点击会让卡片保持打开。
        onClick={(event) => {
          event.preventDefault();
          if (open && !pinned.current) {
            pinned.current = true;
            cancel();
            return;
          }
          pinned.current = !open;
          change(!open);
        }}
        onPointerEnter={(event) => { if (event.pointerType === "mouse") { cancel(); if (!open) schedule(true); } }}
        onPointerLeave={(event) => { if (event.pointerType === "mouse") { if (open) schedule(false); else cancel(); } }}
        type="button"
      >
        {props.children}
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="vk-peek"
        onCloseAutoFocus={(event) => event.preventDefault()}
        onOpenAutoFocus={(event) => { if (!pinned.current) event.preventDefault(); }}
        onPointerDown={() => { pinned.current = true; cancel(); }}
        onPointerEnter={cancel}
        onPointerLeave={(event) => { if (event.pointerType === "mouse") schedule(false); }}
        side="top"
      >
        <header className="vk-peek-head">
          <div>
            <strong>{props.title}</strong>
            {props.summary ? <small>{props.summary}</small> : null}
          </div>
          <button
            className="vk-btn"
            data-size="sm"
            data-variant="ghost"
            onClick={() => { change(false); props.onOpenSettings(); }}
            type="button"
          >
            <Settings2 aria-hidden="true" />
            <span>{props.settingsLabel}</span>
          </button>
        </header>
        <div className="vk-peek-body">{props.card}</div>
      </PopoverContent>
    </Popover>
  );
}

// StatusPeekFacts lists labelled facts inside a status card.
// StatusPeekFacts 在状态卡片中列出带标签的事实。
export function StatusPeekFacts(props: { facts: ReadonlyArray<{ label: string; value: React.ReactNode; tone?: StatusPeekTone }> }) {
  return (
    <dl className="vk-peek-facts">
      {props.facts.map((fact) => (
        <div key={fact.label}>
          <dt>{fact.label}</dt>
          <dd data-tone={fact.tone}>{fact.value}</dd>
        </div>
      ))}
    </dl>
  );
}
