import * as React from "react";

// AdaptiveToolPanelSide describes which edge owns the adaptive panel.
// AdaptiveToolPanelSide 描述自适应面板归属的边缘。
export type AdaptiveToolPanelSide = "left" | "right";

// AdaptiveToolPanelProps describes a reusable dock-or-float panel shell.
// AdaptiveToolPanelProps 描述可复用的停靠或浮动面板外壳。
export interface AdaptiveToolPanelProps {
  // Preferred side used by the expanded and collapsed panel affordance.
  // 展开与折叠面板控件优先使用的侧边。
  side: AdaptiveToolPanelSide;
  // Whether the panel is expanded.
  // 面板是否展开。
  expanded: boolean;
  // Whether external layout state forces the panel to overlay instead of reserving content width.
  // 外部布局状态是否强制面板浮动，而不是为内容预留宽度。
  forceOverlay?: boolean;
  // Fixed panel width used for inline reservation math.
  // 用于内联占位计算的固定面板宽度。
  panelWidth?: number;
  // Minimum content width required before the panel can reserve space.
  // 面板可占位前所需的最小内容宽度。
  minContentWidth?: number;
  // Extra width guard used to avoid near-edge overlap.
  // 用于避免贴边重叠的额外宽度保护。
  reserveGap?: number;
  // Additional padding inserted between an inline panel and content.
  // 内联面板与内容之间插入的额外间距。
  inlineGap?: number;
  // Panel content rendered when expanded or collapsed.
  // 展开或折叠时渲染的面板内容。
  panel: React.ReactNode;
  // Called immediately before the panel switches reserve or overlay mode.
  // 面板切换占位或浮动模式前立即调用。
  onLayoutModeWillChange?: () => void;
  // Main content that may receive reserved inline space.
  // 可能接收内联预留空间的主内容。
  children: React.ReactNode;
}

// AdaptiveToolPanel renders content beside a panel when space allows and overlays it otherwise.
// AdaptiveToolPanel 在空间足够时让面板与内容并列，否则让面板浮动覆盖。
export function AdaptiveToolPanel(props: AdaptiveToolPanelProps) {
  // rootRef measures the available width for reserve-mode decisions.
  // rootRef 测量用于决定占位模式的可用宽度。
  const rootRef = React.useRef<HTMLElement | null>(null);
  // panelMeasureRef measures the current expanded or collapsed panel affordance.
  // panelMeasureRef 测量当前展开或折叠面板控件的实际尺寸。
  const panelMeasureRef = React.useRef<HTMLDivElement | null>(null);
  // reservesContent stores whether the expanded panel can occupy layout space.
  // reservesContent 保存展开面板是否可以占据布局空间。
  const [reservesContent, setReservesContent] = React.useState(false);
  // panelSize stores the measured slot size so panel content can animate between states.
  // panelSize 保存已测量的插槽尺寸，使面板内容可在状态之间动画切换。
  const [panelSize, setPanelSize] = React.useState<{ height: number; width: number } | null>(null);
  // panelSizeRef rejects repeated observer deliveries before they can schedule another React update.
  // panelSizeRef 在重复的观察器投递安排下一次 React 更新之前将其拒绝。
  const panelSizeRef = React.useRef<{ height: number; width: number } | null>(null);
  // reservesContentRef exposes the newest reserve mode inside ResizeObserver callbacks.
  // reservesContentRef 在 ResizeObserver 回调中暴露最新占位模式。
  const reservesContentRef = React.useRef(reservesContent);
  // panelWidth stores the fixed panel width used by CSS variables.
  // panelWidth 保存 CSS 变量使用的固定面板宽度。
  const panelWidth = props.panelWidth ?? 176;
  // minContentWidth stores the minimum centered content width.
  // minContentWidth 保存居中内容所需的最小宽度。
  const minContentWidth = props.minContentWidth ?? 800;
  // reserveGap stores the extra width guard around the panel and content.
  // reserveGap 保存面板与内容周围的额外宽度保护。
  const reserveGap = props.reserveGap ?? 48;
  // inlineGap stores the space between the reserved panel and content padding.
  // inlineGap 保存预留面板与内容 padding 之间的间距。
  const inlineGap = props.inlineGap ?? 24;
  // onLayoutModeWillChange stores the optional layout-preservation callback.
  // onLayoutModeWillChange 保存可选的布局保持回调。
  const onLayoutModeWillChange = props.onLayoutModeWillChange;
  // inlineOffset stores half of the reserved inline width for centered child overlays.
  // inlineOffset 保存预留内联宽度的一半，供子级居中浮层使用。
  const inlineOffset = (panelWidth + inlineGap) / 2;
  // panelReservesContent is the single rendered mode after external overlay constraints are applied.
  // panelReservesContent 是应用外部浮动约束后的唯一渲染模式。
  const panelReservesContent = props.expanded && reservesContent && !props.forceOverlay;

  React.useEffect(() => {
    const element = rootRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    // updateReserveMode keeps the content centered when reservation would fit.
    // updateReserveMode 在占位可容纳时保持内容居中。
    const updateReserveMode = () => {
      const width = element.getBoundingClientRect().width;
      const nextReservesContent = width >= panelWidth + minContentWidth + reserveGap;
      if (reservesContentRef.current !== nextReservesContent) {
        onLayoutModeWillChange?.();
        reservesContentRef.current = nextReservesContent;
        setReservesContent(nextReservesContent);
      }
    };
    updateReserveMode();
    const observer = new ResizeObserver(updateReserveMode);
    observer.observe(element);
    return () => observer.disconnect();
  }, [minContentWidth, onLayoutModeWillChange, panelWidth, reserveGap]);

  React.useEffect(() => {
    const element = panelMeasureRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    // updatePanelSize keeps the outer panel slot animating toward the newest content size.
    // updatePanelSize 让外层面板插槽持续动画到最新内容尺寸。
    const updatePanelSize = () => {
      const rect = element.getBoundingClientRect();
      const nextHeight = Math.ceil(rect.height);
      const nextWidth = Math.ceil(rect.width);
      if (panelSizeRef.current?.height === nextHeight && panelSizeRef.current.width === nextWidth) {
        return;
      }
      const nextPanelSize = { height: nextHeight, width: nextWidth };
      panelSizeRef.current = nextPanelSize;
      setPanelSize(nextPanelSize);
    };
    updatePanelSize();
    const observer = new ResizeObserver(updatePanelSize);
    observer.observe(element);
    return () => observer.disconnect();
  }, [props.panel]);

  return (
    <section
      className="vk-tpanel"
      data-expanded={props.expanded ? "" : undefined}
      data-forced={props.forceOverlay ? "" : undefined}
      data-mode={panelReservesContent ? "inline" : "overlay"}
      data-side={props.side}
      ref={rootRef}
      style={{
        "--vk-tpanel-offset": `${inlineOffset}px`,
        "--vk-tpanel-w": `${panelWidth}px`,
      } as React.CSSProperties}
    >
      <div
        className="vk-tpanel-slot"
        style={panelSize ? {
          height: `${panelSize.height}px`,
          width: `${panelSize.width}px`,
        } : undefined}
      >
        <div className="vk-tpanel-inner" ref={panelMeasureRef}>
          {props.panel}
        </div>
      </div>
      <div className="vk-tpanel-content">
        {props.children}
      </div>
    </section>
  );
}
