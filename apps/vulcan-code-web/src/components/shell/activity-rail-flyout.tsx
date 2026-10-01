import React from "react";

// ActivityRailFlyoutAction describes the one action pinned below the flyout content.
// ActivityRailFlyoutAction 描述固定在浮层内容下方的唯一动作。
export type ActivityRailFlyoutAction = {
  // Visible label.
  // 可见名称。
  label: string;
  // Leading icon.
  // 前置图标。
  icon: React.ReactNode;
  // Runs the action; the flyout closes first.
  // 执行动作；浮层会先关闭。
  onSelect: () => void;
};

// FLYOUT_CLOSE_DELAY_MS keeps the flyout open while the pointer crosses the gap from the trigger.
// FLYOUT_CLOSE_DELAY_MS 让指针从触发器跨越间隙时浮层保持打开。
const FLYOUT_CLOSE_DELAY_MS = 140;

// FLYOUT_OVERLAY_SELECTOR matches menus, popovers and dialogs portaled out of the flyout content.
// FLYOUT_OVERLAY_SELECTOR 匹配从浮层内容中传送出去的菜单、弹出层与对话框。
const FLYOUT_OVERLAY_SELECTOR = "[data-radix-popper-content-wrapper], [role='dialog'], [role='alertdialog']";

// hasOpenOverlayTrigger reports whether a menu or popover opened from inside the flyout is still open.
// hasOpenOverlayTrigger 判断从浮层内部打开的菜单或弹出层是否仍处于打开状态。
// Parameters: the flyout panel. Returns true while one of its triggers owns an open overlay.
// 参数：浮层面板。其中某个触发器持有打开的浮层时返回 true。
function hasOpenOverlayTrigger(panel: HTMLElement | null): boolean {
  return Boolean(panel?.querySelector("[aria-haspopup][data-state='open'], [aria-haspopup][aria-expanded='true']"));
}

// ActivityRailFlyout opens a full-height panel beside the rail on hover or keyboard focus of a rail button.
// The panel starts below the brand strip, hosts arbitrary content, and stays open while a menu opened from it is open.
// ActivityRailFlyout 在悬停或键盘聚焦轨道按钮时，于轨道旁打开铺满高度的面板。
// 面板从品牌条下方开始，可承载任意内容，并在其内部打开的菜单存续期间保持打开。
export function ActivityRailFlyout(props: {
  // Rail button that anchors the flyout.
  // 锚定浮层的轨道按钮。
  children: React.ReactNode;
  // Content shown in the scrollable body.
  // 可滚动主体中显示的内容。
  content: React.ReactNode;
  // Optional action pinned below the body.
  // 可选的、固定在主体下方的动作。
  footer?: ActivityRailFlyoutAction;
  // Accessible panel name and visible heading.
  // 面板的无障碍名称与可见标题。
  title: string;
  // Closes the flyout whenever this value changes, for example after a destination is chosen inside it.
  // 该值变化时关闭浮层，例如在其中选定目标之后。
  dismissKey?: unknown;
}) {
  const anchorRef = React.useRef<HTMLDivElement | null>(null);
  const panelRef = React.useRef<HTMLDivElement | null>(null);
  const closeTimerRef = React.useRef<number | null>(null);
  const [left, setLeft] = React.useState<number | null>(null);

  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);
  // open places the flyout at the rail's right edge (or the anchor's outside a rail); CSS adds the gap.
  // open 将浮层放在轨道右边缘（不在轨道内时为锚点右边缘）；间距由 CSS 添加。
  const open = React.useCallback(() => {
    clearCloseTimer();
    const edge = anchorRef.current?.closest(".activity-rail") ?? anchorRef.current;
    const rect = edge?.getBoundingClientRect();
    if (rect) {
      setLeft(rect.right);
    }
  }, [clearCloseTimer]);
  const close = React.useCallback(() => {
    clearCloseTimer();
    setLeft(null);
  }, [clearCloseTimer]);
  // scheduleClose defers closing, and keeps the panel while a menu opened from it still needs its trigger.
  // scheduleClose 延迟关闭；其内部打开的菜单仍需要触发器时保留面板。
  const scheduleClose = React.useCallback(() => {
    clearCloseTimer();
    closeTimerRef.current = window.setTimeout(() => {
      closeTimerRef.current = null;
      if (!hasOpenOverlayTrigger(panelRef.current)) setLeft(null);
    }, FLYOUT_CLOSE_DELAY_MS);
  }, [clearCloseTimer]);
  React.useEffect(() => clearCloseTimer, [clearCloseTimer]);
  // A new destination closes the flyout; the first render only records the key.
  // 新目标会关闭浮层；首次渲染只记录该值。
  const dismissKeyRef = React.useRef(props.dismissKey);
  React.useEffect(() => {
    if (dismissKeyRef.current !== props.dismissKey) {
      dismissKeyRef.current = props.dismissKey;
      close();
    }
  }, [close, props.dismissKey]);
  // While open, a press outside the rail button, the panel and its overlays closes the panel.
  // 打开期间，在轨道按钮、面板及其浮层之外按下指针会关闭面板。
  const isOpen = left !== null;
  React.useEffect(() => {
    if (!isOpen) return undefined;
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (anchorRef.current?.contains(target) || panelRef.current?.contains(target) || target.closest(FLYOUT_OVERLAY_SELECTOR)) return;
      close();
    };
    document.addEventListener("pointerdown", handlePointerDown, true);
    return () => document.removeEventListener("pointerdown", handlePointerDown, true);
  }, [close, isOpen]);
  // returnToAnchor closes the flyout and gives focus back to the rail button.
  // returnToAnchor 关闭浮层并把焦点交还轨道按钮。
  const returnToAnchor = () => {
    anchorRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
    close();
  };

  return (
    <>
      <div
        className="activity-rail-flyout-anchor"
        onBlur={(event) => { if (!(event.relatedTarget instanceof Element) || !event.relatedTarget.closest(".activity-rail-flyout")) scheduleClose(); }}
        onFocus={open}
        onKeyDown={(event) => { if (event.key === "Escape") { close(); event.stopPropagation(); } }}
        onMouseEnter={open}
        onMouseLeave={scheduleClose}
        ref={anchorRef}
      >
        {props.children}
      </div>
      {isOpen ? (
        <div
          aria-label={props.title}
          className="activity-rail-flyout"
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget) && !(event.relatedTarget instanceof Element && event.relatedTarget.closest(FLYOUT_OVERLAY_SELECTOR))) scheduleClose();
          }}
          onFocus={clearCloseTimer}
          onKeyDown={(event) => {
            if (event.key === "Escape" && !event.defaultPrevented) { event.stopPropagation(); returnToAnchor(); }
          }}
          onMouseEnter={open}
          onMouseLeave={scheduleClose}
          ref={panelRef}
          role="region"
          style={{ left }}
        >
          <div className="activity-rail-flyout-header">
            <span className="activity-rail-flyout-title">{props.title}</span>
          </div>
          <div className="activity-rail-flyout-body">
            {props.content}
          </div>
          {props.footer ? (
            <div className="activity-rail-flyout-footer">
              <button
                className="activity-rail-flyout-item"
                onClick={() => { close(); props.footer?.onSelect(); }}
                type="button"
              >
                <span className="activity-rail-flyout-icon">{props.footer.icon}</span>
                <span className="activity-rail-flyout-label">{props.footer.label}</span>
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
