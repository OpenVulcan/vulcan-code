import React from "react";

// FloatingLayerPosition stores a workspace-relative surface origin.
// FloatingLayerPosition 保存工作区相对的浮动表面原点。
export interface FloatingLayerLayout {
  // Horizontal offset from the workspace left edge.
  // 相对工作区左边缘的水平偏移。
  x: number;
  // Vertical offset from the workspace top edge.
  // 相对工作区上边缘的垂直偏移。
  y: number;
  // User-selected surface width in CSS pixels.
  // 用户选择的表面 CSS 像素宽度。
  width: number;
  // User-selected surface height in CSS pixels.
  // 用户选择的表面 CSS 像素高度。
  height: number;
}

// FloatingLayerBoundarySize stores the title-bar-safe positioning area available to the surface.
// FloatingLayerBoundarySize 保存浮动表面可用且避开标题栏的定位区域。
interface FloatingLayerBoundarySize {
  // Available boundary width in CSS pixels.
  // 可用边界宽度，单位为 CSS 像素。
  width: number;
  // Available boundary height in CSS pixels.
  // 可用边界高度，单位为 CSS 像素。
  height: number;
}

// ProjectFloatingWindowState stores normal geometry and maximize restoration for one project.
// ProjectFloatingWindowState 保存一个项目的普通几何与最大化还原状态。
export interface ProjectFloatingWindowState {
  // Current surface layout, including the full workspace rectangle while maximized.
  // 当前表面布局；最大化时包含完整工作区矩形。
  layout: FloatingLayerLayout | null;
  // Last non-maximized layout restored by the title-bar command.
  // 标题栏命令用于还原的最近一次非最大化布局。
  restoreLayout: FloatingLayerLayout | null;
  // Whether the surface currently occupies the complete workspace boundary.
  // 表面当前是否占满完整工作区边界。
  maximized: boolean;
}

// ProjectFloatingLayerControls exposes window commands to business-owned title-bar content.
// ProjectFloatingLayerControls 向业务持有的标题栏内容暴露窗口命令。
export interface ProjectFloatingLayerControls {
  // Whether the project surface is currently maximized.
  // 项目表面当前是否最大化。
  maximized: boolean;
  // Toggles between the complete workspace and the last normal layout.
  // 在完整工作区与最近普通布局之间切换。
  toggleMaximized: () => void;
}

// ProjectFloatingLayerControlsContext gives the window commands to the surface body as well as its title bar.
// ProjectFloatingLayerControlsContext 将窗口命令同时提供给表面主体及其标题栏。
const ProjectFloatingLayerControlsContext = React.createContext<ProjectFloatingLayerControls | null>(null);

// useProjectFloatingLayerControls returns the commands of the floating window around the caller, or null outside one.
// useProjectFloatingLayerControls 返回调用方所在浮动窗口的命令；不在浮动窗口内时返回 null。
export function useProjectFloatingLayerControls(): ProjectFloatingLayerControls | null {
  return React.useContext(ProjectFloatingLayerControlsContext);
}

// ProjectFloatingLayerProps defines a project-scoped movable container without terminal business logic.
// ProjectFloatingLayerProps 定义不包含终端业务逻辑的项目级可移动容器。
export interface ProjectFloatingLayerProps {
  // Stable project identity used for layout persistence.
  // 用于布局持久化的稳定项目标识。
  projectId: string;
  // Whether the full floating surface is visible.
  // 完整浮动表面是否可见。
  expanded: boolean;
  // Live top-bar button used as the macOS-style Genie animation target.
  // 用作 macOS 风格精灵动画目标的实时顶部栏按钮。
  minimizeAnchorRef: React.RefObject<HTMLButtonElement | null>;
  // Draggable surface header including business controls and optional window commands.
  // 包含业务控制项及可选窗口命令的可拖动表面标题栏。
  header: React.ReactNode | ((controls: ProjectFloatingLayerControls) => React.ReactNode);
  // Floating surface body.
  // 浮动表面主体。
  children: React.ReactNode;
  // Preferred first-open size; omitted surfaces use the desktop terminal default.
  // 首次打开的期望尺寸；省略时使用桌面终端默认尺寸。
  defaultSize?: FloatingLayerPreferredSize;
  // Any change brings the window to the front, as a click on it does.
  // 任何变化都会将窗口置于最前，与点击窗口相同。
  raiseKey?: number;
}

// FloatingLayerPreferredSize caps the first-open surface size in CSS pixels.
// FloatingLayerPreferredSize 以 CSS 像素限定首次打开的表面尺寸。
export interface FloatingLayerPreferredSize {
  // Maximum first-open width.
  // 首次打开的最大宽度。
  width: number;
  // Maximum first-open height.
  // 首次打开的最大高度。
  height: number;
  // Extra first-open distance from the lower-right corner, so windows opened one after another cascade.
  // 首次打开时距右下角的额外距离，使先后打开的窗口层叠错开。
  inset?: number;
}

// DEFAULT_FLOATING_LAYER_SIZE is the first-open size of a window that names none.
// DEFAULT_FLOATING_LAYER_SIZE 是未指定尺寸的窗口首次打开时的尺寸。
export const DEFAULT_FLOATING_LAYER_SIZE: FloatingLayerPreferredSize = { width: 760, height: 620 };

// FloatingLayerGenieDirection identifies whether the surface is entering or leaving the top-bar anchor.
// FloatingLayerGenieDirection 标识表面正在进入还是离开顶部栏锚点。
export type FloatingLayerGenieDirection = "minimize" | "restore";

// FloatingLayerAnimationState exposes the visual phase used to isolate pointer input during deformation.
// FloatingLayerAnimationState 暴露形变期间用于隔离指针输入的视觉阶段。
type FloatingLayerAnimationState = "closed" | "closing" | "opening" | "open";

// settledFloatingLayerState returns the stable endpoint for a canceled animation; the parameter is the semantic direction at cancellation and the return value is open or closed.
// settledFloatingLayerState 返回动画取消后的稳定端点；参数为取消时的语义方向，返回打开或关闭状态。
export function settledFloatingLayerState(
  direction: FloatingLayerGenieDirection,
): Extract<FloatingLayerAnimationState, "open" | "closed"> {
  return direction === "restore" ? "open" : "closed";
}

// floatingVisibilityTransitionDirection returns an animation only for a real visibility change.
// floatingVisibilityTransitionDirection 仅在真实可见性变化时返回动画方向。
export function floatingVisibilityTransitionDirection(
  previousExpanded: boolean,
  nextExpanded: boolean,
): FloatingLayerGenieDirection | null {
  if (previousExpanded === nextExpanded) return null;
  return nextExpanded ? "restore" : "minimize";
}

