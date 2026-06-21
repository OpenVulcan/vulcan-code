import * as React from "react";

import { cn } from "@/lib/utils";

// CONVERSATION_COLLAPSIBLE_MOTION_MS matches the CSS transition duration used by transcript fold boxes.
// CONVERSATION_COLLAPSIBLE_MOTION_MS 对齐会话折叠框使用的 CSS 过渡时长。
const CONVERSATION_COLLAPSIBLE_MOTION_MS = 180;

// ConversationCollapsibleMotionProps describes one animated transcript expansion container.
// ConversationCollapsibleMotionProps 描述单个带动画的会话展开容器。
export interface ConversationCollapsibleMotionProps {
  // Whether the content should be visible after the current transition.
  // 当前过渡结束后内容是否应该可见。
  expanded: boolean;
  // Optional class name applied to the animated outer shell.
  // 应用于动画外层容器的可选类名。
  className?: string;
  // Folded transcript content rendered only while open or closing.
  // 仅在打开或收起动画期间渲染的折叠内容。
  children: React.ReactNode;
}

// ConversationCollapsibleMotion animates height changes without repeatedly rewriting the viewport anchor.
// ConversationCollapsibleMotion 为高度变化添加动画，同时避免反复改写视口锚点。
export function ConversationCollapsibleMotion(props: ConversationCollapsibleMotionProps) {
  const prefersReducedMotion = usePrefersReducedMotion();
  const [rendered, setRendered] = React.useState(props.expanded);
  const [visualOpen, setVisualOpen] = React.useState(props.expanded);
  const [height, setHeight] = React.useState(props.expanded ? "auto" : "0px");
  const innerRef = React.useRef<HTMLDivElement | null>(null);
  const expandedRef = React.useRef(props.expanded);
  const heightRef = React.useRef(height);
  const previousExpandedRef = React.useRef(props.expanded);
  const frameRef = React.useRef<number | null>(null);
  const fallbackTimerRef = React.useRef<number | null>(null);

  React.useEffect(() => {
    expandedRef.current = props.expanded;
  }, [props.expanded]);

  React.useEffect(() => {
    heightRef.current = height;
  }, [height]);

  React.useEffect(() => {
    if (props.expanded) {
      setRendered(true);
    }
  }, [props.expanded]);

  const clearScheduledWork = React.useCallback(() => {
    if (frameRef.current !== null) {
      window.cancelAnimationFrame(frameRef.current);
      frameRef.current = null;
    }
    if (fallbackTimerRef.current !== null) {
      window.clearTimeout(fallbackTimerRef.current);
      fallbackTimerRef.current = null;
    }
  }, []);

  const finishTransition = React.useCallback(() => {
    fallbackTimerRef.current = null;
    if (expandedRef.current) {
      setVisualOpen(true);
      setHeight("auto");
      return;
    }
    setVisualOpen(false);
    setHeight("0px");
    setRendered(false);
  }, []);

  const scheduleTransitionFinish = React.useCallback(() => {
    if (fallbackTimerRef.current !== null) {
      window.clearTimeout(fallbackTimerRef.current);
    }
    fallbackTimerRef.current = window.setTimeout(finishTransition, CONVERSATION_COLLAPSIBLE_MOTION_MS + 80);
  }, [finishTransition]);

  React.useLayoutEffect(() => {
    const previousExpanded = previousExpandedRef.current;
    if (previousExpanded === props.expanded && rendered === props.expanded) {
      return;
    }
    previousExpandedRef.current = props.expanded;
    clearScheduledWork();

    if (prefersReducedMotion) {
      setRendered(props.expanded);
      setVisualOpen(props.expanded);
      setHeight(props.expanded ? "auto" : "0px");
      return;
    }

    if (props.expanded) {
      setRendered(true);
      setVisualOpen(false);
      setHeight("0px");
      frameRef.current = window.requestAnimationFrame(() => {
        frameRef.current = null;
        const nextHeight = innerRef.current?.scrollHeight ?? 0;
        setVisualOpen(true);
        setHeight(`${nextHeight}px`);
        scheduleTransitionFinish();
      });
      return;
    }

    if (!rendered) {
      return;
    }

    const currentHeight = innerRef.current?.scrollHeight ?? 0;
    setVisualOpen(true);
    setHeight(`${currentHeight}px`);
    frameRef.current = window.requestAnimationFrame(() => {
      frameRef.current = null;
      setVisualOpen(false);
      setHeight("0px");
      scheduleTransitionFinish();
    });
  }, [clearScheduledWork, prefersReducedMotion, props.expanded, rendered, scheduleTransitionFinish]);

  React.useLayoutEffect(() => {
    if (!rendered || typeof ResizeObserver === "undefined") {
      return;
    }
    const element = innerRef.current;
    if (!element) {
      return;
    }
    const observer = new ResizeObserver(() => {
      if (expandedRef.current && heightRef.current !== "auto") {
        setHeight(`${element.scrollHeight}px`);
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [rendered]);

  React.useEffect(() => clearScheduledWork, [clearScheduledWork]);

  if (!rendered) {
    return null;
  }

  return (
    <div
      aria-hidden={!props.expanded}
      className={cn("conversation-collapsible-motion", visualOpen && "open", props.className)}
      onTransitionEnd={(event) => {
        if (event.currentTarget !== event.target || event.propertyName !== "height") {
          return;
        }
        if (fallbackTimerRef.current !== null) {
          window.clearTimeout(fallbackTimerRef.current);
          fallbackTimerRef.current = null;
        }
        finishTransition();
      }}
      style={{ height }}
    >
      <div className="conversation-collapsible-motion-inner" ref={innerRef}>
        {props.children}
      </div>
    </div>
  );
}

// usePrefersReducedMotion mirrors the browser motion preference for transcript animations.
// usePrefersReducedMotion 同步浏览器的动效偏好用于会话动画。
function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = React.useState(false);
  React.useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduced(query.matches);
    const handleChange = () => setReduced(query.matches);
    query.addEventListener("change", handleChange);
    return () => query.removeEventListener("change", handleChange);
  }, []);
  return reduced;
}
