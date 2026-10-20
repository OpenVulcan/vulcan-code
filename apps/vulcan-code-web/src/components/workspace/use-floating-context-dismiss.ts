import React from "react";

// useFloatingContextDismiss closes the floating context panel the way a drawer closes: Escape while focus is in the panel
// (or nowhere in particular), or a primary press on the page area the panel covers.
// useFloatingContextDismiss 以抽屉方式关闭浮动上下文面板：焦点位于面板内（或无特定焦点）时按 Escape，或在被面板覆盖的
// 页面区域按下主键。
// Parameters: active is whether the panel currently floats open; hostRef is the panel host; coveredRef is the page area
// beneath it; onDismiss closes the panel. Returns nothing. Events another handler already consumed, and presses inside
// portaled menus, are left alone.
// 参数：active 表示面板当前是否以浮动方式打开；hostRef 为面板宿主；coveredRef 为其下方的页面区域；onDismiss 关闭面板。
// 无返回值。已被其他处理器消费的事件以及门户菜单内的按下操作不受影响。
export function useFloatingContextDismiss(options: {
  active: boolean;
  hostRef: React.RefObject<HTMLElement | null>;
  coveredRef: React.RefObject<HTMLElement | null>;
  onDismiss: () => void;
}): void {
  const { active, hostRef, coveredRef } = options;
  // onDismissRef keeps the latest close action without re-registering listeners on every render.
  // onDismissRef 保存最新的关闭动作，避免每次渲染都重新注册监听。
  const onDismissRef = React.useRef(options.onDismiss);
  onDismissRef.current = options.onDismiss;

  React.useEffect(() => {
    if (!active) return;
    // opener is the control focused when the panel opened; Escape from inside the panel hands focus back to it.
    // opener 是面板打开时获得焦点的控件；在面板内按 Escape 时焦点交还给它。
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== "Escape" || event.defaultPrevented || event.isComposing) return;
      const target = event.target;
      const inside = target instanceof Node && Boolean(hostRef.current?.contains(target));
      if (!inside && target !== document.body && target !== document.documentElement) return;
      event.preventDefault();
      onDismissRef.current();
      if (inside && opener?.isConnected) opener.focus();
    };
    const handlePointerDown = (event: PointerEvent): void => {
      if (event.button !== 0 || event.defaultPrevented) return;
      const target = event.target;
      if (target instanceof Node && coveredRef.current?.contains(target)) onDismissRef.current();
    };
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("pointerdown", handlePointerDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [active, coveredRef, hostRef]);
}