// FloatingLayerRect stores the viewport geometry required by deterministic Genie keyframe generation.
// FloatingLayerRect 保存确定性生成精灵关键帧所需的视口几何。
interface FloatingLayerRect {
  // Viewport-relative left edge in CSS pixels.
  // 视口相对左边缘的 CSS 像素位置。
  left: number;
  // Viewport-relative top edge in CSS pixels.
  // 视口相对上边缘的 CSS 像素位置。
  top: number;
  // Rectangle width in CSS pixels.
  // 矩形宽度，单位为 CSS 像素。
  width: number;
  // Rectangle height in CSS pixels.
  // 矩形高度，单位为 CSS 像素。
  height: number;
}

// createProjectFloatingGenieKeyframes deforms one surface toward an exact button rectangle; parameters are the surface, anchor, and direction, and the return value is an ordered Web Animations keyframe list.
// createProjectFloatingGenieKeyframes 将一个表面形变到精确按钮矩形；参数为表面、锚点和方向，返回有序的 Web Animations 关键帧列表。
export function createProjectFloatingGenieKeyframes(
  surface: FloatingLayerRect,
  anchor: FloatingLayerRect,
  direction: FloatingLayerGenieDirection,
): Keyframe[] {
  // surfaceWidth prevents invalid ratios when layout is still settling.
  // surfaceWidth 防止布局仍在稳定时产生无效比例。
  const surfaceWidth = Math.max(1, surface.width);
  // surfaceHeight prevents invalid ratios when layout is still settling.
  // surfaceHeight 防止布局仍在稳定时产生无效比例。
  const surfaceHeight = Math.max(1, surface.height);
  // anchorCenterX is the horizontal suction destination.
  // anchorCenterX 是水平吸入目标点。
  const anchorCenterX = anchor.left + (anchor.width / 2);
  // anchorCenterY is the vertical suction destination.
  // anchorCenterY 是垂直吸入目标点。
  const anchorCenterY = anchor.top + (anchor.height / 2);
  // anchorAbove selects the edge that narrows first for a top-mounted toolbar button.
  // anchorAbove 选择顶部工具栏按钮对应的优先收窄边缘。
  const anchorAbove = anchorCenterY <= surface.top + (surfaceHeight / 2);
  // originXPercent aligns the deformation funnel with the real button center.
  // originXPercent 将形变漏斗与真实按钮中心对齐。
  const originXPercent = boundedGeniePercentage(((anchorCenterX - surface.left) / surfaceWidth) * 100);
  // originYPercent chooses the top or bottom edge nearest to the button.
  // originYPercent 选择最靠近按钮的顶部或底部边缘。
  const originYPercent = anchorAbove ? 0 : 100;
  // originX converts the percentage transform origin back to viewport coordinates.
  // originX 将百分比变换原点转换回视口坐标。
  const originX = surface.left + ((originXPercent / 100) * surfaceWidth);
  // originY converts the selected edge to a viewport coordinate.
  // originY 将选定边缘转换为视口坐标。
  const originY = surface.top + ((originYPercent / 100) * surfaceHeight);
  // translateX moves the funnel origin to the button center.
  // translateX 将漏斗原点移动到按钮中心。
  const translateX = anchorCenterX - originX;
  // translateY moves the funnel origin to the button center.
  // translateY 将漏斗原点移动到按钮中心。
  const translateY = anchorCenterY - originY;
  // targetScaleX matches the final visual width to the button while retaining a visible suction point.
  // targetScaleX 将最终视觉宽度匹配到按钮，同时保留可见吸入点。
  const targetScaleX = Math.min(0.18, Math.max(0.035, anchor.width / surfaceWidth));
  // targetScaleY matches the final visual height to the button while retaining a visible suction point.
  // targetScaleY 将最终视觉高度匹配到按钮，同时保留可见吸入点。
  const targetScaleY = Math.min(0.18, Math.max(0.035, anchor.height / surfaceHeight));
  // transformOrigin keeps every stage attached to the same funnel edge.
  // transformOrigin 使每个阶段都附着在同一个漏斗边缘。
  const transformOrigin = `${originXPercent}% ${originYPercent}%`;
  // fullFrame is the undeformed terminal window endpoint.
  // fullFrame 是未形变终端窗口端点。
  const fullFrame: Keyframe = {    clipPath: "polygon(0 0, 100% 0, 100% 24%, 100% 56%, 100% 100%, 0 100%, 0 56%, 0 24%)",
    opacity: 1,
    transform: "translate3d(0px, 0px, 0) scale3d(1, 1, 1)",
    transformOrigin,
  };
  // shoulderFrame narrows the button-facing edge while the opposite edge remains broad.
  // shoulderFrame 收窄朝向按钮的边缘，同时保持另一侧边缘较宽。
  const shoulderFrame: Keyframe = {    clipPath: genieFunnelClipPath(originXPercent, 20, 3, anchorAbove),
    opacity: 1,
    transform: `translate3d(${translateX * 0.18}px, ${translateY * 0.3}px, 0) scale3d(0.96, 0.82, 1)`,
    transformOrigin,
  };
  // funnelFrame creates the characteristic macOS-style stretched neck before final absorption.
  // funnelFrame 在最终吸入前形成 macOS 风格的拉伸细颈。
  const funnelFrame: Keyframe = {    clipPath: genieFunnelClipPath(originXPercent, 6, 22, anchorAbove),
    opacity: 0.9,
    transform: `translate3d(${translateX * 0.68}px, ${translateY * 0.78}px, 0) scale3d(0.44, 0.34, 1)`,
    transformOrigin,
  };
  // anchorFrame collapses the remaining neck into the exact toolbar button center.
  // anchorFrame 将剩余细颈折叠到精确的工具栏按钮中心。
  const anchorFrame: Keyframe = {    clipPath: genieFunnelClipPath(originXPercent, 2.5, 44, anchorAbove),
    opacity: 0.18,
    transform: `translate3d(${translateX}px, ${translateY}px, 0) scale3d(${targetScaleX}, ${targetScaleY}, 1)`,
    transformOrigin,
  };

  if (direction === "minimize") {
    return [
      { ...fullFrame, offset: 0 },
      { ...shoulderFrame, offset: 0.42 },
      { ...funnelFrame, offset: 0.76 },
      { ...anchorFrame, offset: 1 },
    ];
  }
  return [
    { ...anchorFrame, offset: 0 },
    { ...funnelFrame, offset: 0.24 },
    { ...shoulderFrame, offset: 0.58 },
    { ...fullFrame, offset: 1 },
  ];
}

