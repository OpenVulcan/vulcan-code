import { useEffect, useState } from "react";
import type { SessionGoalSummary } from "@/types";

// goalElapsedMilliseconds projects the authoritative checkpoint at nowMs; non-active states never interpolate.
// goalElapsedMilliseconds 在 nowMs 投影权威检查点，返回累计毫秒；非活动状态绝不插值走时。
export function goalElapsedMilliseconds(goal: SessionGoalSummary, nowMs: number): number {
  // workTime is optional by protocol for local drafts and pre-clock snapshots.
  // 协议允许本地草稿与旧版本快照缺少 workTime。
  const workTime = goal.workTime;
  if (workTime == null) return Math.max(0, goal.timeUsedSeconds * 1_000);
  return workTime.elapsedMs + (goal.status === "active" ? Math.max(0, nowMs - workTime.observedAtMs) : 0);
}

// formatGoalWorkDuration returns an unbounded-hour h:mm:ss clock for nonnegative elapsed milliseconds.
// formatGoalWorkDuration 将非负累计毫秒返回为小时不封顶的 h:mm:ss 时钟。
export function formatGoalWorkDuration(elapsedMs: number): string {
  // seconds uses one floor over the total so repeated checkpoints never discard fractional intervals.
  // seconds 只对总量取整，避免反复检查点丢失零碎区间。
  const seconds = Math.floor(Math.max(0, elapsedMs) / 1_000);
  // hours is deliberately not reduced modulo one day.
  // hours 有意不按一天取模。
  const hours = Math.floor(seconds / 3_600);
  // minutes is the minute component within the current hour.
  // minutes 是当前小时内的分钟分量。
  const minutes = Math.floor(seconds / 60) % 60;
  return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

// useGoalWorkDuration returns a display-only clock; the effect performs no network or persistence writes.
// useGoalWorkDuration 为 goal 返回纯展示时钟；副作用不进行任何网络或持久化写入。
export function useGoalWorkDuration(goal: SessionGoalSummary): string {
  // nowMs exists only to redraw the label; elapsed time always derives from the backend checkpoint.
  // nowMs 仅用于重绘标签；累计时间始终从后端检查点派生。
  const [nowMs, setNowMs] = useState(Date.now);
  // ticking excludes drafts and every stopped state, including blocked and exhausted goals.
  // ticking 排除草稿及所有停止状态，包括受阻和预算耗尽的目标。
  const ticking = goal.status === "active" && goal.workTime != null;
  useEffect(() => {
    if (!ticking) return;
    // timer only redraws the client label and is released on pause or unmount.
    // timer 只重绘客户端标签，在暂停或卸载时释放。
    const timer = window.setInterval(() => setNowMs(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [ticking]);
  return formatGoalWorkDuration(goalElapsedMilliseconds(goal, nowMs));
}
