import type { ComboboxOption } from "@/components/ui/combobox";
import type { PublicAiInferenceRoute, PublicAiInternalInferenceCandidate } from "@/types";

// Identify an exact route without interpreting display labels or parsing an encoded key.
// 标识精确路由，不解释展示标签或解析编码键。
// route contains declared ownership and binding; returns a stable picker value.
// route 包含已声明归属与绑定；返回稳定选择器值。
export function vceInferenceRouteKey(route: PublicAiInferenceRoute): string {
  // Binding variants are explicitly declared by the shared inference contract.
  // 绑定分支由共享推理契约明确声明。
  const binding = route.binding.kind === "protocol"
    ? `protocol:${route.binding.protocol_profile_id}` : `action:${route.binding.action_binding_id}`;
  return [route.provider_id, route.service_entry_id, route.connection_id, route.model_id,
    binding, route.operation, route.usage_scope].join("\u0000");
}

// Build common searchable options from backend-validated route candidates.
// 从后端已验证路由候选构建通用可搜索选项。
// candidates supply model/account/provider metadata; returns exact-identity picker options.
// candidates 提供模型、账号及供应商元数据；返回精确身份选择器选项。
export function vceRoutePickerOptions(candidates: PublicAiInternalInferenceCandidate[]): ComboboxOption[] {
  return candidates.map((candidate) => {
    // Binding identity remains searchable without becoming a presentation-based routing shortcut.
    // 绑定身份保持可搜索，但不会成为按展示名称路由的捷径。
    const bindingId = candidate.route.binding.kind === "protocol"
      ? candidate.route.binding.protocol_profile_id : candidate.route.binding.action_binding_id;
    return { value: vceInferenceRouteKey(candidate.route),
      label: `${candidate.modelDisplayName} · ${candidate.connectionDisplayName}`,
      description: `${candidate.providerDisplayName} · ${candidate.serviceEntryDisplayName}`,
      keywords: [candidate.route.model_id, candidate.providerDisplayName, candidate.route.provider_id,
        candidate.connectionDisplayName, candidate.route.connection_id, candidate.serviceEntryDisplayName,
        candidate.route.service_entry_id, bindingId] };
  });
}