// boundedGeniePercentage clamps one funnel origin away from unstable extreme corners; the parameter is a percentage and the return value is the safe percentage.
// boundedGeniePercentage 将一个漏斗原点限制在不稳定的极端角落之外；参数为百分比，返回安全百分比。
function boundedGeniePercentage(value: number): number {
  return Math.min(96, Math.max(4, value));
}

// genieFunnelClipPath builds one asymmetric funnel polygon; parameters identify the anchor position, neck width, opposite-edge inset, and direction, and the return value is a CSS polygon.
// genieFunnelClipPath 构建一个非对称漏斗多边形；参数标识锚点位置、细颈宽度、另一侧内缩和方向，返回 CSS 多边形。
function genieFunnelClipPath(
  originXPercent: number,
  neckHalfWidth: number,
  oppositeInset: number,
  anchorAbove: boolean,
): string {
  // neckLeft is the bounded left edge of the button-facing neck.
  // neckLeft 是朝向按钮细颈的受限左边缘。
  const neckLeft = boundedGeniePercentage(originXPercent - neckHalfWidth);
  // neckRight is the bounded right edge of the button-facing neck.
  // neckRight 是朝向按钮细颈的受限右边缘。
  const neckRight = boundedGeniePercentage(originXPercent + neckHalfWidth);
  // oppositeLeft retains a broad opposite edge until the final keyframe.
  // oppositeLeft 在最终关键帧前保留较宽的另一侧边缘。
  const oppositeLeft = Math.min(48, Math.max(0, oppositeInset));
  // oppositeRight mirrors the opposite edge inset.
  // oppositeRight 镜像另一侧边缘内缩。
  const oppositeRight = 100 - oppositeLeft;
  // nearLeft bends the left wall outward close to the suction neck.
  // nearLeft 在靠近吸入细颈处将左侧边缘向外弯曲。
  const nearLeft = neckLeft + ((oppositeLeft - neckLeft) * 0.28);
  // nearRight bends the right wall outward close to the suction neck.
  // nearRight 在靠近吸入细颈处将右侧边缘向外弯曲。
  const nearRight = neckRight + ((oppositeRight - neckRight) * 0.28);
  // middleLeft continues the curved expansion toward the broad edge.
  // middleLeft 延续朝向宽边缘的曲线扩张。
  const middleLeft = neckLeft + ((oppositeLeft - neckLeft) * 0.68);
  // middleRight continues the curved expansion toward the broad edge.
  // middleRight 延续朝向宽边缘的曲线扩张。
  const middleRight = neckRight + ((oppositeRight - neckRight) * 0.68);
  return anchorAbove
    ? `polygon(${neckLeft}% 0, ${neckRight}% 0, ${nearRight}% 24%, ${middleRight}% 56%, ${oppositeRight}% 100%, ${oppositeLeft}% 100%, ${middleLeft}% 56%, ${nearLeft}% 24%)`
    : `polygon(${oppositeLeft}% 0, ${oppositeRight}% 0, ${middleRight}% 44%, ${nearRight}% 76%, ${neckRight}% 100%, ${neckLeft}% 100%, ${nearLeft}% 76%, ${middleLeft}% 44%)`;
}

// floatingLayerRect copies a DOM rectangle into stable numeric geometry; the parameter is a browser rectangle and the return value excludes live DOM accessors.
// floatingLayerRect 将 DOM 矩形复制为稳定数值几何；参数为浏览器矩形，返回值不包含实时 DOM 访问器。
function floatingLayerRect(rect: Pick<DOMRectReadOnly, "left" | "top" | "width" | "height">): FloatingLayerRect {
  return {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height,
  };
}

// PROJECT_FLOATING_LAYER_LEGACY_TOP_INSET_PX is removed from pre-v3 coordinates after the layer moves below the 46px conversation title bar.
// PROJECT_FLOATING_LAYER_LEGACY_TOP_INSET_PX 在浮层移到 46px 会话标题栏下方后，从 v3 之前的坐标中扣除。
const PROJECT_FLOATING_LAYER_LEGACY_TOP_INSET_PX = 46;

// FLOATING_LAYER_MIN_WIDTH_PX and FLOATING_LAYER_MIN_HEIGHT_PX are the smallest size of a regular floating surface.
// FLOATING_LAYER_MIN_WIDTH_PX 与 FLOATING_LAYER_MIN_HEIGHT_PX 是普通浮动表面的最小尺寸。
const FLOATING_LAYER_MIN_WIDTH_PX = 320;
const FLOATING_LAYER_MIN_HEIGHT_PX = 240;

// floatingLayerActivation lists the mounted floating windows from back to front; a window's rank is its index, and
// the stylesheet turns that rank into a z-index inside a fixed band below dialogs and menus.
// floatingLayerActivation 按从后到前列出已挂载的浮动窗口；窗口的排名即其下标，样式表再把排名换算为位于对话框与菜单
// 之下固定区间内的层级。
const floatingLayerActivation: string[] = [];

// floatingLayerListeners re-render every mounted window whenever the activation order changes.
// floatingLayerListeners 在激活顺序变化时重新渲染每个已挂载窗口。
const floatingLayerListeners = new Set<() => void>();

// projectFloatingWindowStateCache preserves geometry across React remounts in the same desktop session.
// projectFloatingWindowStateCache 在同一桌面会话的 React 重挂载之间保留几何状态。
const projectFloatingWindowStateCache = new Map<string, ProjectFloatingWindowState>();

