import type {
  PublicAiCustomProviderSaveRequest,
  PublicAiSettingsSnapshot,
} from "@/types"

// CustomProviderSnapshotPersister is the typed RPC boundary for one complete provider command.
// CustomProviderSnapshotPersister 是一条完整供应商命令的类型化 RPC 边界。
export type CustomProviderSnapshotPersister = (
  request: PublicAiCustomProviderSaveRequest,
) => Promise<PublicAiSettingsSnapshot>

// CustomProviderSnapshotApplier publishes one successfully persisted settings snapshot to UI state.
// CustomProviderSnapshotApplier 把一份已成功持久化的设置快照发布到 UI 状态。
export type CustomProviderSnapshotApplier = (snapshot: PublicAiSettingsSnapshot) => void

// persistAndApplyCustomProviderSnapshot applies UI state only after the complete RPC transaction succeeds.
// persistAndApplyCustomProviderSnapshot 仅在完整 RPC 事务成功后应用 UI 状态。
export async function persistAndApplyCustomProviderSnapshot(
  request: PublicAiCustomProviderSaveRequest,
  persist: CustomProviderSnapshotPersister,
  apply: CustomProviderSnapshotApplier,
): Promise<PublicAiSettingsSnapshot> {
  // snapshot is returned only after the backend has durably committed its full transaction.
  // snapshot 仅在后端持久提交完整事务后返回。
  const snapshot = await persist(request)
  apply(snapshot)
  return snapshot
}
