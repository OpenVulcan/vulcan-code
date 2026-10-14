import { renderServiceErrorMessage } from "@/service/browserTransport"
import type { VmmNativeVectorStatusSummary } from "@/types"

// nativeVectorRouteCause returns why the memory vector route is unusable, in the active language when
// the service sent a catalog cause and as the English diagnostic otherwise; null when there is none.
// nativeVectorRouteCause 返回记忆向量路由不可用的原因：服务给出目录原因时使用当前语言，否则使用英文
// 诊断；没有原因时返回 null。
export function nativeVectorRouteCause(status: VmmNativeVectorStatusSummary): string | null {
  if (status.errorMessage) return renderServiceErrorMessage(status.error ?? "", status.errorMessage)
  return status.error || null
}