// ProjectFloatingLayer renders one project-scoped movable surface above workspace content.
// ProjectFloatingLayer 在工作区内容之上渲染一个项目级可移动表面。
// Parameters: props supplies project identity, visibility and surface slots. Returns the movable project surface and its layout host.
// 参数：props 提供项目身份、可见状态与表面插槽。返回可移动的项目表面及其布局宿主。
export function ProjectFloatingLayer(props: ProjectFloatingLayerProps) {
  // layerRef identifies the workspace-sized positioning boundary.
  // layerRef 标识工作区大小的定位边界。
  const layerRef = React.useRef<HTMLDivElement | null>(null);
  // surfaceRef identifies the movable surface dimensions.
  // surfaceRef 标识可移动表面的尺寸。
  const surfaceRef = React.useRef<HTMLDivElement | null>(null);
  // dragCleanupRef releases window listeners if the layer unmounts during a drag.
  // dragCleanupRef 在拖动期间容器卸载时释放窗口监听器。
  const dragCleanupRef = React.useRef<(() => void) | null>(null);
  // genieAnimationRef owns the currently reversible Web Animations API timeline.
  // genieAnimationRef 持有当前可反向播放的 Web Animations API 时间线。
  const genieAnimationRef = React.useRef<Animation | null>(null);
  // genieDirectionRef records which semantic endpoint the active timeline is approaching.
  // genieDirectionRef 记录活动时间线正在接近的语义端点。
  const genieDirectionRef = React.useRef<FloatingLayerGenieDirection | null>(null);
  // lastAnchorRectRef preserves the latest real button geometry if another workspace view temporarily unmounts it.
  // lastAnchorRectRef 在其他工作区页面临时卸载按钮时保留最近一次真实按钮几何。
  const lastAnchorRectRef = React.useRef<FloatingLayerRect | null>(null);
  // previousExpandedRef distinguishes an actual user visibility transition from a React remount.
  // previousExpandedRef 区分真实用户可见性切换与 React 重挂载。
  const previousExpandedRef = React.useRef(props.expanded);
  // surfaceMounted keeps the terminal pixels alive until a minimize animation has fully reached the button.
  // surfaceMounted 在最小化动画完全到达按钮前保持终端像素挂载。
  const [surfaceMounted, setSurfaceMounted] = React.useState(props.expanded);
  // animationState controls pointer isolation and rendering hints without changing terminal ownership.
  // animationState 控制指针隔离与渲染提示，但不改变终端所有权。
  const [animationState, setAnimationState] = React.useState<FloatingLayerAnimationState>(
    props.expanded ? "open" : "closed",
  );
  // layerId names this window in the shared activation order; a new window opens in front of the others.
  // layerId 在共享激活顺序中标识当前窗口；新窗口打开时位于其他窗口之前。
  const layerId = React.useId();
  React.useLayoutEffect(() => {
    raiseFloatingLayer(layerId);
    return () => dropFloatingLayer(layerId);
  }, [layerId, props.raiseKey]);
  // stackRank keeps independently mounted windows selectable in activation order when their rectangles overlap.
  // stackRank 使独立挂载的窗口在矩形重叠时仍可按激活顺序选择。
  const stackRank = React.useSyncExternalStore(subscribeFloatingLayers, () => floatingLayerRank(layerId));
  // windowState stores project-scoped normal geometry, maximized geometry, and restore geometry.
  // windowState 保存项目级普通几何、最大化几何与还原几何。
  const [windowState, setWindowState] = React.useState<ProjectFloatingWindowState>(
    () => loadProjectFloatingWindowState(props.projectId),
  );
  // boundarySize stores the current title-bar-safe area without mutating saved project geometry.
  // boundarySize 保存当前避开标题栏的区域，但不修改项目已保存几何。
  const [boundarySize, setBoundarySize] = React.useState<FloatingLayerBoundarySize | null>(null);
  // layout projects the project-owned desired rectangle into the current workspace only for rendering.
  // layout 仅为渲染将项目持有的期望矩形投影到当前工作区。
  const layout = boundarySize && windowState.layout
    ? (windowState.maximized
      ? maximizedFloatingLayout(boundarySize.width, boundarySize.height)
      : clampFloatingLayout(windowState.layout, boundarySize.width, boundarySize.height))
    : windowState.layout;
  // expectedRenderedLayoutRef distinguishes React-applied projection from native user resizing.
  // expectedRenderedLayoutRef 区分 React 应用的显示投影与用户原生缩放。
  const expectedRenderedLayoutRef = React.useRef<FloatingLayerLayout | null>(layout);
  expectedRenderedLayoutRef.current = layout;

  // bringToFront raises only this floating window without changing terminal visibility or ownership.
  // bringToFront 仅提升当前浮动窗口，不改变终端可见性或所有权。
  const bringToFront = React.useCallback(() => raiseFloatingLayer(layerId), [layerId]);

  // Load project geometry before paint so a project switch never shows the previous project's size for one frame.
  // 在绘制前加载项目几何，避免项目切换时出现一帧上一个项目的尺寸。
  React.useLayoutEffect(() => {
    dragCleanupRef.current?.();
    setWindowState(loadProjectFloatingWindowState(props.projectId));
    setBoundarySize(null);
    previousExpandedRef.current = props.expanded;
    genieDirectionRef.current = null;
  }, [props.projectId]);
  // Initialize one deterministic project default before paint whenever a minimized surface is mounted for the first time.
  // 在最小化表面首次挂载时，于绘制前初始化一个确定性的项目默认几何。
  React.useLayoutEffect(() => {
    if (!surfaceMounted) return;
    const layer = layerRef.current;
    if (!layer || layer.clientWidth <= 0 || layer.clientHeight <= 0) return;
    // measuredBoundary is already below the conversation title bar because the layer owns that CSS inset.
    // measuredBoundary 已位于会话标题栏下方，因为该 CSS 内缩由浮层边界持有。
    const measuredBoundary: FloatingLayerBoundarySize = {
      width: layer.clientWidth,
      height: layer.clientHeight,
    };
    setBoundarySize(measuredBoundary);
    setWindowState((current) => {
      if (current.layout) return current;
      // initialLayout is project-owned immediately, so later conversation layout changes cannot redefine the default.
      // initialLayout 会立即归项目持有，因此后续会话布局变化不能重新定义默认位置。
      const initialLayout = createDefaultProjectFloatingLayout(
        measuredBoundary.width,
        measuredBoundary.height,
        props.defaultSize,
      );
      // initializedState uses the same normal rectangle for current and maximize restoration geometry.
      // initializedState 为当前几何与最大化还原几何使用同一个普通矩形。
      const initializedState: ProjectFloatingWindowState = {
        layout: initialLayout,
        restoreLayout: initialLayout,
        maximized: false,
      };
      saveProjectFloatingWindowState(props.projectId, initializedState);
      return initializedState;
    });
  }, [props.defaultSize, props.projectId, surfaceMounted]);
  React.useEffect(() => () => {
    dragCleanupRef.current?.();
    // activeAnimation is detached from callbacks before cancellation so unmount never schedules state updates.
    // activeAnimation 在取消前解除回调，避免卸载期间继续调度状态更新。
    const activeAnimation = genieAnimationRef.current;
    if (activeAnimation) {
      activeAnimation.onfinish = null;
      activeAnimation.oncancel = null;
      activeAnimation.cancel();
    }
    genieAnimationRef.current = null;
    genieDirectionRef.current = null;
  }, []);
  React.useEffect(() => {
    if (!props.expanded) {
      // Collapsing ends drag ownership even while the surface remains mounted for its closing animation.
      // 收起立即结束拖动归属，即使表面仍为关闭动画保持挂载。
      dragCleanupRef.current?.();
      return;
    }
    const layer = layerRef.current;
    const surface = surfaceRef.current;
    if (!layer || !surface || typeof ResizeObserver === "undefined") return;
    // reconcileObservedLayout updates boundary projection or persists an explicit native surface resize, never both.
    // reconcileObservedLayout 更新边界投影或持久化明确的原生表面缩放，绝不在一次通知中同时执行。
    const reconcileObservedLayout = (entries: ResizeObserverEntry[]) => {
      // boundaryChanged makes container-driven clamping display-only for this notification cycle.
      // boundaryChanged 使容器驱动的夹紧在当前通知周期内仅影响显示。
      const boundaryChanged = entries.some((entry) => entry.target === layer);
      if (boundaryChanged) {
        // nextBoundary is the exact title-bar-safe area used by drag, restore, and maximize projection.
        // nextBoundary 是拖动、还原及最大化投影使用的精确避开标题栏区域。
        const nextBoundary: FloatingLayerBoundarySize = {
          width: layer.clientWidth,
          height: layer.clientHeight,
        };
        setBoundarySize((current) => (
          current && current.width === nextBoundary.width && current.height === nextBoundary.height
            ? current
            : nextBoundary
        ));
        return;
      }
      // expectedLayout is the last programmatic projection applied by React.
      // expectedLayout 是 React 最近一次应用的程序化显示投影。
      const expectedLayout = expectedRenderedLayoutRef.current;
      if (!expectedLayout) return;
      // measuredLayout captures only the untransformed native-resize result inside the current boundary.
      // measuredLayout 仅捕获当前边界内未变换的原生缩放结果。
      const measuredLayout = clampFloatingLayout(
        measuredFloatingLayout(surface),
        layer.clientWidth,
        layer.clientHeight,
      );
      if (floatingLayoutsEqual(expectedLayout, measuredLayout)) return;
      setWindowState((current) => {
        if (current.maximized || floatingLayoutsEqual(current.layout, measuredLayout)) return current;
        // nextState persists only user-owned normal geometry and keeps maximize restoration exact.
        // nextState 仅持久化用户持有的普通几何，并保持最大化还原精确。
        const nextState: ProjectFloatingWindowState = {
          ...current,
          layout: measuredLayout,
          restoreLayout: measuredLayout,
        };
        saveProjectFloatingWindowState(props.projectId, nextState);
        return nextState;
      });
    };
    // observer uses one delivery batch so a boundary change cannot race a surface projection and overwrite storage.
    // observer 使用同一交付批次，避免边界变化与表面投影竞态并覆盖存储。
    const observer = new ResizeObserver(reconcileObservedLayout);
    observer.observe(layer);
    observer.observe(surface);
    return () => observer.disconnect();
  }, [props.expanded, props.projectId, surfaceMounted]);

  // Synchronize the mounted surface with the requested visibility and play a reversible Genie transition.
  // 将已挂载表面与请求可见性同步，并播放可反向的精灵过渡动画。
  React.useLayoutEffect(() => {
    // visibilityChanged is true only for an actual expanded-state transition, never for a remount.
    // visibilityChanged 仅在 expanded 真实切换时为真，React 重挂载不会触发。
    const transitionDirection = floatingVisibilityTransitionDirection(
      previousExpandedRef.current,
      props.expanded,
    );
    // visibilityChanged distinguishes an actual command from an identical remounted state.
    // visibilityChanged 区分真实命令与重挂载后的相同状态。
    const visibilityChanged = transitionDirection !== null;
    if (transitionDirection) {
      previousExpandedRef.current = props.expanded;
      genieDirectionRef.current = transitionDirection;
      // A restored window comes back in front of the windows opened while it was minimized.
      // 还原的窗口回到其最小化期间打开的窗口之前。
      if (transitionDirection === "restore") raiseFloatingLayer(layerId);
    }
    // requestedDirection remains pending across the extra mount required by a restore transition.
    // requestedDirection 在还原过程额外需要的一次挂载之间保持待处理状态。
    const requestedDirection = genieDirectionRef.current;
    const activeAnimation = genieAnimationRef.current;
    if (activeAnimation && requestedDirection) {
      if (visibilityChanged) {
        setAnimationState(requestedDirection === "restore" ? "opening" : "closing");
        activeAnimation.reverse();
      }
      return;
    }
    if (!requestedDirection) {
      if (props.expanded) {
        if (!surfaceMounted) setSurfaceMounted(true);
        setAnimationState("open");
      } else {
        setAnimationState("closed");
        if (surfaceMounted) setSurfaceMounted(false);
      }
      return;
    }
    if (props.expanded && !surfaceMounted) {
      setSurfaceMounted(true);
      return;
    }
    if (!surfaceMounted) {
      setAnimationState("closed");
      return;
    }

    const surface = surfaceRef.current;
    const liveAnchor = props.minimizeAnchorRef.current;
    const liveAnchorRect = liveAnchor?.isConnected ? floatingLayerRect(liveAnchor.getBoundingClientRect()) : null;
    if (liveAnchorRect) lastAnchorRectRef.current = liveAnchorRect;
    const anchorRect = liveAnchorRect ?? lastAnchorRectRef.current;
    const reducedMotion = typeof window.matchMedia === "function"
      && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!surface || !anchorRect || reducedMotion || typeof surface.animate !== "function") {
      if (surface) surface.style.visibility = props.expanded ? "" : "hidden";
      genieDirectionRef.current = null;
      setAnimationState(props.expanded ? "open" : "closed");
      if (!props.expanded) setSurfaceMounted(false);
      return;
    }

    surface.style.visibility = "";
    const animation = surface.animate(
      createProjectFloatingGenieKeyframes(
        floatingLayerRect(surface.getBoundingClientRect()),
        anchorRect,
        requestedDirection,
      ),
      {
        duration: requestedDirection === "minimize" ? 420 : 380,
        easing: requestedDirection === "minimize"
          ? "cubic-bezier(0.46, 0.04, 0.58, 0.94)"
          : "cubic-bezier(0.18, 0.76, 0.22, 1)",
        fill: "both",
      },
    );
    genieAnimationRef.current = animation;
    genieDirectionRef.current = requestedDirection;
    setAnimationState(requestedDirection === "restore" ? "opening" : "closing");
    animation.onfinish = () => {
      const completedDirection = genieDirectionRef.current;
      genieAnimationRef.current = null;
      genieDirectionRef.current = null;
      animation.oncancel = null;
      if (completedDirection === "minimize") {
        surface.style.visibility = "hidden";
        animation.cancel();
        setAnimationState("closed");
        setSurfaceMounted(false);
      } else {
        animation.cancel();
        setAnimationState("open");
      }
    };
    animation.oncancel = () => {
      if (genieAnimationRef.current !== animation) return;
      // cancelledDirection is the current semantic destination, including a reversed animation.
      // cancelledDirection 是当前语义目标，并包含动画已反向播放的情况。
      const cancelledDirection = genieDirectionRef.current;
      genieAnimationRef.current = null;
      genieDirectionRef.current = null;
      if (!cancelledDirection) return;
      // settledState prevents an interrupted conversation render from leaving pointer input disabled in opening.
      // settledState 防止对话渲染中断后仍停留在 opening 并永久禁用指针输入。
      const settledState = settledFloatingLayerState(cancelledDirection);
      surface.style.visibility = settledState === "open" ? "" : "hidden";
      setAnimationState(settledState);
      if (settledState === "closed") setSurfaceMounted(false);
    };
  }, [layerId, props.expanded, props.minimizeAnchorRef, surfaceMounted]);

  // toggleMaximized switches between the workspace rectangle and the last exact normal geometry.
  // toggleMaximized 在工作区矩形与最近一次精确普通几何之间切换。
  const toggleMaximized = React.useCallback(() => {
    const layer = layerRef.current;
    const surface = surfaceRef.current;
    if (!layer || !surface) return;
    setWindowState((current) => {
      if (current.maximized) {
        // restoredLayout retains the project-owned desired rectangle; rendering alone clamps it to this workspace.
        // restoredLayout 保留项目持有的期望矩形；仅渲染阶段将其夹紧到当前工作区。
        const restoredLayout = current.restoreLayout ?? measuredFloatingLayout(surface);
        // nextState clears maximize mode while retaining the same normal layout as the next restore source.
        // nextState 清除最大化模式，同时保留同一普通布局作为下一次还原来源。
        const nextState: ProjectFloatingWindowState = {
          layout: restoredLayout,
          restoreLayout: restoredLayout,
          maximized: false,
        };
        saveProjectFloatingWindowState(props.projectId, nextState);
        return nextState;
      }
      // measuredNormalLayout captures a native resize that may have arrived immediately before maximizing.
      // measuredNormalLayout 捕获可能恰好在最大化前发生的原生缩放。
      const measuredNormalLayout = clampFloatingLayout(
        measuredFloatingLayout(surface),
        layer.clientWidth,
        layer.clientHeight,
      );
      // normalLayout preserves an off-boundary project preference unless the live box proves a pending user resize.
      // normalLayout 保留超出当前边界的项目偏好，除非实时盒明确证明存在待保存的用户缩放。
      const normalLayout = current.layout
        && floatingLayoutsEqual(expectedRenderedLayoutRef.current, measuredNormalLayout)
        ? current.layout
        : measuredNormalLayout;
      // nextState fills the complete workspace while preserving the exact normal restoration rectangle.
      // nextState 占满完整工作区，同时保留精确的普通还原矩形。
      const nextState: ProjectFloatingWindowState = {
        layout: maximizedFloatingLayout(layer.clientWidth, layer.clientHeight),
        restoreLayout: normalLayout,
        maximized: true,
      };
      saveProjectFloatingWindowState(props.projectId, nextState);
      return nextState;
    });
  }, [props.projectId]);

  // handleHeaderPointerDown begins pointer-owned surface dragging outside interactive controls.
  // handleHeaderPointerDown 在交互控制项之外开始由指针持有的表面拖动。
  // Parameters: event is the header pointer press. Returns nothing; accepted gestures install project-owned drag listeners.
  // 参数：event 是标题指针按下事件。无返回值；接纳的手势安装项目自有拖动监听。
  const handleHeaderPointerDown = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (windowState.maximized || event.button !== 0
      || (event.target as HTMLElement).closest("button, select, input, [role='button']")) return;
    const layer = layerRef.current;
    const surface = surfaceRef.current;
    if (!layer || !surface) return;
    dragCleanupRef.current?.();
    event.preventDefault();
    const layerRect = layer.getBoundingClientRect();
    const surfaceRect = surface.getBoundingClientRect();
    const pointerOffset = {
      x: event.clientX - surfaceRect.left,
      y: event.clientY - surfaceRect.top,
    };
    const handlePointerMove = (moveEvent: PointerEvent) => {
      const maximumX = Math.max(0, layerRect.width - surfaceRect.width);
      const maximumY = Math.max(0, layerRect.height - surfaceRect.height);
      setWindowState((current) => ({
        ...current,
        layout: {
          x: Math.min(maximumX, Math.max(0, moveEvent.clientX - layerRect.left - pointerOffset.x)),
          y: Math.min(maximumY, Math.max(0, moveEvent.clientY - layerRect.top - pointerOffset.y)),
          width: current.layout?.width ?? surfaceRect.width,
          height: current.layout?.height ?? surfaceRect.height,
        },
      }));
    };
    // releaseDrag removes listeners and optionally persists the final project position.
    // releaseDrag 移除监听器，并按需持久化最终项目位置。
    // Parameters: persist is true only for completed gestures. Returns nothing and releases every listener of this drag.
    // 参数：persist 仅对完成的手势为真。无返回值，并释放本次拖动的全部监听。
    const releaseDrag = (persist: boolean) => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerCancel);
      dragCleanupRef.current = null;
      if (persist) {
        setWindowState((current) => {
          saveProjectFloatingWindowState(props.projectId, current);
          return current;
        });
      }
    };
    // handlePointerUp commits the dragged position at the end of pointer ownership.
    // handlePointerUp 在指针所有权结束时提交拖动位置。
    const handlePointerUp = () => releaseDrag(true);
    // handlePointerCancel releases a cancelled gesture without persisting an incomplete drag.
    // handlePointerCancel 释放取消的手势，不保存未完成的拖动。
    // Parameters: none. Returns nothing; the captured release function clears this gesture without persisting position.
    // 参数：无。无返回值；捕获的释放函数清理本次手势且不保存位置。
    const handlePointerCancel = () => releaseDrag(false);
    dragCleanupRef.current = () => releaseDrag(false);
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp, { once: true });
    window.addEventListener("pointercancel", handlePointerCancel, { once: true });
  }, [props.projectId, windowState.maximized]);

  // handleHeaderDoubleClick toggles maximize only from the non-interactive draggable title region.
  // handleHeaderDoubleClick 仅从非交互式可拖动标题区域切换最大化。
  const handleHeaderDoubleClick = React.useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest("button, select, input, [role='button']")) return;
    event.preventDefault();
    toggleMaximized();
  }, [toggleMaximized]);

  // headerControls is the exact window-command surface passed to terminal-owned title content.
  // headerControls 是传给终端业务标题内容的精确窗口命令界面。
  const headerControls = React.useMemo<ProjectFloatingLayerControls>(() => ({
    maximized: windowState.maximized,
    toggleMaximized,
  }), [toggleMaximized, windowState.maximized]);
  // renderedHeader resolves a static title or a command-aware title renderer.
  // renderedHeader 解析静态标题或可感知窗口命令的标题渲染器。
  const renderedHeader = typeof props.header === "function" ? props.header(headerControls) : props.header;

  return (
    <div
      ref={layerRef}
      className="vk-float-layer"
      data-project-id={props.projectId}
      data-stack-rank={stackRank}
      style={{ "--vk-float-rank": stackRank } as React.CSSProperties}
    >
      {surfaceMounted ? (
        <section
          ref={surfaceRef}
          aria-hidden={!props.expanded}
          inert={!props.expanded}
          className="vk-float-win"
          data-animation-state={animationState}
          data-maximized={windowState.maximized ? "true" : "false"}
          data-workspace-input-preserve-owner="true"
          onPointerDownCapture={bringToFront}
          style={layout ? {
            right: "auto",
            bottom: "auto",
            left: layout.x,
            top: layout.y,
            width: layout.width,
            height: layout.height,
          } : undefined}
        >
          <div
            className="vk-float-win-head"
            onDoubleClick={handleHeaderDoubleClick}
            onPointerDown={handleHeaderPointerDown}
          >
            {renderedHeader}
          </div>
          <div className="vk-float-win-body">
            <ProjectFloatingLayerControlsContext.Provider value={headerControls}>{props.children}</ProjectFloatingLayerControlsContext.Provider>
          </div>
        </section>
      ) : null}
    </div>
  );
}

