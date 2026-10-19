import * as React from "react"

import { errorMessage } from "@/lib/display-format"
import { loadPublicAiSettings } from "@/service/settings-actions-service"
import type { PublicAiSettingsSnapshot } from "@/types"

// SnapshotMutations serializes every settings write of the model services page.
// SnapshotMutations 串行化模型服务页的全部设置写入。
export interface SnapshotMutations {
  // pendingKey names the running write, or null when idle.
  // pendingKey 为正在执行的写入名称，空闲时为 null。
  pendingKey: string | null
  // error is the latest failure message shown by the page.
  // error 是页面显示的最近一次失败消息。
  error: string | null
  // clearError dismisses the shown failure.
  // clearError 清除已显示的失败。
  clearError: () => void
  // run executes one write unless another is running; a returned snapshot replaces the page state.
  // When reconcile is set, a failed write reloads the authoritative snapshot because a multi-step write
  // may have saved part of its change. Returns whether the write ran and succeeded.
  // run 在没有其他写入时执行一次写入；返回的快照替换页面状态。设置 reconcile 时，失败的写入会
  // 重新读取权威快照，因为多步写入可能已保存部分变更。返回写入是否执行且成功。
  run: (
    key: string,
    mutation: () => Promise<PublicAiSettingsSnapshot | void>,
    options?: { reconcile?: boolean },
  ) => Promise<boolean>
}

// useSnapshotMutations owns the single write lane so late responses never regress newer state.
// useSnapshotMutations 持有唯一写入通道，使迟到的响应不会让较新状态倒退。
// Parameters: onSnapshot receives every snapshot a write returns. Returns the shared mutation handle.
// 参数：onSnapshot 接收每次写入返回的快照。返回共享写入句柄。
export function useSnapshotMutations(onSnapshot: (next: PublicAiSettingsSnapshot) => void): SnapshotMutations {
  const [pendingKey, setPendingKey] = React.useState<string | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const runningRef = React.useRef(false)
  const onSnapshotRef = React.useRef(onSnapshot)
  onSnapshotRef.current = onSnapshot

  const run = React.useCallback<SnapshotMutations["run"]>(async (key, mutation, options) => {
    if (runningRef.current) return false
    runningRef.current = true
    setPendingKey(key)
    setError(null)
    try {
      const next = await mutation()
      if (next) onSnapshotRef.current(next)
      return true
    } catch (reason) {
      setError(errorMessage(reason))
      if (options?.reconcile) {
        try {
          onSnapshotRef.current(await loadPublicAiSettings())
        } catch {
          // The original write error stays visible when reconciliation cannot load.
          // 对账读取失败时保留原写入错误。
        }
      }
      return false
    } finally {
      runningRef.current = false
      setPendingKey(null)
    }
  }, [])
  const clearError = React.useCallback(() => setError(null), [])
  return { pendingKey, error, clearError, run }
}
