// STREAMING_THINKING_BOTTOM_THRESHOLD_PX defines the popup distance treated as tail-pinned.
// STREAMING_THINKING_BOTTOM_THRESHOLD_PX 定义思考弹窗视为锁定尾部的距离。
export const STREAMING_THINKING_BOTTOM_THRESHOLD_PX = 24;

// nextStreamingThinkingTailFollow resolves lock state without mistaking layout scroll for user intent.
// nextStreamingThinkingTailFollow 解析锁底状态，避免把布局滚动误认为用户意图。
//
// Parameters: following is the previous lock, distanceFromBottom is current geometry, and manualInteraction identifies explicit user scrolling.
// 参数：following 是先前锁定状态，distanceFromBottom 是当前几何距离，manualInteraction 标识明确用户滚动。
// Returns: the next tail-follow state; reaching bottom always restores the lock.
// 返回值：下一锁底状态；真正到达底部时始终恢复锁定。
export function nextStreamingThinkingTailFollow(
  following: boolean,
  distanceFromBottom: number,
  manualInteraction: boolean,
): boolean {
  if (distanceFromBottom <= STREAMING_THINKING_BOTTOM_THRESHOLD_PX) {
    return true;
  }
  return manualInteraction ? false : following;
}