// raiseFloatingLayer moves one window to the front of the activation order.
// raiseFloatingLayer 将一个窗口移到激活顺序的最前方。
export function raiseFloatingLayer(id: string): void {
  const index = floatingLayerActivation.indexOf(id);
  if (index >= 0 && index === floatingLayerActivation.length - 1) return;
  if (index >= 0) floatingLayerActivation.splice(index, 1);
  floatingLayerActivation.push(id);
  floatingLayerListeners.forEach((listener) => listener());
}

// dropFloatingLayer removes an unmounted window so the remaining ranks stay contiguous.
// dropFloatingLayer 移除已卸载的窗口，使其余排名保持连续。
export function dropFloatingLayer(id: string): void {
  const index = floatingLayerActivation.indexOf(id);
  if (index < 0) return;
  floatingLayerActivation.splice(index, 1);
  floatingLayerListeners.forEach((listener) => listener());
}

// floatingLayerRank returns how many mounted windows sit behind this one.
// floatingLayerRank 返回位于该窗口之后的已挂载窗口数量。
export function floatingLayerRank(id: string): number {
  return Math.max(0, floatingLayerActivation.indexOf(id));
}

// subscribeFloatingLayers follows activation order changes for useSyncExternalStore.
// subscribeFloatingLayers 为 useSyncExternalStore 跟随激活顺序的变化。
function subscribeFloatingLayers(listener: () => void): () => void {
  floatingLayerListeners.add(listener);
  return () => floatingLayerListeners.delete(listener);
}

