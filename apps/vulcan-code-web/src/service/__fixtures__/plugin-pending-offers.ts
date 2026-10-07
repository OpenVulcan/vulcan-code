import type { PluginPendingActionOffer } from "@/service/plugin-pending-operations-service";
import type { PluginPrewarmPolicyReceipt } from "@/types/plugin-prewarm-policy";

// prewarmReceiptOffers stands in for the service registry in browser tests: it declares the continuations the kernel
// offers for receipt, so component tests exercise rendering of declared offers rather than a browser-side rule.
// prewarmReceiptOffers 在浏览器测试中替代服务登记表：声明内核为 receipt 提供的继续方式，
// 使组件测试验证已声明动作的渲染，而非浏览器端规则。
export function prewarmReceiptOffers(receipt: PluginPrewarmPolicyReceipt | null): PluginPendingActionOffer[] {
  if (receipt === null) return [];
  const terminal = receipt.phase === "committed" || receipt.phase === "cancelled" || receipt.phase === "interrupted";
  if (terminal && receipt.persistenceError === undefined) return [{ action: "forget", requiresAcknowledgement: false }];
  return [...receipt.phase === "prepared" ? [{ action: "cancel", requiresAcknowledgement: false } as const] : [],
    { action: "resume", requiresAcknowledgement: false }];
}