// projectFloatingPositionStorageKey returns the exact project-scoped layout key.
// projectFloatingPositionStorageKey 返回精确的项目级布局键。
function projectFloatingPositionStorageKey(projectId: string): string {
  return `vulcan.project-floating-layer.${projectId}`;
}

// createDefaultProjectFloatingWindowState builds one untouched normal window state.
// createDefaultProjectFloatingWindowState 构建一个未使用的普通窗口状态。
function createDefaultProjectFloatingWindowState(): ProjectFloatingWindowState {
  return { layout: null, restoreLayout: null, maximized: false };
}

// loadProjectFloatingWindowState reads one exact project state from the session cache or durable storage.
// loadProjectFloatingWindowState 从会话缓存或持久存储读取一个精确项目状态。
function loadProjectFloatingWindowState(projectId: string): ProjectFloatingWindowState {
  const cached = projectFloatingWindowStateCache.get(projectId);
  if (cached) return cached;
  if (typeof window === "undefined") return createDefaultProjectFloatingWindowState();
  try {
    const raw = window.localStorage.getItem(projectFloatingPositionStorageKey(projectId));
    if (!raw) return createDefaultProjectFloatingWindowState();
    // parsed is validated before it can cross into the project-level geometry cache.
    // parsed 在进入项目级几何缓存前完成校验。
    const parsed = JSON.parse(raw) as unknown;
    // legacyLayout migrates the previous flat x/y/width/height storage contract exactly once.
    // legacyLayout 对上一版扁平 x/y/width/height 存储合同执行一次精确迁移。
    const legacyLayout = validatedFloatingLayout(parsed);
    if (legacyLayout) {
      // migratedLayout keeps the old visual top while changing the coordinate origin below the title bar.
      // migratedLayout 在坐标原点移到标题栏下方时保持旧的视觉顶部。
      const migratedLayout = migrateLegacyFloatingLayout(legacyLayout);
      const migratedState: ProjectFloatingWindowState = {
        layout: migratedLayout,
        restoreLayout: migratedLayout,
        maximized: false,
      };
      saveProjectFloatingWindowState(projectId, migratedState);
      return migratedState;
    }
    if (!isRecord(parsed)
      || (parsed.version !== 2 && parsed.version !== 3)
      || typeof parsed.maximized !== "boolean") {
      return createDefaultProjectFloatingWindowState();
    }
    // parsedLayout is the validated current rectangle stored by contract version two or three.
    // parsedLayout 是合同版本二或三存储的已校验当前矩形。
    const parsedLayout = parsed.layout === null ? null : validatedFloatingLayout(parsed.layout);
    // parsedRestoreLayout is the validated non-maximized rectangle stored by contract version two or three.
    // parsedRestoreLayout 是合同版本二或三存储的已校验非最大化矩形。
    const parsedRestoreLayout = parsed.restoreLayout === null
      ? null
      : validatedFloatingLayout(parsed.restoreLayout);
    if ((parsed.layout !== null && !parsedLayout)
      || (parsed.restoreLayout !== null && !parsedRestoreLayout)
      || (parsed.maximized && !parsedRestoreLayout)) {
      return createDefaultProjectFloatingWindowState();
    }
    // coordinateMigration converts only v2 full-workspace coordinates to the v3 title-bar-safe origin.
    // coordinateMigration 仅将 v2 全工作区坐标转换为 v3 避开标题栏的原点。
    const coordinateMigration = parsed.version === 2;
    const loadedState: ProjectFloatingWindowState = {
      layout: coordinateMigration && parsedLayout ? migrateLegacyFloatingLayout(parsedLayout) : parsedLayout,
      restoreLayout: coordinateMigration && parsedRestoreLayout
        ? migrateLegacyFloatingLayout(parsedRestoreLayout)
        : parsedRestoreLayout,
      maximized: parsed.maximized,
    };
    if (coordinateMigration) {
      saveProjectFloatingWindowState(projectId, loadedState);
      return loadedState;
    }
    projectFloatingWindowStateCache.set(projectId, loadedState);
    return loadedState;
  } catch {
    return createDefaultProjectFloatingWindowState();
  }
}

// saveProjectFloatingWindowState persists current, restore, and maximize state for one exact project.
// saveProjectFloatingWindowState 为一个精确项目持久化当前、还原及最大化状态。
function saveProjectFloatingWindowState(projectId: string, state: ProjectFloatingWindowState): void {
  projectFloatingWindowStateCache.set(projectId, state);
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(projectFloatingPositionStorageKey(projectId), JSON.stringify({
      version: 3,
      layout: state.layout,
      restoreLayout: state.restoreLayout,
      maximized: state.maximized,
    }));
  } catch {
    // Layout persistence is optional and must never break terminal interaction.
    // 布局持久化是可选能力，绝不能破坏终端交互。
  }
}

// isRecord narrows one decoded JSON value to an object with named fields.
// isRecord 将一个已解码 JSON 值收窄为带命名字段的对象。
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

// validatedFloatingLayout validates and normalizes one decoded layout object.
// validatedFloatingLayout 校验并规范化一个已解码布局对象。
function validatedFloatingLayout(value: unknown): FloatingLayerLayout | null {
  if (!isRecord(value)
    || typeof value.x !== "number" || !Number.isFinite(value.x)
    || typeof value.y !== "number" || !Number.isFinite(value.y)
    || typeof value.width !== "number" || !Number.isFinite(value.width)
    || typeof value.height !== "number" || !Number.isFinite(value.height)) return null;
  return {
    x: Math.max(0, Number(value.x)),
    y: Math.max(0, Number(value.y)),
    width: Math.max(FLOATING_LAYER_MIN_WIDTH_PX, Number(value.width)),
    height: Math.max(FLOATING_LAYER_MIN_HEIGHT_PX, Number(value.height)),
  };
}

// migrateLegacyFloatingLayout translates a pre-v3 full-workspace origin into the title-bar-safe coordinate system.
// migrateLegacyFloatingLayout 将 v3 之前的全工作区原点转换为避开标题栏的坐标系统。
function migrateLegacyFloatingLayout(layout: FloatingLayerLayout): FloatingLayerLayout {
  return {
    ...layout,
    y: Math.max(0, layout.y - PROJECT_FLOATING_LAYER_LEGACY_TOP_INSET_PX),
  };
}

// measuredFloatingLayout reads the untransformed CSS box used as a normal restore source.
// measuredFloatingLayout 读取用作普通还原来源的未变换 CSS 盒。
function measuredFloatingLayout(surface: HTMLDivElement): FloatingLayerLayout {
  return {
    x: surface.offsetLeft,
    y: surface.offsetTop,
    width: surface.offsetWidth,
    height: surface.offsetHeight,
  };
}

// createDefaultProjectFloatingLayout returns one deterministic lower-right normal rectangle inside the safe boundary.
// createDefaultProjectFloatingLayout 返回避开标题栏边界内一个确定性的右下普通矩形。
export function createDefaultProjectFloatingLayout(
  boundaryWidth: number,
  boundaryHeight: number,
  preferredSize: FloatingLayerPreferredSize = DEFAULT_FLOATING_LAYER_SIZE,
): FloatingLayerLayout {
  // preferredWidth matches the requested default while leaving a compact outer margin when space permits.
  // preferredWidth 匹配请求的默认宽度，并在空间允许时保留紧凑外边距。
  const preferredWidth = Math.min(preferredSize.width, Math.max(0, boundaryWidth - 40));
  // preferredHeight scales with the safe workspace and retains the existing 340px normal desktop minimum.
  // preferredHeight 随安全工作区缩放，并保留现有的 340px 普通桌面最小高度。
  const preferredHeight = Math.min(
    preferredSize.height,
    Math.max(Math.min(340, boundaryHeight), Math.round(boundaryHeight * 0.58)),
  );
  return clampFloatingLayout({
    x: Math.max(0, boundaryWidth - preferredWidth - 16 - (preferredSize.inset ?? 0)),
    y: Math.max(0, boundaryHeight - preferredHeight - 26 - (preferredSize.inset ?? 0)),
    width: preferredWidth,
    height: preferredHeight,
  }, boundaryWidth, boundaryHeight);
}

// maximizedFloatingLayout returns the exact complete workspace rectangle.
// maximizedFloatingLayout 返回精确的完整工作区矩形。
export function maximizedFloatingLayout(boundaryWidth: number, boundaryHeight: number): FloatingLayerLayout {
  return {
    x: 0,
    y: 0,
    width: Math.max(0, boundaryWidth),
    height: Math.max(0, boundaryHeight),
  };
}

// clampFloatingLayout keeps a floating surface wholly inside the current workspace boundary.
// clampFloatingLayout 使浮动表面完整保持在当前工作区边界之内。
export function clampFloatingLayout(
  layout: FloatingLayerLayout,
  boundaryWidth: number,
  boundaryHeight: number,
): FloatingLayerLayout {
  const minimumWidth = Math.min(FLOATING_LAYER_MIN_WIDTH_PX, boundaryWidth);
  const minimumHeight = Math.min(FLOATING_LAYER_MIN_HEIGHT_PX, boundaryHeight);
  const width = Math.min(Math.max(minimumWidth, layout.width), boundaryWidth);
  const height = Math.min(Math.max(minimumHeight, layout.height), boundaryHeight);
  return {
    x: Math.min(Math.max(0, boundaryWidth - width), Math.max(0, layout.x)),
    y: Math.min(Math.max(0, boundaryHeight - height), Math.max(0, layout.y)),
    width,
    height,
  };
}

// floatingLayoutsEqual suppresses ResizeObserver feedback when geometry is unchanged.
// floatingLayoutsEqual 在几何未变化时抑制 ResizeObserver 反馈。
function floatingLayoutsEqual(
  left: FloatingLayerLayout | null,
  right: FloatingLayerLayout,
): boolean {
  return left !== null
    && Math.abs(left.x - right.x) < 0.5
    && Math.abs(left.y - right.y) < 0.5
    && Math.abs(left.width - right.width) < 0.5
    && Math.abs(left.height - right.height) < 0.5;
}
